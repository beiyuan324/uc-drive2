import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { OFFLINE_DIR, STORAGE_DIR, DATA_DIR } from '../config.js';
import { normPath, walkFiles, mimeOf, uniquePath } from '../util/fsx.js';

/** 临时 torrent 上传目录（任务创建后即清理） */
export const TMP_TORRENT_DIR = path.join(DATA_DIR, 'tmp', 'torrents');

/**
 * 统计任务目录中文件的 NTFS 有效数据长度（真实写入字节，排除稀疏文件预分配）。
 * gopeed 下载时预分配完整文件大小，fs.statSync().size 会虚高；
 * 用 fsutil queryValidData 拿真实有效数据。非 Windows 或失败时回退 stat().size。
 */
function validDataBytes(dir) {
  let files = [];
  try {
    files = walkFiles(dir);
  } catch {
    return 0;
  }
  if (!files.length) return 0;
  let total = 0;
  for (const f of files) {
    if (process.platform === 'win32') {
      try {
        const out = execFileSync('fsutil', ['file', 'queryValidData', f], { encoding: 'utf8', timeout: 3000, windowsHide: true });
        const m = /0x([0-9a-fA-F]+)/.exec(out);
        if (m) { total += parseInt(m[1], 16); continue; }
      } catch { /* 回退 stat */ }
    }
    try {
      total += fs.statSync(f).size;
    } catch { /* 文件可能被删除 */ }
  }
  return total;
}

/**
 * 离线下载编排：tasks 表为任务记录，gopeed 实际执行。
 * 固定下载到 storage/offline/<taskId>/，完成后登记进文件树（根 → 任务名目录 → 文件）。
 */

function now() {
  return new Date().toISOString();
}

export class TaskService {
  constructor(db, gopeed) {
    this.db = db;
    this.gopeed = gopeed;
    gopeed.onEvent(ev => this._onGopeedEvent(ev));
  }

  _onGopeedEvent(ev) {
    if (ev.type === 'tasks') this._syncFromGopeed(ev.tasks).catch(() => {});
    if (ev.type === 'started') this._resumeInterrupted();
  }

  /** gopeed 重启后，把中断任务重新置为运行 */
  async _resumeInterrupted() {
    const rows = this.db.prepare("SELECT * FROM tasks WHERE status IN ('running','queued') AND gopeed_id != ''").all();
    for (const r of rows) {
      try {
        const t = await this.gopeed.getTask(r.gopeed_id);
        if (t && ['ready', 'pause', 'wait'].includes(t.status)) {
          await this.gopeed.resume(r.gopeed_id);
        }
      } catch { /* 任务可能已被删除 */ }
    }
  }

  list() {
    return this.db.prepare('SELECT * FROM tasks ORDER BY id DESC').all()
      .map(r => this._row(r));
  }

  get(id) {
    return this._row(this.db.prepare('SELECT * FROM tasks WHERE id = ?').get(Number(id)));
  }

  _row(r) {
    if (!r) return undefined;
    let metadata = {};
    try { metadata = r.metadata ? JSON.parse(r.metadata) : {}; } catch { /* 忽略 */ }
    return { ...r, metadata, progress: Number(r.progress), speed: Number(r.speed) };
  }

  /** 历史记录：已完成 / 失败 / 已替换 的任务 */
  history() {
    return this.db.prepare(
      "SELECT * FROM tasks WHERE status IN ('done','error','cookie_expired','replaced') ORDER BY updated_at DESC LIMIT 300"
    ).all().map(r => this._row(r));
  }

  clearHistory() {
    const info = this.db.prepare(
      "DELETE FROM tasks WHERE status IN ('done','error','cookie_expired','replaced')"
    ).run();
    return { deleted: Number(info.changes) };
  }

