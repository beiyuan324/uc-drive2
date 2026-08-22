import os from 'node:os';
import path from 'node:path';
import fs from 'node:fs';

export const APP_NAME = 'uc-drive2';
/** 后端默认监听端口，被占用时自动 +1 */
export const BASE_PORT = 17210;

/** 用户数据目录：%APPDATA%/uc-drive2（可用 UC_DRIVE2_DATA_DIR 覆盖，测试用） */
function resolveDataDir() {
  if (process.env.UC_DRIVE2_DATA_DIR) return path.resolve(process.env.UC_DRIVE2_DATA_DIR);
  if (process.env.APPDATA) return path.join(process.env.APPDATA, APP_NAME);
  return path.join(os.homedir(), `.${APP_NAME}`);
}

export const DATA_DIR = resolveDataDir();
export const STORAGE_DIR = process.env.UC_DRIVE2_STORAGE_DIR
  ? path.resolve(process.env.UC_DRIVE2_STORAGE_DIR)
  : path.join(DATA_DIR, 'storage');
export const GOPEED_DIR = path.join(DATA_DIR, 'gopeed');
export const DB_DIR = path.join(DATA_DIR, 'data');
export const DB_FILE = path.join(DB_DIR, 'uc-drive.db');
export const PORT_FILE = path.join(DATA_DIR, 'server.port');
export const OFFLINE_DIR = path.join(STORAGE_DIR, 'offline');

/** gopeed 可执行文件路径：env GOPEED_PATH（Tauri 注入）> 资源目录/工作目录 > 项目 bin */
export function resolveGopeedPath() {
  if (process.env.GOPEED_PATH && fs.existsSync(process.env.GOPEED_PATH)) {
    return process.env.GOPEED_PATH;
  }
  const candidates = [
    path.join(process.cwd(), 'gopeed.exe'), // sidecar 资源目录
    path.join(process.cwd(), 'bin', 'gopeed', 'gopeed-web.exe'), // 项目根开发
    path.join(path.dirname(process.execPath), 'gopeed.exe'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return candidates[0];
}

export function ensureDirs() {
  for (const d of [DATA_DIR, DB_DIR, STORAGE_DIR, GOPEED_DIR, OFFLINE_DIR]) {
    fs.mkdirSync(d, { recursive: true });
  }
}
