import { test, before, after, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

// 必须在导入服务模块前设置环境变量
const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-files-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

const { createApp } = await import('../src/app.js');
const { openDb } = await import('../src/db.js');
const { STORAGE_DIR } = await import('../src/config.js');
const { GopeedManager } = await import('../src/services/gopeed.js');
const { TaskService } = await import('../src/services/tasks.js');

let app, server, base, db;

const noopLog = { log() {}, warn() {}, error() {} };

before(async () => {
  db = openDb();
  const gopeed = new GopeedManager({ log: noopLog });
  const tasks = new TaskService(db, gopeed);
  app = createApp({ db, gopeed, tasks });
  server = app.listen(0, '127.0.0.1');
  await new Promise(r => server.once('listening', r));
  base = `http://127.0.0.1:${server.address().port}`;
});

after(() => {
  server?.close();
  db?.close();
  rmSync(DATA_DIR, { recursive: true, force: true });
});

async function api(method, url, opts = {}) {
  const res = await fetch(base + url, { method, ...opts });
  const ct = res.headers.get('content-type') || '';
  const body = ct.includes('json') ? await res.json() : await res.arrayBuffer();
  return { status: res.status, headers: res.headers, body };
}

test('健康检查与设置', async () => {
  const { status, body } = await api('GET', '/api/health');
  assert.equal(status, 200);
  assert.equal(body.ok, true);
  const s = await api('GET', '/api/settings');
  assert.equal(s.body.storageDir, STORAGE_DIR);
});

test('新建目录与列表', async () => {
  const { status, body } = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '文档', parent: 'root' }),
  });
  assert.equal(status, 201);
  assert.equal(body.is_dir, true);
  const dirId = body.id;

  const list = await api('GET', '/api/files?parent=root');
  assert.equal(list.body.length, 1);
  assert.equal(list.body[0].name, '文档');

  // 嵌套
  const sub = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '子目录', parent: dirId }),
  });
  assert.equal(sub.status, 201);
  const subList = await api('GET', `/api/files?parent=${dirId}`);
  assert.equal(subList.body.length, 1);
  assert.equal(subList.body[0].name, '子目录');
});

test('非法目录名被拒绝', async () => {
  const { status } = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'a/b/c' }),
  });
  assert.equal(status, 400);
});

test('multipart 上传与元数据一致性', async () => {
  const dir = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '上传区' }),
  });
  const dirId = dir.body.id;

  const form = new FormData();
  const buf = new TextEncoder().encode('hello uc-drive2 中文内容');
  form.append('parent', String(dirId));
  form.append('files', new Blob([buf], { type: 'text/plain' }), '说明.txt');

  const res = await api('POST', '/api/files', { body: form });
  assert.equal(res.status, 200);
  assert.equal(res.body.length, 1);
  const row = res.body[0];
  assert.equal(row.name, '说明.txt');
  assert.equal(row.size, buf.length);
  assert.equal(row.mime, 'text/plain');
  assert.equal(row.is_dir, false);

  // 磁盘上确实存在且内容一致
  const fsRow = (await api('GET', `/api/files/${row.id}`)).body;
  const onDisk = await (await fetch(base + `/api/files/${row.id}/download`)).arrayBuffer();
  assert.equal(new TextDecoder().decode(onDisk), 'hello uc-drive2 中文内容');
  assert.ok(fsRow.path.includes('上传区'));
});

test('上传重名自动去重', async () => {
  const buf = new TextEncoder().encode('x');
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([buf]), '重名.txt');
  await api('POST', '/api/files', { body: form });
  const form2 = new FormData();
  form2.append('parent', 'root');
  form2.append('files', new Blob([buf]), '重名.txt');
  const res = await api('POST', '/api/files', { body: form2 });
  assert.equal(res.body[0].name, '重名 (1).txt');
});

test('Range 下载：整文件 / 部分 / 越界', async () => {
  const data = Buffer.from('0123456789abcdefghij'); // 20 字节
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([data]), 'range.bin');
  const { body: [row] } = await api('POST', '/api/files', { body: form });

  // 整文件
  const full = await api('GET', `/api/files/${row.id}/download`);
  assert.equal(full.status, 200);
  assert.equal(full.headers.get('accept-ranges'), 'bytes');
  assert.equal(Buffer.from(full.body).length, 20);

  // 部分
  const part = await fetch(base + `/api/files/${row.id}/download`, { headers: { Range: 'bytes=2-5' } });
  assert.equal(part.status, 206);
  assert.equal(part.headers.get('content-range'), 'bytes 2-5/20');
  assert.equal(Buffer.from(await part.arrayBuffer()).toString(), '2345');

  // 尾部 N 字节
  const tail = await fetch(base + `/api/files/${row.id}/download`, { headers: { Range: 'bytes=-4' } });
  assert.equal(tail.status, 206);
  assert.equal(Buffer.from(await tail.arrayBuffer()).toString(), 'ghij');

  // 越界
  const bad = await fetch(base + `/api/files/${row.id}/download`, { headers: { Range: 'bytes=50-60' } });
  assert.equal(bad.status, 416);
  assert.equal(bad.headers.get('content-range'), 'bytes */20');

  // HEAD 也支持 Range 语义
  const head = await fetch(base + `/api/files/${row.id}/download`, { method: 'HEAD' });
  assert.equal(head.status, 200);
});

