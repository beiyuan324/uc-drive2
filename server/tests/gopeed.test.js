import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync, existsSync, mkdirSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';

const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-gopeed-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

const { GopeedManager } = await import('../src/services/gopeed.js');
const { TaskService } = await import('../src/services/tasks.js');
const { openDb } = await import('../src/db.js');
const { STORAGE_DIR } = await import('../src/config.js');

const noopLog = { log() {}, warn() {}, error() {} };

/** 伪造子进程对象，可触发 exit */
function fakeProc() {
  const listeners = {};
  const proc = {
    killed: false,
    stdout: { on() {} },
    stderr: { on() {} },
    on(ev, fn) { (listeners[ev] ||= []).push(fn); return proc; },
    once(ev, fn) { (listeners[ev] ||= []).push(fn); return proc; },
    emit(ev, ...a) { (listeners[ev] || []).forEach(fn => fn(...a)); },
    kill() { proc.killed = true; setTimeout(() => proc.emit('exit', 0), 5); },
  };
  return proc;
}

/** 模拟 gopeed REST 服务 */
function startMockGopeed(port, token, store, { crashOn = null } = {}) {
  return http.createServer((req, res) => {
    const respond = data => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify({ code: 0, msg: '', data })); };
    const fail = (code, msg) => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify({ code, msg, data: null })); };
    if (!req.headers['x-api-token'] || req.headers['x-api-token'] !== token) return fail(1001, 'unauthorized');
    const u = new URL(req.url, 'http://x');
    if (req.method === 'GET' && u.pathname === '/api/v1/info') return respond({ version: 'mock', runtime: 'go', os: 'windows', arch: 'amd64' });
    if (req.method === 'POST' && u.pathname === '/api/v1/tasks') {
      let body = '';
      req.on('data', c => (body += c));
      req.on('end', () => {
        if (crashOn === 'create') process.exit(1);
        const parsed = JSON.parse(body);
        const t = {
          id: `mock-${store.size + 1}`,
          name: parsed.req?.url?.split('/').pop() || 'mock',
          status: 'ready',
          size: 100,
          progress: { speed: 0, downloaded: 0 },
          meta: { req: parsed.req, res: { name: '', size: 100, files: [{ name: 'f.bin', path: '', size: 100 }] }, opts: parsed.opts },
        };
        store.set(t.id, t);
        respond(t.id);
      });
      return;
    }
    if (req.method === 'GET' && u.pathname === '/api/v1/tasks') return respond([...store.values()]);
    const m = u.pathname.match(/^\/api\/v1\/tasks\/([^/]+)\/(pause|continue)$/);
    if (req.method === 'PUT' && m) {
      const t = store.get(m[1]);
      if (!t) return fail(2001, 'task not found');
      t.status = m[2] === 'pause' ? 'pause' : 'running';
      return respond(null);
    }
    const dm = u.pathname.match(/^\/api\/v1\/tasks\/([^/]+)$/);
    if (req.method === 'DELETE' && dm) {
      store.delete(dm[1]);
      return respond(null);
    }
    fail(1000, 'not found');
  });
}

let db;
const store = new Map();
let manager;
let servers = [];

before(async () => {
  db = openDb();
  const spawned = [];
  manager = new GopeedManager({
    log: noopLog,
    spawnFn(args) {
      spawned.push(args);
      const port = Number(args[args.indexOf('-P') + 1]);
      const token = args[args.indexOf('-T') + 1];
      const srv = startMockGopeed(port, token, store);
      servers.push(srv);
      srv.listen(port, '127.0.0.1');
      const proc = fakeProc();
      proc.spawned = spawned.length;
      return proc;
    },
  });
  await manager.start();
});

after(async () => {
  await manager.stop().catch(() => {});
  for (const s of servers) s.close();
  db.close();
  rmSync(DATA_DIR, { recursive: true, force: true });
});

test('gopeed 管理器：启动健康检查', async () => {
  assert.equal(manager.ready, true);
  const info = await manager.info();
  assert.equal(info.version, 'mock');
  assert.ok(manager.port > 0);
  assert.ok(manager.token.length >= 32);
});

test('gopeed 管理器：建任务 / 列表 / 暂停 / 继续 / 删除', async () => {
  const id = await manager.createTask({ url: 'http://example.com/a.zip' }, { path: 'C:/tmp' });
  assert.equal(id, 'mock-1');
  assert.equal(store.size, 1);

  const tasks = await manager.listTasks();
  assert.equal(tasks.length, 1);
  assert.equal(tasks[0].name, 'a.zip');

  await manager.pause(id);
  assert.equal(store.get(id).status, 'pause');
  await manager.resume(id);
  assert.equal(store.get(id).status, 'running');

  await manager.remove(id, true);
  assert.equal(store.size, 0);
});

