import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs, { mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// UC 网盘真实链路 e2e：parse → listFolder → getDownloadUrl → gopeed 下载 → 完成登记
// 需要根目录 ucAuth.txt（[url] 分享链接 + [cookie] Cookie），不存在或网络不可用时自动 skip。
// 此测试验证用户核心诉求：用 gopeed 成功下载 UC 网盘文件。
const HERE = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.join(HERE, '..', '..');
const AUTH_FILE = path.join(PROJECT_ROOT, 'ucAuth.txt');

function readAuth() {
  try {
    const lines = fs.readFileSync(AUTH_FILE, 'utf-8').split(/\r?\n/).map(s => s.trim()).filter(Boolean);
    let shareLink = '', cookie = '';
    for (let i = 0; i < lines.length; i++) {
      if (lines[i] === '[url]') shareLink = lines[i + 1];
      if (lines[i] === '[cookie]') cookie = lines[i + 1];
    }
    if (shareLink && cookie) return { shareLink, cookie };
  } catch { /* 文件不存在 */ }
  return null;
}

const AUTH = readAuth();
const HAVE_GOPEED = fs.existsSync(path.join(PROJECT_ROOT, 'bin', 'gopeed', 'gopeed-web.exe'));
const skipReason = !AUTH ? '缺少 ucAuth.txt（[url]+[cookie]），跳过真实 UC 链路测试' : !HAVE_GOPEED ? '未找到 gopeed-web.exe' : false;

const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-uc-e2e-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

let db, manager, tasks;

before(async () => {
  if (skipReason) return;
  const { openDb } = await import('../src/db.js');
  const { GopeedManager } = await import('../src/services/gopeed.js');
  const { TaskService } = await import('../src/services/tasks.js');
  db = openDb();
  manager = new GopeedManager({ exePath: path.join(PROJECT_ROOT, 'bin', 'gopeed', 'gopeed-web.exe'), log: { log() {}, warn: console.warn, error: console.error } });
  tasks = new TaskService(db, manager);
  await manager.start();
});

after(async () => {
  if (manager) await manager.stop().catch(() => {});
  db?.close();
  rmSync(DATA_DIR, { recursive: true, force: true });
});

test('UC 真实链路：解析→直链→gopeed 下载→登记文件树', { skip: skipReason, timeout: 180000 }, async () => {
  const { parse, listFolder, getDownloadUrl, UA } = await import('../src/services/uc.js');

  // 1. 解析分享链接
  const r = await parse(AUTH.shareLink, AUTH.cookie);
  assert.ok(r.shareId, '应解析出 shareId');
  assert.ok(r.session?.stoken && r.session?.ctoken, '应建立会话');
  assert.ok(r.files.length > 0, '应有文件列表');

  // 2. 递归收集全部文件，选最小文件（快）
  const all = [];
  async function walk(pdirFid) {
    const list = await listFolder(r.shareId, r.session.stoken, pdirFid, r.session.ctoken, r.session.cookies);
    for (const f of list) {
      if (f.file) all.push(f);
      else await walk(f.fid);
    }
  }
  await walk(null);
  assert.ok(all.length > 0, '分享中应有可下载文件');
  const target = all.sort((a, b) => a.size - b.size)[0];

  // 3. 直链
  const url = await getDownloadUrl(r.shareId, r.session.stoken, target.fid, target.share_fid_token, r.session.ctoken, r.session.cookies);
  assert.ok(/^https:\/\//.test(url), '应拿到 https 直链');

  // 4. gopeed 下载（300 连接突破 UC 限速）
  manager.startPolling(1000);
  const created = await tasks.create({
    source: 'uc',
    url,
    filename: target.name,
    headers: { 'User-Agent': UA, 'Cookie': r.session.cookies, 'Referer': 'https://drive.uc.cn/', 'Origin': 'https://drive.uc.cn', 'x-csrf-token': r.session.ctoken },
    uc: { shareId: r.shareId, shareLink: AUTH.shareLink, fid: target.fid, filename: target.name, shareFidToken: target.share_fid_token, size: target.size },
    connections: 300,
  });

  // 5. 轮询直到 done / error（最长 150s）
  const deadline = Date.now() + 150_000;
  let final;
  while (Date.now() < deadline) {
    const row = await tasks.get(created.id);
    if (row && ['done', 'error', 'cookie_expired'].includes(row.status)) { final = row; break; }
    await new Promise(r => setTimeout(r, 2000));
  }
  manager.stopPolling?.();
  assert.ok(final, '任务应在 150s 内结束');
  assert.equal(final.status, 'done', `任务应完成，实际: ${final.status} ${final.error || ''}`);
  assert.equal(final.progress, 100, '完成时进度应为 100%');

  // 6. 文件登记进文件树
  const fileRow = db.prepare('SELECT name, size, path FROM files WHERE id = ?').get(final.id);
  assert.ok(fileRow, '文件应登记');
  assert.equal(fileRow.size, target.size, '登记大小应与分享一致');
  assert.ok(fs.existsSync(fileRow.path), '登记文件应真实存在于磁盘');
  assert.equal(fs.statSync(fileRow.path).size, target.size, '磁盘文件大小应完整');
});

test('UC 限速特征：多连接可叠加（参考 X 网盘助手 300 连接方案）', { skip: skipReason, timeout: 60000 }, async () => {
  // UC 直链每连接限速 ~100KB/s，但多连接可叠加（curl 64 连接实测 4.7MB/s）。
  // 用 8 个并发 1MB Range 验证叠加（单连接 10s，8 连接应总耗时接近而非 80s）。
  const { parse, listFolder, getDownloadUrl, UA } = await import('../src/services/uc.js');
  const r = await parse(AUTH.shareLink, AUTH.cookie);
  const all = [];
  async function walk(pdirFid) {
    const list = await listFolder(r.shareId, r.session.stoken, pdirFid, r.session.ctoken, r.session.cookies);
    for (const f of list) {
      if (f.file) all.push(f);
      else await walk(f.fid);
    }
  }
  await walk(null);
  const big = all.sort((a, b) => b.size - a.size)[0];
  const url = await getDownloadUrl(r.shareId, r.session.stoken, big.fid, big.share_fid_token, r.session.ctoken, r.session.cookies);
  const H = { 'User-Agent': UA, 'Cookie': r.session.cookies, 'Referer': 'https://drive.uc.cn/', 'Origin': 'https://drive.uc.cn', 'x-csrf-token': r.session.ctoken };

  const t0 = Date.now();
  const res = await Promise.all(Array.from({ length: 8 }, (_, i) => (async () => {
    const rr = await fetch(url, { headers: { ...H, Range: `bytes=${i * 1048576}-${i * 1048576 + 1048575}` }, signal: AbortSignal.timeout(60000) });
    return rr.status === 206;
  })()));
  const dur = (Date.now() - t0) / 1000;
  assert.ok(res.every(Boolean), '8 个并发 Range 都应 206');
  // 单连接下 8MB 需 ~80s（100KB/s 限速）；8 并发应在远小于该时间完成 ⇒ 叠加生效
  assert.ok(dur < 40, `多连接应叠加提速（8×1MB 实测 <40s，本次 ${dur.toFixed(1)}s）`);
});
