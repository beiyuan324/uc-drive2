/** 文件 / 目录元数据（后端 DTO） */
export interface FileNode {
  id: number;
  name: string;
  parent_id: number | null;
  is_dir: boolean;
  path: string;
  size: number;
  mime: string;
  created_at: string;
  updated_at: string;
}

export type TaskSource = 'url' | 'magnet' | 'torrent' | 'uc';
export type TaskStatus = 'queued' | 'running' | 'paused' | 'done' | 'error' | 'cookie_expired' | 'replaced';

export interface TaskItem {
  id: number;
  gopeed_id: string;
  source: TaskSource;
  source_url: string;
  status: TaskStatus;
  progress: number;
  speed: number;
  error: string;
  target_dir: string;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  finished_at: string | null;
}

/** UC 网盘文件条目 */
export interface UcFile {
  fid: string;
  name: string;
  size: number;
  file: boolean;
  format_type: string;
  share_fid_token: string;
}

/** UC 解析会话（前端内存持有，用于目录浏览与下载） */
export interface UcSession {
  stoken: string;
  ctoken: string;
  cookies: string;
}

export interface UcParseResult {
  platform: 'uc';
  shareId: string;
  pdirFid: string | null;
  files: UcFile[];
  session: UcSession;
  shareLink: string;
  cookieUsed: boolean;
}

export interface SettingsInfo {
  storageDir: string;
  /** 默认存储目录（%APPDATA%/uc-drive2/storage，可恢复） */
  defaultStorageDir?: string;
  dataDir: string;
  gopeedDir: string;
  gopeed: { running: boolean; port: number | null; base: string | null };
  download?: DownloadConfig;
  /** 切换存储目录的响应字段 */
  changed?: boolean;
  movedFiles?: number;
}

/** 下载参数（后端持久化，应用到 gopeed） */
export interface DownloadConfig {
  /** UC 直链并发连接数（每连接限速 ~100KB/s，多连接叠加） */
  ucConnections: number;
  /** 普通 HTTP 链接并发连接数（0 = 用 gopeed 全局默认） */
  httpConnections: number;
  /** 同时下载任务数 */
  maxRunning: number;
}

export interface TreeNode {
  id: number;
  name: string;
  path: string;
  children: TreeNode[];
}

export type ViewMode = 'grid' | 'list';
export type ThemeMode = 'light' | 'dark' | 'auto';