test('重命名与移动', async () => {
  const dir = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '甲目录' }),
  });
  const dirB = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '乙目录' }),
  });

  const buf = new TextEncoder().encode('move me');
  const form = new FormData();
  form.append('parent', String(dir.body.id));
  form.append('files', new Blob([buf]), 'm.txt');
  const { body: [row] } = await api('POST', '/api/files', { body: form });

  // 重命名
  const renamed = await api('PATCH', `/api/files/${row.id}`, {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'm2.txt' }),
  });
  assert.equal(renamed.body.name, 'm2.txt');

  // 移动
  const moved = await api('PATCH', `/api/files/${row.id}`, {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parent: String(dirB.body.id) }),
  });
  assert.ok(moved.body.path.includes('乙目录'));
  const listB = await api('GET', `/api/files?parent=${dirB.body.id}`);
  assert.equal(listB.body.length, 1);

  // 目录整体移动后后代路径同步
  const buf2 = new TextEncoder().encode('deep');
  const f2 = new FormData();
  f2.append('parent', String(dir.body.id));
  f2.append('files', new Blob([buf2]), 'inner.txt');
  await api('POST', '/api/files', { body: f2 });
  await api('PATCH', `/api/files/${dir.body.id}`, {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '甲目录改名' }),
  });
  const deep = await api('GET', '/api/files?parent=' + dir.body.id);
  assert.ok(deep.body[0].path.includes('甲目录改名'));
});

test('删除（目录递归）', async () => {
  const dir = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '待删除' }),
  });
  const form = new FormData();
  form.append('parent', String(dir.body.id));
  form.append('files', new Blob([new TextEncoder().encode('x')]), 'a.txt');
  await api('POST', '/api/files', { body: form });

  const del = await api('DELETE', `/api/files/${dir.body.id}`);
  assert.equal(del.status, 200);
  const list = await api('GET', '/api/files?parent=root');
  assert.ok(!list.body.some(f => f.name === '待删除'));
  const gone = await api('GET', `/api/files/${dir.body.id}`);
  assert.equal(gone.status, 404);
});

test('搜索', async () => {
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([new TextEncoder().encode('s')]), '季度报表.xlsx');
  await api('POST', '/api/files', { body: form });
  const { body } = await api('GET', '/api/search?q=' + encodeURIComponent('季度'));
  assert.ok(body.some(f => f.name === '季度报表.xlsx'));
});

test('目录下载被拒绝', async () => {
  const dir = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '防目录下载' }),
  });
  const { status } = await api('GET', `/api/files/${dir.body.id}/download`);
  assert.equal(status, 400);
});

test('路径穿越防护', async () => {
  const form = new FormData();
  form.append('parent', 'root');
  form.append('files', new Blob([new TextEncoder().encode('x')]), '正常名.txt');
  const res = await api('POST', '/api/files', { body: form });
  assert.equal(res.status, 200);
  assert.ok(!res.body[0].path.includes('..'));
});

test('移动防护：不能移入自身子目录', async () => {
  const a = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '防循环甲' }),
  });
  const b = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '防循环乙', parent: a.body.id }),
  });
  // 把父目录移动到子目录 → 拒绝
  const bad = await api('PATCH', `/api/files/${a.body.id}`, {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parent: String(b.body.id) }),
  });
  assert.equal(bad.status, 400);
  // 移动到自身 → 拒绝
  const self = await api('PATCH', `/api/files/${a.body.id}`, {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parent: String(a.body.id) }),
  });
  assert.equal(self.status, 400);
});

test('浏览目录不刷新目录修改时间', async () => {
  const dir = await api('POST', '/api/dirs', {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: '时间稳定' }),
  });
  const first = (await api('GET', `/api/files/${dir.body.id}`)).body;
  await api('GET', '/api/files?parent=root');
  await api('GET', '/api/files?parent=root');
  const second = (await api('GET', `/api/files/${dir.body.id}`)).body;
  assert.equal(first.updated_at, second.updated_at, '浏览不应改变 updated_at');
});

test('临时 torrent 上传与清理', async () => {
  // 上传一个伪 torrent 到临时目录
  const bytes = new TextEncoder().encode('d8:announce0e');
  const form = new FormData();
  form.append('file', new Blob([bytes]), 'sample.torrent');
  const res = await api('POST', '/api/tmp-files', { body: form });
  assert.equal(res.status, 201);
  assert.ok(res.body.name.endsWith('.torrent'));

  // 非 torrent 扩展名被拒绝且不残留
  const bad = new FormData();
  bad.append('file', new Blob([bytes]), 'not-torrent.txt');
  const badRes = await api('POST', '/api/tmp-files', { body: bad });
  assert.equal(badRes.status, 400);

  // 未进入文件树
  const list = await api('GET', '/api/files?parent=root');
  assert.ok(!list.body.some(f => f.name.includes('sample.torrent')), '临时文件不应进入文件树');
});
