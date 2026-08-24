import { invoke } from '@tauri-apps/api/core';
import type { DownloadConfig, FileNode, SettingsInfo, TaskItem, TaskSource, TreeNode, UcFile, UcParseResult, UcSession } from '@/types';

/**
 * 后端 API 客户端。
 * base URL 来源优先级：Tauri invoke(get_server_port) → window.__UCDRIVE2_BASE__（开发注入） → 默认 17210。
 */

declare global {
  interface Window {
    __UCDRIVE2_BASE__?: string;
  }
}

const DEFAULT_PORT = 17210;
const MAX_PORT_TRIES = 20;

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

async function reachable(base: string, timeout = 600): Promise<boolean> {
  try {
    const res = await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(timeout) });
    return res.ok;
  } catch {
    return false;
  }
}

function isNetworkError(e: unknown): boolean {
  // fetch 连接拒绝/网络失败抛 TypeError；AbortSignal.timeout 超时抛 AbortError
  return e instanceof TypeError || (e instanceof DOMException && e.name === 'AbortError');
}

async function detectBase(): Promise<string> {
  if (window.__UCDRIVE2_BASE__) return window.__UCDRIVE2_BASE__;
  // Tauri 环境：Rust 侧已轮询等待 port 文件，拿到后仍需确认 HTTP 真的就绪
  try {
    const port = await invoke<number>('get_server_port');
    if (port > 0) {
      const base = `http://127.0.0.1:${port}`;
      if (await reachable(base, 1000)) return base;
    }
  } catch { /* 非 Tauri 环境 */ }
  // 轮询探测：并行扫 17210..17229，直到后端就绪（最长 30s），避免启动竞态永久失败
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const found = await Promise.all(
      Array.from({ length: MAX_PORT_TRIES }, (_, i) =>
        reachable(`http://127.0.0.1:${DEFAULT_PORT + i}`)),
    );
    const idx = found.findIndex(Boolean);
    if (idx >= 0) return `http://127.0.0.1:${DEFAULT_PORT + idx}`;
    await sleep(500);
  }
  return `http://127.0.0.1:${DEFAULT_PORT}`;
}

let basePromise: Promise<string> | null = null;
export function getBase(): Promise<string> {
  basePromise ||= detectBase();
  return basePromise;
}

export function resetBase(): void {
  basePromise = null;
}

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  let lastErr: unknown;
  // 网络层错误（后端尚未就绪/重启/连接被拒）自动重试并重新探测 base；业务错误直接抛出
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const base = await getBase();
      const res = await fetch(base + path, {
        method,
        headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
        body: body !== undefined ? JSON.stringify(body) : undefined,
      });
      const data = await res.json().catch(() => null);
      if (!res.ok) {
        const msg = data?.error || `请求失败 (${res.status})`;
        const err = new Error(msg) as Error & { kind?: string };
        err.kind = data?.kind;
        throw err;
      }
      return data as T;
    } catch (e) {
      if (!isNetworkError(e)) throw e;
      lastErr = e;
      resetBase();
      await sleep(800 * (attempt + 1));
    }
  }
  throw lastErr;
}

export const api = {
  // 文件
  listFiles: (parent: number | null) =>
    req<FileNode[]>('GET', '/api/files' + (parent == null ? '?parent=root' : `?parent=${parent}`)),
  getFile: (id: number) => req<FileNode>('GET', `/api/files/${id}`),
  mkdir: (name: string, parent: number | null) =>
    req<FileNode>('POST', '/api/dirs', { name, parent: parent ?? 'root' }),
  rename: (id: number, patch: { name?: string; parent?: number | null }) =>
    req<FileNode>('PATCH', `/api/files/${id}`, {
      ...(patch.name !== undefined ? { name: patch.name } : {}),
      ...(patch.parent !== undefined ? { parent: patch.parent ?? 'root' } : {}),
    }),
  remove: (id: number) => req<{ ok: boolean }>('DELETE', `/api/files/${id}`),
  upload: async (parent: number | null, files: File[]): Promise<FileNode[]> => {
    const base = await getBase();
    const form = new FormData();
    form.append('parent', parent == null ? 'root' : String(parent));
    for (const f of files) form.append('files', f, f.name);
    const res = await fetch(base + '/api/files', { method: 'POST', body: form });
    const data = await res.json();
    if (!res.ok) throw new Error(data?.error || '上传失败');
    return data;
  },
  /** 临时上传（torrent 用，不入文件树，任务创建后后端清理） */
  uploadTmp: async (file: File): Promise<{ name: string }> => {
    const base = await getBase();
    const form = new FormData();
    form.append('file', file, file.name);
    const res = await fetch(base + '/api/tmp-files', { method: 'POST', body: form });
    const data = await res.json();
    if (!res.ok) throw new Error(data?.error || '临时上传失败');
    return data;
  },
  downloadUrl: async (id: number, preview = false): Promise<string> => {
    const base = await getBase();
    return `${base}/api/files/${id}/download${preview ? '?preview=1' : ''}`;
  },
  search: (q: string) => req<FileNode[]>('GET', `/api/search?q=${encodeURIComponent(q)}`),
  tree: () => req<TreeNode[]>('GET', '/api/tree'),

  // 任务
  createTask: (payload: { source: TaskSource; url?: string; torrentId?: number; torrentName?: string; connections?: number }) =>
    req<TaskItem>('POST', '/api/tasks', payload),
  listTasks: () => req<TaskItem[]>('GET', '/api/tasks'),
  pauseTask: (id: number) => req<TaskItem>('POST', `/api/tasks/${id}/pause`),
  resumeTask: (id: number) => req<TaskItem>('POST', `/api/tasks/${id}/resume`),
  deleteTask: (id: number, force = false) => req<{ ok: boolean }>('POST', `/api/tasks/${id}/delete`, { force }),

  // 下载参数
  downloadConfig: () => req<DownloadConfig>('GET', '/api/tasks/config'),
  saveDownloadConfig: (patch: Partial<DownloadConfig>) =>
    req<DownloadConfig>('PUT', '/api/tasks/config', patch),

  // UC 网盘解析
  ucParse: (shareLink: string, cookie?: string) =>
    req<UcParseResult>('POST', '/api/uc/parse', { shareLink, cookie }),
  ucListFolder: (shareId: string, pdirFid: string | null, session: UcSession) =>
    req<{ files: UcFile[] }>('POST', '/api/uc/list-folder', { shareId, pdirFid, session }),
  ucDownload: (payload: {
    shareId: string; stoken: string; fid: string; shareFidToken: string;
    filename: string; size: number; ctoken: string; cookies: string; shareLink: string;
    connections?: number;
  }) => req<TaskItem>('POST', '/api/uc/download', payload),

  // UC Cookie
  cookieStatus: () => req<{ hasCookie: boolean }>('GET', '/api/cookie'),
  saveCookie: (cookie: string) => req<{ ok: boolean }>('PUT', '/api/cookie', { cookie }),
  clearCookie: () => req<{ ok: boolean }>('DELETE', '/api/cookie'),

  // 历史
  history: () => req<TaskItem[]>('GET', '/api/history'),
  clearHistory: () => req<{ deleted: number }>('DELETE', '/api/history'),

  // 系统
  health: () => req<{ ok: boolean; gopeed: boolean; version: string }>('GET', '/api/health'),
  settings: () => req<SettingsInfo>('GET', '/api/settings'),
};

export function formatSize(bytes: number): string {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  return `${(bytes / 1024 ** i).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  return bytesPerSec > 0 ? `${formatSize(bytesPerSec)}/s` : '';
}
