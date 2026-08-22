import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';

// CORS 回归：WebView（origin=tauri.localhost）fetch 本机后端必须带上 CORS 头，
// 否则前端所有请求被浏览器拦截（Failed to fetch / 无限重试转圈）。
const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-cors-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

const { openDb } = await import('../src/db.js');
const { createApp } = await import('../src/app.js');

let db, server, port;
const gopeedStub = { ready: false, port: null, base: null };

before(async () => {
  db = openDb();
  const app = createApp({ db, gopeed: gopeedStub, tasks: {} });
  server = http.createServer(app);
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  port = server.address().port;
});

after(async () => {
  server?.close();
  db?.close();
  rmSync(DATA_DIR, { recursive: true, force: true });
});

function corsRequest(method, url, headers = {}) {
  return fetch(`http://127.0.0.1:${port}${url}`, { method, headers });
}

test('GET 响应带 Access-Control-Allow-Origin（跨域可读）', async () => {
  const res = await corsRequest('GET', '/api/health', { Origin: 'http://tauri.localhost' });
  assert.equal(res.status, 200);
  assert.equal(res.headers.get('access-control-allow-origin'), '*');
});

test('OPTIONS 预检返回 204 并声明允许的方法与请求头', async () => {
  const res = await corsRequest('OPTIONS', '/api/files', {
    Origin: 'http://tauri.localhost',
    'Access-Control-Request-Method': 'GET',
    'Access-Control-Request-Headers': 'content-type',
  });
  assert.equal(res.status, 204);
  assert.equal(res.headers.get('access-control-allow-origin'), '*');
  const methods = res.headers.get('access-control-allow-methods') || '';
  assert.ok(methods.includes('POST') && methods.includes('DELETE'));
  const headers = res.headers.get('access-control-allow-headers') || '';
  assert.ok(headers.toLowerCase().includes('range'), '应允许 Range 头（下载断点续传）');
});

test('业务接口同样带 CORS 头', async () => {
  const res = await corsRequest('GET', '/api/settings', { Origin: 'https://tauri.localhost' });
  assert.equal(res.status, 200);
  assert.equal(res.headers.get('access-control-allow-origin'), '*');
  assert.equal(res.headers.get('access-control-expose-headers'), 'Content-Range,Content-Length,Accept-Ranges');
});