  /**
   * 创建任务。
   * source: url | magnet | torrent | uc
   *   url     → { url }
   *   magnet  → { url: "magnet:?xt=..." }
   *   torrent → { torrentId: 文件树中已上传的 .torrent 文件 id } 或
   *             { torrentName: 临时上传（POST /api/tmp-files）返回的文件名 }
   *   uc      → { url: 已解析直链, filename, headers, uc: shareInfo }
   */
  async create({ source, url, torrentId, torrentName, filename, headers, uc, connections }) {
    let tmpTorrentPath = null;
    let metadata = {};
    if (source === 'uc') {
      metadata = { uc };
    } else if (source === 'torrent') {
      if (torrentName) {
        // 临时目录中的 torrent（任务创建后删除，不残留文件树）
        fs.mkdirSync(TMP_TORRENT_DIR, { recursive: true });
        const candidate = path.join(TMP_TORRENT_DIR, path.basename(torrentName));
        if (!candidate.toLowerCase().endsWith('.torrent') || !fs.existsSync(candidate)) {
          throw new Error('无效的 torrent 文件');
        }
        url = 'file:///' + candidate.replace(/\\/g, '/');
        tmpTorrentPath = candidate;
      } else {
        const row = this.db.prepare('SELECT * FROM files WHERE id = ?').get(Number(torrentId));
        if (!row || !row.path.toLowerCase().endsWith('.torrent')) throw new Error('无效的 torrent 文件');
        url = 'file:///' + row.path.replace(/\\/g, '/');
      }
    }
    if (!url || !/^(https?:\/\/|magnet:|file:\/\/)/i.test(url)) throw new Error('链接不合法');

    const taskDir = path.join(OFFLINE_DIR, `task-${Date.now()}-${Math.floor(Math.random() * 1000)}`);
    fs.mkdirSync(taskDir, { recursive: true });
    const info = this.db.prepare(`
      INSERT INTO tasks (source, source_url, status, target_dir, metadata, created_at, updated_at)
      VALUES (?, ?, 'queued', ?, ?, ?, ?)
    `).run(source, url, normPath(taskDir), JSON.stringify(metadata), now(), now());
    const id = Number(info.lastInsertRowid);
    const taskDirFinal = path.join(OFFLINE_DIR, `task-${id}`);
    fs.renameSync(taskDir, taskDirFinal);
    this.db.prepare('UPDATE tasks SET target_dir = ? WHERE id = ?').run(normPath(taskDirFinal), id);

    try {
      const req = { url };
      const opts = { path: taskDirFinal };
      if (headers && Object.keys(headers).length) req.extra = { header: headers };
      if (filename) opts.name = filename;
      if (connections) opts.extra = { ...(opts.extra || {}), connections };
      const gid = await this.gopeed.createTask(req, opts);
      this.db.prepare('UPDATE tasks SET gopeed_id = ?, status = ?, updated_at = ? WHERE id = ?').run(gid, 'queued', now(), id);
      return this.get(id);
    } catch (err) {
      fs.rmSync(taskDirFinal, { recursive: true, force: true });
      this.db.prepare('UPDATE tasks SET status = ?, error = ?, updated_at = ? WHERE id = ?')
        .run('error', String(err.message || err), now(), id);
      throw err;
    } finally {
      // 临时 torrent 用完即删，不留残留
      if (tmpTorrentPath) fs.rmSync(tmpTorrentPath, { force: true });
    }
  }

  /**
   * UC 直链过期后刷新重试：换新直链 → 替换 gopeed 任务 → 继续下载。
   * 最多重试 5 次，每次间隔 30s；cookie 失效则置 cookie_expired。
   */
  async _refreshUcUrl(r) {
    const uc = r.metadata?.uc;
    if (!uc || !uc.shareId || !uc.fid) return false;
    const retryCount = uc.retryCount || 0;
    if (retryCount >= 5) return false;
    const lastRefresh = uc.lastRefreshAt || 0;
    if (Date.now() - lastRefresh < 30000) return false;

    const { getUcCookie } = await import('./cookie.js');
    const ucSvc = await import('./uc.js');
    const cookie = getUcCookie(this.db);
    if (!cookie) {
      this.db.prepare("UPDATE tasks SET status = 'cookie_expired', error = ?, updated_at = ? WHERE id = ?")
        .run('需要 UC Cookie 才能刷新下载链接', now(), r.id);
      return true;
    }

    try {
      // 重建会话 + 新直链（stoken 失效时重新解析拿新 token）
      const session = await ucSvc.getCtoken(uc.shareLink || `https://drive.uc.cn/s/${uc.shareId}`, cookie);
      let stoken = uc.stoken;
      let shareFidToken = uc.shareFidToken;
      let url;
      try {
        url = await ucSvc.getDownloadUrl(uc.shareId, stoken, uc.fid, shareFidToken, session.ctoken, session.cookies);
      } catch {
        stoken = await ucSvc.getStoken(uc.shareId, session.ctoken, session.cookies);
        const files = await ucSvc.findFiles(uc.shareId, stoken, session.ctoken, session.cookies);
        const fresh = files.find(f => f.fid === uc.fid);
        if (!fresh) throw new Error('分享文件已失效');
        shareFidToken = fresh.share_fid_token;
        url = await ucSvc.getDownloadUrl(uc.shareId, stoken, uc.fid, shareFidToken, session.ctoken, session.cookies);
      }

      // 清掉旧 gopeed 任务（保留已下载文件），重建任务
      try { await this.gopeed.remove(r.gopeed_id, false); } catch { /* 忽略 */ }
      const headers = { 'Cookie': session.cookies, 'User-Agent': ucSvc.UA || undefined, 'Referer': 'https://drive.uc.cn/', 'Origin': 'https://drive.uc.cn', 'x-csrf-token': session.ctoken };
      const req = { url, extra: { header: headers } };
      const opts = { path: r.target_dir, name: uc.filename || undefined };
      const gid = await this.gopeed.createTask(req, opts);

      const meta = { ...r.metadata, uc: { ...uc, stoken, shareFidToken, retryCount: retryCount + 1, lastRefreshAt: Date.now() } };
      this.db.prepare(`UPDATE tasks SET gopeed_id = ?, status = 'queued', error = '', source_url = ?, metadata = ?, updated_at = ? WHERE id = ?`)
        .run(gid, url, JSON.stringify(meta), now(), r.id);
      return true;
    } catch (err) {
      const meta = { ...r.metadata, uc: { ...uc, retryCount: retryCount + 1, lastRefreshAt: Date.now() } };
      this.db.prepare(`UPDATE tasks SET error = ?, metadata = ?, updated_at = ? WHERE id = ?`)
        .run(`刷新链接失败: ${err.message}`, JSON.stringify(meta), now(), r.id);
      return false;
    }
  }

