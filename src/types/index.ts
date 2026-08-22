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
  dataDir: string;
  gopeedDir: string;
  gopeed: { running: boolean; port: number | null; base: string | null };
}

export interface TreeNode {
  id: number;
  name: string;
  path: string;
  children: TreeNode[];
}

export type ViewMode = 'grid' | 'list';
export type ThemeMode = 'light' | 'dark' | 'auto';
