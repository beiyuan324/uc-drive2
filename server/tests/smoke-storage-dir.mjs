// 冒烟测试：真实启动后端，验证「切换网盘存储目录」API 全链路（含重启持久化）。
// 用法：node tests/smoke-storage-dir.mjs
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

const TMP = mkdtempSync(path.join(os.tmpdir(), 'ucd2-smoke-'));
const DATA = path.join(TMP, 'data');
const STORAGE = path.join(TMP, 'storage');
const NEW = path.join(TMP, 'newstore');

function startServer(env) {
  const child = spawn(process.execPath, [path.resolve('src/index.js')], {
    // GOPEED_PATH 指向真实 gopeed-web.exe（项目 bin），让引擎随服务一起拉起
    env: {
      ...process.env,
      UC_DRIVE2_DATA_DIR: DATA,
      GOPEED_PATH: path.resolve('../bin/gopeed/gopeed-web.exe'),
      ...env,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', d => process.stdout.write('[srv] ' + d));
  child.stderr.on('data', d => process.stderr.write('[srv:err] ' + d));
  return child;
}

async function waitReady(base = 'http://127.0.0.1:17210') {
  for (let i = 0; i < 50; i++) {
    try {
      const r = await fetch(base + '/api/health');
      if (r.ok) return;
    } catch { /* not ready */ }
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error('后端 10s 内未就绪');
}

async function api(base, method, url, body) {
  const res = await fetch(base + url, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  return { status: res.status, body: await res.json() };
}

let ok = 0;
function assert(cond, msg) {
  if (!cond) throw new Error('断言失败: ' + msg);
  ok++;
  console.log('  ✓ ' + msg);
}

let server = null;
async function stopServer() {
  if (!server) return;
  const p = server;
  server = null;
  p.kill();
  await Promise.race([
    new Promise(r => p.once('exit', r)),
    new Promise(r => setTimeout(r, 3000)),
  ]);
}

function rmRetry(dir) {
  for (let i = 0; i < 5; i++) {
    try { rmSync(dir, { recursive: true, force: true }); return; } catch { /* 等待锁释放 */ }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 400);
  }
}

try {
  // 第一次启动：默认存储目录
  server = startServer({ UC_DRIVE2_STORAGE_DIR: STORAGE });
  await waitReady();
  console.log('== 1) 默认存储目录 ==');
  let s = await api('http://127.0.0.1:17210', 'GET', '/api/settings');
  assert(s.body.storageDir === STORAGE, `storageDir = ${s.body.storageDir}`);
  assert(s.body.defaultStorageDir === STORAGE, 'defaultStorageDir = 环境默认');

  // 上传一个文件
  const file = path.join(TMP, 'a.txt');
  writeFileSync(file, 'hello storage-move');
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([readFileSync(file)]), 'a.txt');
  const up = await fetch('http://127.0.0.1:17210/api/files', { method: 'POST', body: form });
  const rows = await up.json();
  assert(rows.length === 1, '上传成功');

  // 切换存储目录（迁移）
  console.log('== 2) 切换存储目录 + 迁移 ==');
  const sw = await api('http://127.0.0.1:17210', 'PUT', '/api/settings/storage-dir', { dir: NEW, moveFiles: true });
  assert(sw.status === 200 && sw.body.changed === true, 'changed=true');
  assert(sw.body.movedFiles === 1, `movedFiles=1（实际 ${sw.body.movedFiles}）`);
  assert(sw.body.storageDir === NEW, `storageDir=${sw.body.storageDir}`);

  // 文件可下载、磁盘已在新目录
  const dl = await fetch(`http://127.0.0.1:17210/api/files/${rows[0].id}/download`);
  const content = await dl.text();
  assert(content === 'hello storage-move', '下载内容一致');
  assert(readFileSync(path.join(NEW, 'a.txt'), 'utf8') === 'hello storage-move', '文件已存在于新目录');

  // 幂等
  const same = await api('http://127.0.0.1:17210', 'PUT', '/api/settings/storage-dir', { dir: NEW });
  assert(same.body.changed === false, '相同目录幂等 changed=false');

  // 重启：应恢复自定义目录
  await stopServer();
  await new Promise(r => setTimeout(r, 500));
  server = startServer({}); // 不传 STORAGE_DIR env，验证从 DB 恢复
  await waitReady();
  console.log('== 3) 重启后持久化 ==');
  s = await api('http://127.0.0.1:17210', 'GET', '/api/settings');
  assert(s.body.storageDir === NEW, `重启后 storageDir=${s.body.storageDir}（应为 ${NEW}）`);
  const dl2 = await fetch(`http://127.0.0.1:17210/api/files/${rows[0].id}/download`);
  assert(await dl2.text() === 'hello storage-move', '重启后文件仍可下载');

  // 恢复默认
  console.log('== 4) 恢复默认目录 ==');
  const defDir = s.body.defaultStorageDir; // 重启后进程的默认（DATA_DIR/storage）
  const back = await api('http://127.0.0.1:17210', 'PUT', '/api/settings/storage-dir', { dir: '', moveFiles: true });
  assert(back.body.changed === true && back.body.storageDir === defDir, `恢复默认 storageDir=${back.body.storageDir}（应为 ${defDir}）`);
  const dl3 = await fetch(`http://127.0.0.1:17210/api/files/${rows[0].id}/download`);
  assert(await dl3.text() === 'hello storage-move', '恢复默认后文件仍可下载');

  await stopServer();
  await new Promise(r => setTimeout(r, 500));
  console.log(`\n全部通过（${ok} 项断言）`);
} finally {
  await stopServer();
  rmRetry(TMP);
}