  /** 判断 gopeed 任务错误是否可重试 / 是否 cookie 过期 */
  _classifyUcError(r, g) {
    if (!r.metadata?.uc) return null;
    const msg = String(g.error || '').toLowerCase();
    if (/cookie|require login|authentication failed|login required/i.test(msg)) return 'cookie_expired';
    if (/401|403|404|expired|signature|error code 22/i.test(msg)) return 'retry';
    return null;
  }

  async pause(id) {
    const r = this.get(id);
    if (!r || !r.gopeed_id) throw new Error('任务不存在');
    await this.gopeed.pause(r.gopeed_id);
    this.db.prepare("UPDATE tasks SET status = 'paused', updated_at = ? WHERE id = ?").run(now(), id);
    return this.get(id);
  }

  async resume(id) {
    const r = this.get(id);
    if (!r || !r.gopeed_id) throw new Error('任务不存在');
    await this.gopeed.resume(r.gopeed_id);
    this.db.prepare("UPDATE tasks SET status = 'running', updated_at = ? WHERE id = ?").run(now(), id);
    return this.get(id);
  }

  /** force=true 时同时删除已下载文件与文件树登记 */
  async remove(id, force = false) {
    const r = this.get(id);
    if (!r) throw new Error('任务不存在');
    if (r.gopeed_id) {
      try { await this.gopeed.remove(r.gopeed_id, force); } catch { /* 忽略 */ }
    }
    if (force) {
      fs.rmSync(r.target_dir, { recursive: true, force: true });
      this.db.prepare("DELETE FROM files WHERE path = ? OR path LIKE ?")
        .run(r.target_dir, r.target_dir + '/%');
    }
    this.db.prepare('DELETE FROM tasks WHERE id = ?').run(id);
    return { ok: true };
  }

  /** 由 gopeed 轮询结果同步状态（先登记入树再置 done，登记失败可自动重试） */
  async _syncFromGopeed(tasks) {
    const byId = new Map(tasks.map(t => [t.id, t]));
    const rows = this.db.prepare("SELECT * FROM tasks WHERE gopeed_id != ''").all();
    for (const r of rows) {
      const g = byId.get(r.gopeed_id);
      if (!g) continue;
      const status = this.gopeed.mapStatus(g.status);
      // UC 任务错误分类：cookie 失效 / 直链过期可重试
      if (status === 'error' && r.status !== 'cookie_expired') {
        const kind = this._classifyUcError({ ...r, metadata: this._parseMeta(r) }, g);
        if (kind === 'cookie_expired') {
          this.db.prepare("UPDATE tasks SET status = 'cookie_expired', error = 'UC Cookie 已失效，请在设置中更新', updated_at = ? WHERE id = ?")
            .run(now(), r.id);
          continue;
        }
        if (kind === 'retry') {
          const refreshed = await this._refreshUcUrl({ ...r, metadata: this._parseMeta(r) });
          if (refreshed) continue;
        }
      }
      // 完成且尚未登记：先登记，失败则置 error 等待下轮重试
      if (status === 'done' && r.status !== 'done') {
        try {
          this._registerIntoTree(r.id, g.name || '离线任务');
        } catch (err) {
          this.db.prepare("UPDATE tasks SET status = 'error', error = ?, updated_at = ? WHERE id = ?")
            .run(`文件登记失败: ${err.message || err}`, now(), r.id);
          continue;
        }
      }
      const total = g.size || g.meta?.res?.size || 0;
      // 真实进度：gopeed 的 progress.downloaded 字段不可靠（大文件实测 2GB 时仍显示 0.2MB），
      // 改用 NTFS 有效数据长度（fsutil queryValidData）统计磁盘真实写入字节。
      // done 后文件已登记移出 target_dir，进度固定 100。
      const validBytes = total > 0 && status !== 'done' ? validDataBytes(r.target_dir) : 0;
      const progress = status === 'done' ? 100 : total > 0 ? Math.min(100, Math.round(validBytes / total * 1000) / 10) : 0;
      const speed = status === 'done' ? 0 : g.progress?.speed || 0;
      const patch = { status, progress, speed };
      if (status === 'done') { patch.finished_at = now(); patch.error = ''; }
      if (status === 'error') patch.error = '下载失败';
      this.db.prepare(`
        UPDATE tasks SET status = ?, progress = ?, speed = ?, finished_at = COALESCE(?, finished_at), error = ?, updated_at = ?
        WHERE id = ? AND (status != ? OR progress != ? OR speed != ? OR error != ?)
      `).run(status, progress, speed, patch.finished_at || null, patch.error ?? r.error, now(), r.id, status, progress, speed, patch.error ?? r.error);
    }
  }

