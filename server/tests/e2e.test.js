import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, existsSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

// 真实 gopeed 端到端：URL 任务 → 完成 → 登记进文件树
// 显式指向项目根 bin/gopeed/gopeed-web.exe（cwd 在 server/ 下时找不到）
const HERE = path.dirname(fileURLToPath(import.meta.url));
const GOPEED_CANDIDATE = path.join(HERE, '..', '..', 'bin', 'gopeed', 'gopeed-web.exe');
if (existsSync(GOPEED_CANDIDATE)) {
  process.env.GOPEED_PATH = GOPEED_CANDIDATE;
}

const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-e2e-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

const { GopeedManager } = await import('../src/services/gopeed.js');
const { TaskService } = await import('../src/services/tasks.js');
const { openDb } = await import('../src/db.js');
const { STORAGE_DIR, resolveGopeedPath } = await import('../src/config.js');

const GOPEED_EXE = resolveGopeedPath();
const HAVE_GOPEED = existsSync(GOPEED_EXE);

let db, manager, tasks;
let fileServer, fileServerPort;
const FILE_NAME = 'e2e-data.bin';
const FILE_SIZE = 256 * 1024;
let fileBuffer;

before(async () => {
  // 生成测试文件并起本地 HTTP 服务器
  fileBuffer = crypto.randomBytes(FILE_SIZE);
  fileServer = http.createServer((req, res) => {
    if (req.url === `/${FILE_NAME}`) {
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': FILE_SIZE });
      res.end(fileBuffer);
    } else {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise(r => fileServer.listen(0, '127.0.0.1', r));
  fileServerPort = fileServer.address().port;

  db = openDb();
  manager = new GopeedManager({ exePath: GOPEED_EXE, log: { log() {}, warn: console.warn, error: console.error } });
  tasks = new TaskService(db, manager);
  await manager.start();
});

after(async () => {
  await manager.stop().catch(() => {});
  fileServer?.close();
  db?.close();
  rmSync(DATA_DIR, { recursive: true, force: true });
});

test('真实 gopeed：URL 任务完成并登记进树', { skip: !HAVE_GOPEED && '未找到 gopeed-web.exe' }, async () => {
  const url = `http://127.0.0.1:${fileServerPort}/${FILE_NAME}`;
  const row = await tasks.create({ source: 'url', url });
  assert.ok(row.id > 0);
  assert.ok(row.gopeed_id, '应获得 gopeed 任务 id');

  // 轮询直到完成
  let final;
  const deadline = Date.now() + 60000;
  while (Date.now() < deadline) {
    final = tasks.get(row.id);
    if (final.status === 'done' || final.status === 'error') break;
    // 手动触发同步（生产环境由 gopeed 轮询事件驱动）
    try {
      const gts = await manager.listTasks();
      tasks._syncFromGopeed(gts);
    } catch { /* 忽略瞬时错误 */ }
    await new Promise(r => setTimeout(r, 800));
  }
  assert.equal(final.status, 'done', `任务应完成，实际: ${final.status} ${final.error}`);
  assert.equal(final.progress, 100);

  // 文件树根下应有 e2e-data.bin
  const rootRows = db.prepare('SELECT name, path FROM files WHERE parent_id IS NULL').all();
  const registered = rootRows.find(r => r.name === FILE_NAME);
  assert.ok(registered, '单文件任务应登记到根下');
  const diskPath = registered.path.replace(/\//g, path.sep);
  assert.ok(existsSync(diskPath), '磁盘上应存在已登记文件');
  const onDisk = readFileSync(diskPath);
  assert.equal(onDisk.length, FILE_SIZE);
  assert.deepEqual(onDisk, fileBuffer, '下载内容应与源一致');
});

test('真实 gopeed：任务列表与删除', { skip: !HAVE_GOPEED && '未找到 gopeed-web.exe' }, async () => {
  const url = `http://127.0.0.1:${fileServerPort}/${FILE_NAME}`;
  const row = await tasks.create({ source: 'url', url });
  await tasks.remove(row.id, true);
  assert.equal(tasks.get(row.id), undefined);
});
