import express from 'express';
import multer from 'multer';
import fs from 'node:fs';
import path from 'node:path';
import { GOPEED_DIR, DATA_DIR, getStorageDir, setStorageDir, resolveDefaultStorageDir, ensureDirs } from './config.js';
import * as fileSvc from './services/files.js';
import { TMP_TORRENT_DIR } from './services/tasks.js';
import * as ucSvc from './services/uc.js';
import { getUcCookie, setUcCookie, hasUcCookie, getSetting, setSetting } from './services/cookie.js';
import { isPreviewable, normPath } from './util/fsx.js';

/**
 * Express 应用工厂。仅监听 127.0.0.1，无鉴权（单用户本地网盘）。
 */

function parseRange(rangeHeader, size) {
  if (!rangeHeader) return null;
  const m = /^bytes=(\d*)-(\d*)$/.exec(rangeHeader.trim());
  if (!m) return null;
  let start = m[1] === '' ? undefined : Number(m[1]);
  let end = m[2] === '' ? undefined : Number(m[2]);
  if (start === undefined && end === undefined) return null;
  if (start === undefined) {
    // 末尾 N 字节
    start = Math.max(0, size - end);
    end = size - 1;
  } else {
    if (end === undefined) end = size - 1;
    end = Math.min(end, size - 1);
  }
  if (start > end || start >= size) return { invalid: true };
  return { start, end };
}

