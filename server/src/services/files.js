import fs from 'node:fs';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { STORAGE_DIR, OFFLINE_DIR } from '../config.js';
import { normPath, assertInside, uniquePath, mimeOf, scanDir } from '../util/fsx.js';

/**
 * 文件服务：文件系统为事实源，SQLite 只存元数据（派生缓存）。
 * 列目录时对磁盘扫描并 upsert，保证缓存不漂移。
 */

function now() {
  return new Date().toISOString();
}

/** 根目录行：以 parent_id 为 NULL 表示存储根下的条目 */
export function toDto(row) {
  return row ? { ...row, is_dir: !!row.is_dir } : null;
}

export function childRows(db, parentId) {
  const rows = parentId == null || parentId === 'root'
    ? db.prepare('SELECT * FROM files WHERE parent_id IS NULL').all()
    : db.prepare('SELECT * FROM files WHERE parent_id = ?').all(Number(parentId));
  return rows.map(toDto);
}

export function getRow(db, id) {
  return db.prepare('SELECT * FROM files WHERE id = ?').get(Number(id));
}

/** 由 id 解析磁盘绝对路径；id 为 'root' 时返回存储根（不存在时自动创建） */
export function resolveDirPath(db, id) {
  let p;
  if (id == null || id === 'root') {
    p = STORAGE_DIR;
  } else {
    const row = getRow(db, id);
    if (!row || !row.is_dir) {
      const err = new Error('目录不存在');
      err.code = 'ENOENT';
      throw err;
    }
    p = row.path;
  }
  fs.mkdirSync(p, { recursive: true });
  return p;
}

/** 列目录：扫描磁盘并同步缓存，返回子项 */
export function listDir(db, parentId) {
  const dirPath = resolveDirPath(db, parentId);
  if (!fs.existsSync(dirPath)) {
    // 磁盘目录被外部删除：清理缓存后当作空目录
    pruneMissing(db, dirPath);
    return [];
  }
  const entries = scanDir(dirPath).filter(e => normPath(path.join(dirPath, e.name)) !== normPath(OFFLINE_DIR));
  const existing = childRows(db, parentId);
  const seen = new Set();
  // 性能：DO UPDATE 带 WHERE 条件 —— 磁盘元数据未变化时整行跳过（不产生 SQLite 写入）。
  // 此前每次浏览都会对每个条目重写一行（WAL 下也有开销），大目录逐次浏览成本线性累积。
  const upsert = db.prepare(`
    INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(path) DO UPDATE SET
      name = excluded.name,
      is_dir = excluded.is_dir,
      size = excluded.size,
      mime = excluded.mime,
      updated_at = excluded.updated_at
    WHERE files.name != excluded.name
       OR files.is_dir != excluded.is_dir
       OR files.size != excluded.size
       OR files.mime != excluded.mime
  `);
  const del = db.prepare('DELETE FROM files WHERE id = ?');
  const ts = now();
  for (const e of entries) {
    const p = path.join(dirPath, e.name);
    const key = normPath(p);
    seen.add(key);
    upsert.run(e.name, parentId == null ? null : Number(parentId), e.isDir ? 1 : 0, key, e.size, e.isDir ? '' : mimeOf(e.name), ts, ts);
  }
  // 删除磁盘上已不存在的直接子项（含其递归后代）
  for (const r of existing) {
    if (!seen.has(r.path)) {
      del.run(r.id);
    }
  }
  return childRows(db, parentId);
}

function pruneMissing(db, dirPath) {
  const rows = db.prepare('SELECT * FROM files WHERE path = ? OR path LIKE ?').all(normPath(dirPath), normPath(dirPath) + '/%');
  for (const r of rows) {
    if (!fs.existsSync(r.path)) db.prepare('DELETE FROM files WHERE id = ?').run(r.id);
  }
}

/** 跨卷移动（EXDEV 时降级为复制+删除）；同卷 renameSync 原子移动 */
function movePath(src, dest) {
  try {
    fs.renameSync(src, dest);
  } catch (err) {
    if (err.code === 'EXDEV') {
      fs.copyFileSync(src, dest);
      fs.rmSync(src, { force: true });
    } else {
      throw err;
    }
  }
}

/**
 * 迁移存储根目录：把 fromDir 下已登记的文件/目录搬到 toDir（保留相对结构），
 * 并同步更新 DB 中所有条目的 path。offline 暂存目录不属于文件树，不迁移。
 * 返回迁移的文件数。
 */
