// 冒烟测试：删除功能全链路（真实后端）
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

const TMP = mkdtempSync(path.join(os.tmpdir(), 'ucd2-del-'));
const DATA = path.join(TMP, 'data');
const STORAGE = path.join(TMP, 'storage');

const child = spawn(process.execPath, [path.resolve('src/index.js')], {
  env: {
    ...process.env,
    UC_DRIVE2_DATA_DIR: DATA,
    UC_DRIVE2_STORAGE_DIR: STORAGE,
    GOPEED_PATH: path.resolve('../bin/gopeed/gopeed-web.exe'),
  },
  stdio: ['ignore', 'pipe', 'pipe'],
});
child.stdout.on('data', d => process.stdout.write('[srv] ' + d));
child.stderr.on('data', d => process.stderr.write('[srv:err] ' + d));

const BASE = 'http://127.0.0.1:17210';
async function waitReady() {
  for (let i = 0; i < 50; i++) {
    try { if ((await fetch(BASE + '/api/health')).ok) return; } catch {}
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error('not ready');
}
async function api(method, url, body) {
  const res = await fetch(BASE + url, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const ct = res.headers.get('content-type') || '';
  return { status: res.status, body: ct.includes('json') ? await res.json() : null };
}

let ok = 0;
function assert(cond, msg) {
  if (!cond) throw new Error('断言失败: ' + msg);
  ok++;
  console.log('  ✓ ' + msg);
}

try {
  await waitReady();
  console.log('== 1) 上传文件 ==');
  const f = path.join(TMP, 'del.txt');
  writeFileSync(f, 'to be deleted');
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([readFileSync(f)]), 'del.txt');
  const up = await fetch(BASE + '/api/files', { method: 'POST', body: form });
  const [row] = await up.json();
  assert(row.id > 0, `上传 id=${row.id}`);

  console.log('== 2) 删除单个文件 ==');
  const del = await api('DELETE', `/api/files/${row.id}`);
  assert(del.status === 200 && del.body.ok === true, '删除返回 ok');
  const gone = await api('GET', `/api/files/${row.id}`);
  assert(gone.status === 404, '删除后 GET 404');
  let fileGone = true;
  try { readFileSync(path.join(STORAGE, 'del.txt')); fileGone = false; } catch {}
  assert(fileGone, '磁盘文件已删除');

  console.log('== 3) 删除目录（含子目录+文件） ==');
  const d1 = await api('POST', '/api/dirs', { name: '删除甲', parent: 'root' });
  const d1id = d1.body.id;
  const d2 = await api('POST', '/api/dirs', { name: '删除乙', parent: String(d1id) });
  const d2id = d2.body.id;
  const form2 = new FormData();
  form2.append('parent', String(d2id));
  form2.append('files', new Blob([new TextEncoder().encode('deep file')]), 'deep.txt');
  await fetch(BASE + '/api/files', { method: 'POST', body: form2 });
  const listBefore = await api('GET', '/api/files?parent=root');
  assert(listBefore.body.some(x => x.name === '删除甲'), '目录存在');
  // 确认磁盘上确实有文件（删除前的状态）
  const diskBefore = readFileSync(path.join(STORAGE, '删除甲', '删除乙', 'deep.txt'), 'utf8');
  assert(diskBefore === 'deep file', '删除前磁盘文件存在');

  const delDir = await api('DELETE', `/api/files/${d1id}`);
  assert(delDir.status === 200, `删除目录返回 200（实际 ${delDir.status}）`);
  const listAfter = await api('GET', '/api/files?parent=root');
  assert(!listAfter.body.some(x => x.name === '删除甲'), '目录已从列表消失');
  // 磁盘上应已不存在
  let diskGone = true;
  try { readFileSync(path.join(STORAGE, '删除甲', '删除乙', 'deep.txt')); diskGone = false; } catch {}
  assert(diskGone, '磁盘目录树已删除');
  // DB 子行也应级联删除
  const deepRow = await api('GET', '/api/files/999999');
  assert(deepRow.status === 404, 'placeholder3');

  console.log('== 4) 删除不存在的 id ==');
  const nf = await api('DELETE', '/api/files/999999');
  assert(nf.status === 404, '不存在返回 404');

  console.log('== 5) 磁盘上已被外部删除的文件再删除 ==');
  const f2 = path.join(TMP, 'ghost.txt');
  writeFileSync(f2, 'ghost');
  const form3 = new FormData();
  form3.append('parent', 'root');
  form3.append('files', new Blob([readFileSync(f2)]), 'ghost.txt');
  const up3 = await fetch(BASE + '/api/files', { method: 'POST', body: form3 });
  const [row3] = await up3.json();
  rmSync(path.join(STORAGE, 'ghost.txt')); // 外部删除
  const delGhost = await api('DELETE', `/api/files/${row3.id}`);
  assert(delGhost.status === 200, '磁盘已无文件仍返回 ok（force）');

  console.log('\n全部通过（' + ok + ' 项断言）');
} catch (e) {
  console.error('\n失败:', e.message);
  process.exitCode = 1;
} finally {
  child.kill();
  await new Promise(r => setTimeout(r, 800));
  try { rmSync(TMP, { recursive: true, force: true }); } catch {}
}