export function createApp({ db, gopeed, tasks }) {
  const app = express();
  app.use(express.json({ limit: '2mb' }));

  // CORS：前端页面 origin 为 tauri.localhost（WebView），请求本机后端属跨域。
  // 缺此头时 WebView 会拦截所有 fetch（表现为 Failed to fetch / 无限重试转圈）。
  app.use((req, res, next) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET,POST,PUT,PATCH,DELETE,OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type,Range');
    res.setHeader('Access-Control-Expose-Headers', 'Content-Range,Content-Length,Accept-Ranges');
    if (req.method === 'OPTIONS') return res.sendStatus(204);
    next();
  });

  // 诊断：访问日志（%APPDATA%/uc-drive2/access.log，便于排查前端请求是否到达）
  const ACCESS_LOG = path.join(DATA_DIR, 'access.log');
  app.use((req, _res, next) => {
    try {
      fs.appendFileSync(ACCESS_LOG, `${new Date().toISOString()} ${req.method} ${req.originalUrl}\n`);
    } catch { /* 日志失败不影响服务 */ }
    next();
  });

  const upload = multer({
    storage: multer.diskStorage({
      destination: (_req, _file, cb) => {
        const dir = path.join(DATA_DIR, 'tmp');
        fs.mkdirSync(dir, { recursive: true });
        cb(null, dir);
      },
      filename: (_req, file, cb) => cb(null, `${Date.now()}-${file.originalname}`),
    }),
    defParamCharset: 'utf8',
    limits: { fileSize: 1024 * 1024 * 1024 * 4, files: 100 },
  });

  // ---------- 健康 & 设置 ----------
  app.get('/api/health', (_req, res) => {
    res.json({ ok: true, gopeed: gopeed.ready, version: '1.1.0' });
  });

  function settingsPayload() {
    return {
      storageDir: getStorageDir(),
      defaultStorageDir: resolveDefaultStorageDir(),
      dataDir: DATA_DIR,
      gopeedDir: GOPEED_DIR,
      gopeed: { running: gopeed.ready, port: gopeed.port, base: gopeed.base },
      download: tasks.getConfig(),
    };
  }

  app.get('/api/settings', (_req, res) => {
    res.json(settingsPayload());
  });

  // 切换网盘存储目录（用户自定义，持久化到 settings 表，重启后恢复）
  // body: { dir: string, moveFiles?: boolean }；dir 为空字符串 = 恢复默认目录
  app.put('/api/settings/storage-dir', (req, res, next) => {
    const { dir, moveFiles } = req.body || {};
    const raw = typeof dir === 'string' ? dir.trim() : '';
    try {
      const current = getStorageDir();
      const target = raw ? path.resolve(raw) : resolveDefaultStorageDir();
      if (normPath(target) === normPath(current)) {
        return res.json({ ...settingsPayload(), changed: false });
      }
      // 目标目录必须可创建、可写（写探针避免选到只读/网络异常目录）
      fs.mkdirSync(target, { recursive: true });
      const probe = path.join(target, `.ucd2-write-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
      fs.writeFileSync(probe, 'ok');
      fs.rmSync(probe, { force: true });

      // 默认把当前存储根下已登记的文件搬到新目录（保留相对结构，支持跨盘）
      let movedFiles = 0;
      if (moveFiles !== false) {
        movedFiles = fileSvc.moveStorageDir(db, current, target);
      }
      setSetting(db, 'storage_dir', raw);
      setStorageDir(raw);
      ensureDirs();
      res.json({ ...settingsPayload(), changed: true, movedFiles });
    } catch (err) {
      next(err);
    }
  });

  // ---------- 文件 ----------
  app.get('/api/files', (req, res) => {
    const { parent } = req.query;
    res.json(fileSvc.listDir(db, parent == null || parent === 'root' ? null : Number(parent)));
  });

  app.get('/api/files/:id', (req, res) => {
    const row = fileSvc.getRow(db, req.params.id);
    if (!row) return res.status(404).json({ error: '文件不存在' });
    res.json(fileSvc.toDto(row));
  });

  app.get('/api/tree', (_req, res) => {
    res.json(fileSvc.tree(db));
  });

  app.get('/api/search', (req, res) => {
    const q = String(req.query.q || '').trim();
    if (!q) return res.json([]);
    res.json(fileSvc.search(db, q));
  });

  app.post('/api/files', upload.array('files', 100), (req, res) => {
    if (!req.files || req.files.length === 0) {
      return res.status(400).json({ error: '未收到文件' });
    }
    const parent = req.body.parent == null || req.body.parent === 'root' ? null : Number(req.body.parent);
    const rows = req.files.map(f => fileSvc.registerUpload(db, parent, f));
    res.json(rows.map(fileSvc.toDto));
  });

  app.get('/api/files/:id/download', (req, res) => {
    const row = fileSvc.getRow(db, req.params.id);
    if (!row) return res.status(404).json({ error: '文件不存在' });
    if (row.is_dir) return res.status(400).json({ error: '目录不可下载' });
    const abs = row.path;
    if (!fs.existsSync(abs)) return res.status(404).json({ error: '文件在磁盘上不存在' });
    const stat = fs.statSync(abs);
    const preview = req.query.preview === '1' || isPreviewable(row.mime);
    const disposition = preview ? 'inline' : 'attachment';
    const encoded = encodeURIComponent(path.basename(abs));
    res.setHeader('Content-Type', row.mime || 'application/octet-stream');
    res.setHeader('Accept-Ranges', 'bytes');
    res.setHeader('Content-Disposition', `${disposition}; filename*=UTF-8''${encoded}`);
    res.setHeader('Cache-Control', 'private, max-age=3600');

    const range = parseRange(req.headers.range, stat.size);
    if (range?.invalid) {
      res.setHeader('Content-Range', `bytes */${stat.size}`);
      return res.status(416).end();
    }
    if (range) {
      const { start, end } = range;
      res.status(206);
      res.setHeader('Content-Range', `bytes ${start}-${end}/${stat.size}`);
      res.setHeader('Content-Length', end - start + 1);
      fs.createReadStream(abs, { start, end }).pipe(res);
    } else {
      res.setHeader('Content-Length', stat.size);
      fs.createReadStream(abs).pipe(res);
    }
  });

  app.post('/api/dirs', (req, res) => {
    const { name, parent } = req.body || {};
    if (!name) return res.status(400).json({ error: '缺少目录名' });
    const row = fileSvc.mkdir(db, String(name), parent == null || parent === 'root' ? null : Number(parent));
    res.status(201).json(fileSvc.toDto(row));
  });

  app.patch('/api/files/:id', (req, res) => {
    const { name, parent } = req.body || {};
    if (name === undefined && parent === undefined) {
      return res.status(400).json({ error: '没有可更新的字段' });
    }
    const row = fileSvc.rename(db, req.params.id, {
      ...(name !== undefined ? { name: String(name) } : {}),
      ...(parent !== undefined ? { parent: parent == null || parent === 'root' ? null : Number(parent) } : {}),
    });
    res.json(fileSvc.toDto(row));
  });

  app.delete('/api/files/:id', (req, res) => {
    res.json(fileSvc.remove(db, req.params.id));
  });

  // ---------- 离线任务 ----------
  // 临时 torrent 上传：不入文件树，任务创建后由后端清理
  app.post('/api/tmp-files', upload.single('file'), (req, res) => {
    if (!req.file) return res.status(400).json({ error: '未收到文件' });
    if (!req.file.originalname.toLowerCase().endsWith('.torrent')) {
      fs.rmSync(req.file.path, { force: true });
      return res.status(400).json({ error: '仅支持 .torrent 文件' });
    }
    fs.mkdirSync(TMP_TORRENT_DIR, { recursive: true });
    const dest = path.join(TMP_TORRENT_DIR, path.basename(req.file.filename));
    fs.renameSync(req.file.path, dest);
    res.status(201).json({ name: path.basename(dest) });
  });

  app.post('/api/tasks', async (req, res) => {
    const { source = 'url', url, torrentId, torrentName, connections } = req.body || {};
    const task = await tasks.create({ source, url, torrentId, torrentName, connections });
    res.status(201).json(task);
  });

  // 下载参数（并发连接数 / 同时下载任务数），持久化到 DB 并应用到 gopeed
  app.get('/api/tasks/config', (_req, res) => {
    res.json(tasks.getConfig());
  });

  app.put('/api/tasks/config', async (req, res) => {
    const { ucConnections, httpConnections, maxRunning } = req.body || {};
    if (ucConnections === undefined && httpConnections === undefined && maxRunning === undefined) {
      return res.status(400).json({ error: '没有可更新的参数' });
    }
    res.json(await tasks.setConfig({ ucConnections, httpConnections, maxRunning }));
  });

  app.get('/api/tasks', (_req, res) => {
    res.json(tasks.list());
  });

  app.get('/api/tasks/:id', (req, res) => {
    const row = tasks.get(req.params.id);
    if (!row) return res.status(404).json({ error: '任务不存在' });
    res.json(row);
  });

  app.post('/api/tasks/:id/pause', async (req, res) => {
    res.json(await tasks.pause(req.params.id));
  });

  app.post('/api/tasks/:id/resume', async (req, res) => {
    res.json(await tasks.resume(req.params.id));
  });

  app.post('/api/tasks/:id/delete', async (req, res) => {
    const force = !!(req.body && req.body.force);
    res.json(await tasks.remove(req.params.id, force));
  });

  // ---------- UC 网盘解析 ----------
  app.post('/api/uc/parse', async (req, res) => {
    const { shareLink, cookie } = req.body || {};
    if (!shareLink || typeof shareLink !== 'string') {
      return res.status(400).json({ error: '缺少分享链接' });
    }
    const savedCookie = getUcCookie(db) || '';
    const parsed = await ucSvc.parse(shareLink, cookie || savedCookie);
    res.json({ ...parsed, cookieUsed: Boolean(cookie || savedCookie) });
  });

  app.post('/api/uc/list-folder', async (req, res) => {
    const { shareId, pdirFid, session } = req.body || {};
    if (!shareId || !session?.stoken) return res.status(400).json({ error: '缺少目录参数' });
    const files = await ucSvc.listFolder(shareId, session.stoken, pdirFid || null, session.ctoken || '', session.cookies || '');
    res.json({ files });
  });

  // 创建 UC 下载任务：解析直链 → gopeed 下载（带 UC headers，登记元数据供过期刷新）
  app.post('/api/uc/download', async (req, res) => {
    const { shareId, stoken, fid, shareFidToken, filename, size, ctoken, cookies, shareLink, connections } = req.body || {};
    if (!shareId || !fid) return res.status(400).json({ error: '缺少下载参数' });
    const url = await ucSvc.resolveDownload({ shareId, stoken, fid, shareFidToken, ctoken, cookies });
    // 预检直链：UC 直链带 OSS 登录回调，Cookie 失效/链接签名错误会 403 且 2 秒内就失败。
    // 创建任务前先带 Cookie 拉 4KB 验证，把真实原因直接抛给前端（而非笼统「下载失败」）。
    const probe = await ucSvc.probeDownloadUrl(url, cookies || '');
    if (probe.kind === 'cookie_expired') {
      return res.status(403).json({ error: 'UC Cookie 已失效，请在设置中更新后重试', kind: 'cookie_expired' });
    }
    if (probe.kind === 'url_invalid') {
      return res.status(502).json({ error: '直链校验失败（签名无效），请重试', kind: 'url_invalid' });
    }
    // probe.network / probe.http：瞬时问题，交给 gopeed 引擎处理（可能重试成功）
    const headers = {
      'Cookie': cookies || '',
      'User-Agent': ucSvc.UA,
      'Referer': 'https://drive.uc.cn/',
      'Origin': 'https://drive.uc.cn',
      'x-csrf-token': ctoken || '',
    };
    const uc = {
      shareId, stoken, fid, shareFidToken, shareLink,
      filename: filename || '', size: Number(size) || 0,
      retryCount: 0, lastRefreshAt: 0,
    };
    const task = await tasks.create({
      source: 'uc', url, filename: filename || undefined, headers, uc, connections,
    });
    res.status(201).json(task);
  });

  // ---------- UC Cookie ----------
  app.get('/api/cookie', (_req, res) => {
    res.json({ hasCookie: hasUcCookie(db) });
  });

  app.put('/api/cookie', (req, res) => {
    const { cookie } = req.body || {};
    if (typeof cookie !== 'string' || !cookie.trim()) {
      return res.status(400).json({ error: 'Cookie 不能为空' });
    }
    setUcCookie(db, cookie);
    res.json({ ok: true });
  });

  app.delete('/api/cookie', (_req, res) => {
    db.prepare('DELETE FROM settings WHERE key = \'uc_cookie\'').run();
    res.json({ ok: true });
  });

  // ---------- 历史记录 ----------
  app.get('/api/history', (_req, res) => {
    res.json(tasks.history());
  });

  app.delete('/api/history', (_req, res) => {
    res.json(tasks.clearHistory());
  });

  // ---------- 错误处理 ----------
  app.use((err, _req, res, _next) => {
    if (err?.code === 'ENOENT') return res.status(404).json({ error: err.message || '不存在' });
    if (err?.code === 'EBUSY') return res.status(400).json({ error: err.message || '文件正被占用，请稍后重试' });
    if (err?.code === 'EPERM' || err?.code === 'EINVAL' || err?.code === 'EACCES') return res.status(400).json({ error: err.message });
    if (err?.code === 'LIMIT_FILE_SIZE') return res.status(413).json({ error: '文件过大' });
    res.status(500).json({ error: err?.message || '服务器错误' });
  });

  return app;
}