export function moveStorageDir(db, fromDir, toDir) {
  const oldRoot = normPath(fromDir);
  const newRoot = normPath(toDir);
  if (oldRoot === newRoot) return 0;
  fs.mkdirSync(newRoot, { recursive: true });

  // 目录行先建结构（ORDER BY is_dir DESC 确保父目录先处理），文件再移动
  const rows = db.prepare('SELECT * FROM files ORDER BY is_dir DESC, id ASC').all();
  let moved = 0;
  for (const r of rows) {
    const rel = path.relative(fromDir, r.path);
    if (rel.startsWith('..') || path.isAbsolute(rel)) continue; // 存储根之外的条目不动
    const dest = normPath(path.join(newRoot, rel));
    if (r.is_dir) {
      fs.mkdirSync(dest, { recursive: true });
    } else if (fs.existsSync(r.path)) {
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      movePath(r.path, dest);
      moved += 1;
    }
    db.prepare('UPDATE files SET path = ? WHERE id = ?').run(dest, r.id);
  }
  return moved;
}

/** 新建目录 */
export function mkdir(db, name, parentId) {
  const parentPath = resolveDirPath(db, parentId);
  if (!/^[^\\/:*?"<>|]{1,255}$/.test(name)) {
    const err = new Error('目录名不合法');
    err.code = 'EINVAL';
    throw err;
  }
  const dirPath = uniquePath(path.join(parentPath, name));
  fs.mkdirSync(dirPath, { recursive: false });
  const row = {
    name: path.basename(dirPath), parent_id: parentId == null ? null : Number(parentId),
    is_dir: 1, path: normPath(dirPath), size: 0, mime: '',
  };
  const info = db.prepare(`
    INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `).run(row.name, row.parent_id, 1, row.path, 0, '', now(), now());
  return toDto(getRow(db, info.lastInsertRowid));
}

/** 重命名 / 移动（PATCH body: {name?, parent?}） */
export function rename(db, id, patch) {
  const row = getRow(db, id);
  if (!row) {
    const err = new Error('文件不存在');
    err.code = 'ENOENT';
    throw err;
  }
  let targetDir = path.dirname(row.path);
  if (patch.parent !== undefined) {
    // 禁止把目录移动到自身或其子目录（会形成循环）
    if (row.is_dir && patch.parent != null) {
      const descendants = new Set(collectDescendants(db, row.id).map(d => d.id));
      descendants.add(row.id);
      if (descendants.has(Number(patch.parent))) {
        const err = new Error('不能移动到自身或其子目录');
        err.code = 'EINVAL';
        throw err;
      }
    }
    targetDir = resolveDirPath(db, patch.parent);
  }
  const name = patch.name ?? row.name;
  if (!/^[^\\/:*?"<>|]{1,255}$/.test(name)) {
    const err = new Error('名称不合法');
    err.code = 'EINVAL';
    throw err;
  }
  const newPath = uniquePath(path.join(targetDir, name));
  assertInside(STORAGE_DIR, newPath);
  fs.renameSync(row.path, newPath);
  const parentId = patch.parent !== undefined
    ? (patch.parent == null ? null : Number(patch.parent))
    : row.parent_id;
  db.prepare('UPDATE files SET name = ?, parent_id = ?, path = ?, updated_at = ? WHERE id = ?')
    .run(name, parentId, normPath(newPath), now(), row.id);
  if (row.is_dir && row.path !== normPath(newPath)) {
    // 递归修正后代路径
    const descendants = collectDescendants(db, row.id);
    for (const d of descendants) {
      const rel = d.path.slice(row.path.length);
      const newChild = normPath(newPath) + rel;
      db.prepare('UPDATE files SET path = ? WHERE id = ?').run(newChild, d.id);
    }
  }
  return toDto(getRow(db, row.id));
}

function collectDescendants(db, id) {
  const out = [];
  const rows = db.prepare('SELECT id, path FROM files WHERE parent_id = ?').all(id);
  for (const r of rows) {
    out.push(r);
    out.push(...collectDescendants(db, r.id));
  }
  return out;
}

/** 短等待（Windows 释放文件锁需要时间） */
function await_short() {
  return new Promise(r => setTimeout(r, 300));
}

/**
 * 移动到 Windows 回收站（PowerShell + VisualBasic FileIO，原生 API 不支持回收站）。
 * 文件/目录均可；失败时抛错（由调用方决定是否回退永久删除）。
 * 异步执行：execFileSync 会阻塞整个事件循环（PowerShell 启动慢则全应用卡顿），
 * 改为异步后删除期间下载进度轮询 / 健康检查 / 其他请求照常响应。
 */
function moveToRecycleBin(target) {
  const esc = String(target).replace(/'/g, "''");
  const script = [
    'Add-Type -AssemblyName Microsoft.VisualBasic',
    `$p = '${esc}'`,
    'if (Test-Path -LiteralPath $p) {',
    '  $item = Get-Item -LiteralPath $p',
    '  if ($item.PSIsContainer) {',
    '    [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, \'OnlyErrorDialogs\', \'SendToRecycleBin\')',
    '  } else {',
    '    [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, \'OnlyErrorDialogs\', \'SendToRecycleBin\')',
    '  }',
    '}',
  ].join('; ');
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', ['-NoProfile', '-STA', '-Command', script], {
      timeout: 60000,
      windowsHide: true,
      maxBuffer: 1 * 1024 * 1024,
    }, err => (err ? reject(err) : resolve()));
  });
}

