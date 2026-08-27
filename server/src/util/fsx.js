import fs from 'node:fs';
import path from 'node:path';

/** 路径统一用正斜杠存储（Windows 下也如此），FS 操作前再还原 */
export function normPath(p) {
  return p.split(path.sep).join('/');
}

/** 防止路径穿越：确保目标仍在根目录内 */
export function assertInside(root, target) {
  const rel = path.relative(root, target);
  if (rel === '') return;
  if (rel.startsWith('..') || path.isAbsolute(rel)) {
    const err = new Error('非法路径');
    err.code = 'EPERM';
    throw err;
  }
}

/** 若重名则追加 (1)、(2)…，返回可用且唯一的路径 */
export function uniquePath(filePath) {
  if (!fs.existsSync(filePath)) return filePath;
  const dir = path.dirname(filePath);
  const ext = path.extname(filePath);
  const base = path.basename(filePath, ext);
  for (let i = 1; ; i++) {
    const candidate = path.join(dir, `${base} (${i})${ext}`);
    if (!fs.existsSync(candidate)) return candidate;
  }
}

const MIME_MAP = {
  '.txt': 'text/plain', '.md': 'text/markdown', '.json': 'application/json',
  '.html': 'text/html', '.htm': 'text/html', '.css': 'text/css', '.js': 'text/javascript',
  '.mjs': 'text/javascript', '.cjs': 'text/javascript', '.ts': 'text/typescript',
  '.xml': 'application/xml', '.csv': 'text/csv', '.log': 'text/plain', '.ini': 'text/plain',
  '.yml': 'text/yaml', '.yaml': 'text/yaml', '.pdf': 'application/pdf',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif',
  '.webp': 'image/webp', '.svg': 'image/svg+xml', '.bmp': 'image/bmp', '.ico': 'image/x-icon',
  '.avif': 'image/avif', '.heic': 'image/heic',
  '.mp4': 'video/mp4', '.webm': 'video/webm', '.mkv': 'video/x-matroska',
  '.mov': 'video/quicktime', '.avi': 'video/x-msvideo', '.m4v': 'video/x-m4v',
  '.mp3': 'audio/mpeg', '.wav': 'audio/wav', '.flac': 'audio/flac', '.aac': 'audio/aac',
  '.ogg': 'audio/ogg', '.m4a': 'audio/mp4', '.opus': 'audio/opus',
  '.zip': 'application/zip', '.rar': 'application/vnd.rar', '.7z': 'application/x-7z-compressed',
  '.tar': 'application/x-tar', '.gz': 'application/gzip', '.bz2': 'application/x-bzip2',
  '.xz': 'application/x-xz', '.torrent': 'application/x-bittorrent',
  '.exe': 'application/x-msdownload', '.msi': 'application/x-msi', '.dll': 'application/x-msdownload',
  '.apk': 'application/vnd.android.package-archive', '.iso': 'application/x-iso9660-image',
  '.doc': 'application/msword', '.docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  '.xls': 'application/vnd.ms-excel', '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  '.ppt': 'application/vnd.ms-powerpoint', '.pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
};

export function mimeOf(name) {
  const ext = path.extname(name).toLowerCase();
  return MIME_MAP[ext] || 'application/octet-stream';
}

export function isPreviewable(mime) {
  return mime.startsWith('image/') || mime.startsWith('video/') || mime.startsWith('audio/') || mime === 'text/plain' || mime === 'text/markdown';
}

/**
 * 目录扫描：返回 [{name, isDir, size}]，跳过隐藏系统文件。
 * 性能说明：目录 size 恒为 0 —— 不递归计算子树大小。
 * 目录大小此前是每次浏览都同步递归整棵子树（列表越深越慢，事件循环被阻塞），
 * 而 UI 对目录只显示「目录/—」，从不展示大小，纯属浪费。
 */
export function scanDir(dirPath) {
  const out = [];
  let entries;
  try {
    entries = fs.readdirSync(dirPath, { withFileTypes: true });
  } catch {
    return out;
  }
  const statCache = fs.statSync.bind(fs);
  for (const e of entries) {
    if (e.name.startsWith('.')) continue;
    let size = 0;
    if (e.isFile()) {
      try { size = statCache(path.join(dirPath, e.name)).size; } catch { size = 0; }
    }
    out.push({ name: e.name, isDir: e.isDirectory(), size });
  }
  return out.sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name, 'zh') : a.isDir ? -1 : 1));
}

/** 递归收集目录下所有文件的绝对路径 */
export function walkFiles(dir, list = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, list);
    else if (e.isFile()) list.push(p);
  }
  return list;
}
