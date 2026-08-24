import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import net from 'node:net';
import fs from 'node:fs';
import path from 'node:path';
import { resolveGopeedPath, GOPEED_DIR, DATA_DIR } from '../config.js';

/** 记录 node/gopeed pid，供 Tauri 退出时直接 TerminateProcess（不依赖 taskkill，秒退） */
function writeBackendState(gopeedPid) {
  try {
    fs.writeFileSync(
      path.join(DATA_DIR, 'backend-state.json'),
      JSON.stringify({ node: process.pid, gopeed: gopeedPid }),
    );
  } catch {}
}

/**
 * gopeed 托管：headless 拉起 gopeed-web.exe，REST 客户端，健康轮询，
 * 异常退出自动重启。gopeed 无 WebSocket 推送，统一用 2s 轮询同步。
 */

const STATUS_MAP = {
  ready: 'running',
  running: 'running',
  pause: 'paused',
  wait: 'queued',
  error: 'error',
  done: 'done',
};

function freePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

export class GopeedManager {
  constructor({ exePath = resolveGopeedPath(), storageDir = GOPEED_DIR, log = console, spawnFn } = {}) {
    this.exePath = exePath;
    this.storageDir = storageDir;
    this.log = log;
    this.spawnFn = spawnFn || null;
    this.proc = null;
    this.host = '127.0.0.1';
    this.port = null;
    this.token = crypto.randomBytes(16).toString('hex');
    this.base = null;
    this.startedAt = 0;
    this.restarts = 0;
    this.maxRestarts = 5;
    this.stopping = false;
    this.listeners = new Set();
    this._pollTimer = null;
  }

  onEvent(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  _emit(ev) {
    for (const fn of this.listeners) fn(ev);
  }

  get running() {
    return !!this.proc && !this.proc.killed;
  }

  get ready() {
    return this.running && !!this.base;
  }

  /** 启动 gopeed 子进程并等待健康 */
  async start() {
    if (this.running) return this;
    this.port = await freePort();
    this.base = `http://${this.host}:${this.port}`;
    this.stopping = false;
    this._spawn();
    await this._waitReady(30);
    this._emit({ type: 'started', base: this.base });
    return this;
  }

  _spawn() {
    const args = ['-A', this.host, '-P', String(this.port), '-T', this.token, '-d', this.storageDir];
    this.log.log?.(`[gopeed] spawn ${this.exePath} ${args.join(' ')}`);
    this.startedAt = Date.now();
    let proc;
    if (this.spawnFn) {
      proc = this.spawnFn(args, this);
    } else {
      proc = spawn(this.exePath, args, {
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    }
    this.proc = proc;
    writeBackendState(proc.pid);
    proc.stdout?.on('data', d => this.log.log?.(`[gopeed] ${String(d).trim()}`));
    proc.stderr?.on('data', d => this.log.error?.(`[gopeed] ${String(d).trim()}`));
    proc.on('exit', (code, signal) => {
      this.proc = null;
      this.base = null;
      this._emit({ type: 'exit', code, signal });
      if (!this.stopping && this.restarts < this.maxRestarts) {
        this.restarts += 1;
        this.log.warn?.(`[gopeed] 异常退出(${code}), 1.5s 后重启 (${this.restarts}/${this.maxRestarts})`);
        setTimeout(() => this._restart(), 1500);
      } else if (!this.stopping) {
        this.log.error?.(`[gopeed] 重启次数超限，放弃`);
      }
    });
  }

  /** 重启：换新端口、重新拉起、等待健康 */
  async _restart() {
    this.port = await freePort();
    this.base = `http://${this.host}:${this.port}`;
    this._spawn();
    try {
      await this._waitReady(30);
      this._emit({ type: 'started', base: this.base });
    } catch (err) {
      this.log.error?.(`[gopeed] 重启后健康检查失败: ${err.message}`);
    }
  }

  async _waitReady(timeoutSec) {
    const deadline = Date.now() + timeoutSec * 1000;
    while (Date.now() < deadline) {
      if (!this.running) {
        await sleep(200);
        continue;
      }
      try {
        const info = await this.info();
        if (info) return info;
      } catch { /* 未就绪 */ }
      await sleep(400);
    }
    throw new Error('gopeed 启动超时');
  }

  async stop() {
    this.stopping = true;
    clearInterval(this._pollTimer);
    this._pollTimer = null;
    if (this.proc) {
      this.proc.kill();
      await new Promise(r => {
        const t = setTimeout(() => { this.proc?.kill('SIGKILL'); r(); }, 3000);
        this.proc.once('exit', () => { clearTimeout(t); r(); });
      });
    }
    this.base = null;
  }

  // ---------- REST 客户端 ----------

  async _req(method, path, body) {
    if (!this.base) throw Object.assign(new Error('gopeed 未就绪'), { code: 'GOPEED_DOWN' });
    const res = await fetch(this.base + path, {
      method,
      headers: {
        'X-Api-Token': this.token,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const json = await res.json().catch(() => ({ code: -1, msg: 'bad response' }));
    if (json.code !== 0) throw Object.assign(new Error(json.msg || 'gopeed 请求失败'), { code: json.code });
    return json.data;
  }

  info() {
    return this._req('GET', '/api/v1/info');
  }

  /** 全局配置（含 maxRunning / protocolConfig） */
  getConfig() {
    return this._req('GET', '/api/v1/config');
  }

  putConfig(cfg) {
    return this._req('PUT', '/api/v1/config', cfg);
  }

  createTask(req, opts = {}) {
    return this._req('POST', '/api/v1/tasks', { req, opts });
  }

  listTasks() {
    return this._req('GET', '/api/v1/tasks');
  }

  getTask(id) {
    return this._req('GET', `/api/v1/tasks/${encodeURIComponent(id)}`);
  }

  pause(id) {
    return this._req('PUT', `/api/v1/tasks/${encodeURIComponent(id)}/pause`);
  }

  resume(id) {
    return this._req('PUT', `/api/v1/tasks/${encodeURIComponent(id)}/continue`);
  }

  remove(id, force = false) {
    return this._req('DELETE', `/api/v1/tasks/${encodeURIComponent(id)}?force=${force}`);
  }

  // ---------- 轮询同步 ----------

  /** 启动任务轮询（每 2s 拉全量，回调每条任务） */
  startPolling(intervalMs = 2000) {
    if (this._pollTimer) return;
    this._pollTimer = setInterval(async () => {
      try {
        const tasks = await this.listTasks();
        this._emit({ type: 'tasks', tasks });
      } catch { /* gopeed 暂不可用，下轮再试 */ }
    }, intervalMs);
    this._pollTimer.unref?.();
  }

  mapStatus(s) {
    return STATUS_MAP[s] || 'queued';
  }
}

function sleep(ms) {
  return new Promise(r => setTimeout(r, ms));
}
