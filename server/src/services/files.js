import fs from 'node:fs';
import path from 'node:path';
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
  const upsert = db.prepare(`
    INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(path) DO UPDATE SET
      name = excluded.name,
      is_dir = excluded.is_dir,
      size = excluded.size,
      mime = excluded.mime,
      -- 仅在内容元数据实际变化时刷新修改时间（浏览目录不应更新时间）
      updated_at = CASE
        WHEN files.size != excluded.size OR files.is_dir != excluded.is_dir OR files.mime != excluded.mime
        THEN excluded.updated_at ELSE files.updated_at END
  `);
  for (const e of entries) {
    const p = path.join(dirPath, e.name);
    seen.add(normPath(p));
    upsert.run(e.name, parentId == null ? null : Number(parentId), e.isDir ? 1 : 0, normPath(p), e.size, e.isDir ? '' : mimeOf(e.name), now(), now());
  }
  // 删除磁盘上已不存在的直接子项（含其递归后代）
  for (const r of existing) {
    if (!seen.has(r.path)) {
      db.prepare('DELETE FROM files WHERE id = ?').run(r.id);
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

/** 删除（目录递归） */
export function remove(db, id) {
  const row = getRow(db, id);
  if (!row) {
    const err = new Error('文件不存在');
    err.code = 'ENOENT';
    throw err;
  }
  assertInside(STORAGE_DIR, row.path);
  fs.rmSync(row.path, { recursive: true, force: true });
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

/** 计算目录树（用于移动对话框） */
export function tree(db, parentId = null) {
  const rows = childRows(db, parentId).filter(r => r.is_dir);
  return rows.map(r => ({
    id: r.id, name: r.name, path: r.path,
    children: tree(db, r.id),
  }));
}