test('gopeed 管理器：状态映射', () => {
  assert.equal(manager.mapStatus('ready'), 'running');
  assert.equal(manager.mapStatus('running'), 'running');
  assert.equal(manager.mapStatus('pause'), 'paused');
  assert.equal(manager.mapStatus('wait'), 'queued');
  assert.equal(manager.mapStatus('error'), 'error');
  assert.equal(manager.mapStatus('done'), 'done');
});

test('gopeed 管理器：异常退出自动重启', async () => {
  // 让当前 mock 进程退出 → 管理器应自动拉起第二个实例
  const oldPort = manager.port;
  manager.proc.emit('exit', 1);
  // 等待重启 + 健康
  const deadline = Date.now() + 8000;
  while (Date.now() < deadline) {
    if (manager.ready && manager.port !== oldPort) break;
    await new Promise(r => setTimeout(r, 200));
  }
  assert.equal(manager.ready, true, '重启后应就绪');
  assert.notEqual(manager.port, oldPort, '重启后端口应更换');
});

test('任务服务：创建 URL 任务并登记 gopeed_id', async () => {
  const tasks = new TaskService(db, manager);
  const row = await tasks.create({ source: 'url', url: 'http://example.com/data.bin' });
  assert.ok(row.id > 0);
  assert.equal(row.status, 'queued');
  assert.ok(row.gopeed_id.startsWith('mock-'), `gopeed_id=${row.gopeed_id}`);
  assert.ok(row.target_dir.includes('offline'));
});

test('任务服务：完成后登记进文件树（单文件直挂根）', async () => {
  const tasks = new TaskService(db, manager);
  const row = await tasks.create({ source: 'url', url: 'http://example.com/hello.txt' });
  // 模拟下载完成：往 target_dir 写文件，再推送 done 事件
  const target = row.target_dir.replace(/\//g, path.sep);
  writeFileSync(path.join(target, 'hello.txt'), 'downloaded content');
  const gid = row.gopeed_id;
  const gTask = {
    id: gid, name: 'hello.txt', status: 'done', size: 18,
    progress: { speed: 0, downloaded: 18 },
  };
  manager._emit({ type: 'tasks', tasks: [gTask] });
  await new Promise(r => setTimeout(r, 50));

  const updated = tasks.get(row.id);
  assert.equal(updated.status, 'done');
  assert.equal(updated.progress, 100);
  // 文件树根下应出现 hello.txt（单文件直挂根）
  const rootChildren = db.prepare('SELECT name FROM files WHERE parent_id IS NULL').all();
  assert.ok(rootChildren.some(r => r.name === 'hello.txt'), '文件应登记到根下');
  // 磁盘上存在于存储根
  const diskPath = path.join(STORAGE_DIR, 'hello.txt');
  assert.ok(existsSync(diskPath));
});

test('任务服务：暂停 / 继续 / 删除', async () => {
  const tasks = new TaskService(db, manager);
  const row = await tasks.create({ source: 'url', url: 'http://example.com/x.bin' });
  const paused = await tasks.pause(row.id);
  assert.equal(paused.status, 'paused');
  const resumed = await tasks.resume(row.id);
  assert.equal(resumed.status, 'running');
  await tasks.remove(row.id, false);
  assert.equal(tasks.get(row.id), undefined);
});

test('任务服务：非法链接被拒绝', async () => {
  const tasks = new TaskService(db, manager);
  await assert.rejects(() => tasks.create({ source: 'url', url: 'not-a-url' }), /链接不合法/);
});

test('任务服务：torrent 来源需真实 torrent 文件', async () => {
  const tasks = new TaskService(db, manager);
  await assert.rejects(() => tasks.create({ source: 'torrent', torrentId: 999 }), /无效的 torrent 文件/);
});

test('任务服务：临时 torrent 创建后自动清理', async () => {
  const { TMP_TORRENT_DIR } = await import('../src/services/tasks.js');
  const tasks = new TaskService(db, manager);
  // 手动放置一个临时 torrent
  mkdirSync(TMP_TORRENT_DIR, { recursive: true });
  const tmpFile = path.join(TMP_TORRENT_DIR, 'tmp-test-1.torrent');
  writeFileSync(tmpFile, 'd8:announce0e');
  const row = await tasks.create({ source: 'torrent', torrentName: 'tmp-test-1.torrent' });
  assert.ok(row.gopeed_id.startsWith('mock-'));
  assert.ok(!existsSync(tmpFile), '临时 torrent 应被清理');
  // 无效文件名被拒绝
  await assert.rejects(() => tasks.create({ source: 'torrent', torrentName: '不存在.torrent' }), /无效的 torrent 文件/);
});