/** 删除（目录递归）。默认进 Windows 回收站（可恢复）；回收站不可用时回退永久删除 */
export async function remove(db, id) {
  const row = getRow(db, id);
  if (!row) {
    const err = new Error('文件不存在');
    err.code = 'ENOENT';
    throw err;
  }
  assertInside(STORAGE_DIR, row.path);
  // 1) 优先进回收站（可恢复，符合本地网盘预期）
  try {
    await moveToRecycleBin(row.path);
  } catch (recycleErr) {
    // 2) 回收站失败（极端环境）→ 退化为永久删除；仍失败则给友好错误
    try {
      fs.rmSync(row.path, { recursive: true, force: true });
    } catch (err) {
      if (err.code === 'EBUSY' || err.code === 'EPERM') {
        await await_short();
        try {
          fs.rmSync(row.path, { recursive: true, force: true });
        } catch (err2) {
          if (err2.code === 'EBUSY' || err2.code === 'EPERM') {
            const e = new Error('文件正被占用（可能正在预览或下载中），请稍后重试');
            e.code = 'EBUSY';
            throw e;
          }
          throw err2;
        }
      } else {
        throw err;
      }
    }
  }
  db.prepare('DELETE FROM files WHERE id = ?').run(row.id);
  return { ok: true };
}

/** 上传文件落盘并登记；返回登记的元数据行 */
export function registerUpload(db, parentId, fileInfo) {
  const parentPath = resolveDirPath(db, parentId);
  const dest = uniquePath(path.join(parentPath, fileInfo.originalname));
  assertInside(STORAGE_DIR, dest);
  fs.renameSync(fileInfo.path, dest);
  const stat = fs.statSync(dest);
  const info = db.prepare(`
    INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
    VALUES (?, ?, 0, ?, ?, ?, ?, ?)
  `).run(path.basename(dest), parentId == null ? null : Number(parentId), normPath(dest), stat.size, mimeOf(dest), now(), now());
  return toDto(getRow(db, info.lastInsertRowid));
}

/** 全文搜索（名称模糊匹配，含目录） */
export function search(db, q, limit = 50) {
  const like = `%${q}%`;
  return db.prepare(`
    SELECT * FROM files WHERE name LIKE ? ORDER BY is_dir DESC, name LIMIT ?
  `).all(like, limit).map(toDto);
}

/** 计算目录树（用于移动对话框）。单次查询全量目录 + 内存组装，避免逐层 N+1 查询 */
export function tree(db, parentId = null) {
  const dirs = db.prepare('SELECT id, name, path, parent_id FROM files WHERE is_dir = 1 ORDER BY name').all();
  const byParent = new Map();
  for (const r of dirs) {
    const p = r.parent_id == null ? null : Number(r.parent_id);
    if (!byParent.has(p)) byParent.set(p, []);
    byParent.get(p).push(r);
  }
  const build = pid => (byParent.get(pid) || []).map(r => ({
    id: r.id, name: r.name, path: r.path,
    children: build(r.id),
  }));
  return build(parentId == null ? null : Number(parentId));
}

/** 祖先链（含自身，根 → 目标），一次性查询，供面包屑使用 */
export function ancestors(db, id) {
  const out = [];
  let row = getRow(db, Number(id));
  if (!row) return null;
  while (row) {
    out.unshift(toDto(row));
    row = row.parent_id == null ? null : getRow(db, row.parent_id);
  }
  return out;
}