  _parseMeta(r) {
    try { return r.metadata ? JSON.parse(r.metadata) : {}; } catch { return {}; }
  }

  /** 任务完成后，把 target_dir 内容登记进文件树 */
  _registerIntoTree(taskId, taskName) {
    const r = this.get(taskId);
    if (!r) return;
    const files = walkFiles(r.target_dir);
    if (files.length === 0) return;

    const single = files.length === 1 && normPath(path.dirname(files[0])) === r.target_dir;
    if (single) {
      // 单文件：直接挂到存储根，名称去重
      const dest = uniquePath(path.join(STORAGE_DIR, path.basename(files[0])));
      fs.renameSync(files[0], dest);
      const rootRow = dbRowForParent(this.db, STORAGE_DIR);
      upsertFileRow(this.db, rootRow ? rootRow.id : null, dest);
    } else {
      // 多文件/目录资源：根下建任务名目录，镜像结构
      const dirName = (taskName || `任务-${taskId}`).replace(/[\\/:*?"<>|]/g, '_') || `任务-${taskId}`;
      const finalDir = uniqueDir(this.db, dirName);
      this.db.prepare(`
        INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
        VALUES (?, NULL, 1, ?, 0, '', ?, ?)
      `).run(path.basename(finalDir), normPath(finalDir), now(), now());
      for (const f of files) {
        const rel = path.relative(r.target_dir, f);
        const dest = path.join(finalDir, rel);
        fs.mkdirSync(path.dirname(dest), { recursive: true });
        fs.renameSync(f, dest);
        upsertFileRow(this.db, null, dest);
      }
    }
    // 清理空壳目录
    fs.rmSync(r.target_dir, { recursive: true, force: true });
    this.db.prepare('UPDATE tasks SET target_dir = ? WHERE id = ?').run(normPath(STORAGE_DIR), taskId);
  }
}

function uniqueDir(db, name) {
  let n = name;
  let i = 1;
  while (db.prepare('SELECT 1 FROM files WHERE path = ?').get(normPath(path.join(STORAGE_DIR, n)))) {
    n = `${name} (${i++})`;
  }
  return path.join(STORAGE_DIR, n);
}

/** 取存储根（或指定目录）对应的父行 id；根无行时返回 null */
function dbRowForParent(db, absPath) {
  return db.prepare('SELECT id FROM files WHERE path = ?').get(normPath(absPath)) || null;
}

function upsertFileRow(db, parentId, absPath) {
  const stat = fs.statSync(absPath);
  const dbPath = normPath(absPath);
  let pid = parentId;
  // 确保父链上的目录行存在（从存储根逐级建行）
  const rel = path.relative(STORAGE_DIR, path.dirname(absPath));
  if (rel && rel !== '..' && !rel.startsWith('..' + path.sep)) {
    let cur = STORAGE_DIR;
    for (const part of rel.split(path.sep)) {
      cur = path.join(cur, part);
      const row = dbRowForParent(db, cur);
      if (row) {
        pid = row.id;
      } else {
        const ins = db.prepare(`
          INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
          VALUES (?, ?, 1, ?, 0, '', ?, ?)
        `).run(part, pid, normPath(cur), now(), now());
        pid = Number(ins.lastInsertRowid);
      }
    }
  }
  const isDir = stat.isDirectory();
  db.prepare(`
    INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(path) DO UPDATE SET
      name = excluded.name, is_dir = excluded.is_dir, size = excluded.size,
      mime = excluded.mime, updated_at = excluded.updated_at
  `).run(path.basename(absPath), pid, isDir ? 1 : 0, dbPath, isDir ? 0 : stat.size, isDir ? '' : mimeOf(absPath), now(), now());
}
