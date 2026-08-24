import { test, before } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const DATA_DIR = mkdtempSync(path.join(os.tmpdir(), 'ucd2-cookie-'));
process.env.UC_DRIVE2_DATA_DIR = DATA_DIR;
process.env.UC_DRIVE2_STORAGE_DIR = path.join(DATA_DIR, 'storage');

let db, cookie, uc;
let ucSvc;

test('cookie 加解密与读写', async () => {
  const { openDb } = await import('../src/db.js');
  const ck = await import('../src/services/cookie.js');
  db = openDb();
  cookie = ck;

  const secret = 'SUBID=xxx; SNUID=yyy; cookie2=zzz';
  ck.setUcCookie(db, secret);
  assert.ok(ck.hasUcCookie(db), '应已保存 cookie');

  const saved = ck.getUcCookie(db);
  assert.equal(saved, secret, '解密应还原原文');

  // 存储值必须是密文（不泄露明文）
  const raw = db.prepare("SELECT value FROM settings WHERE key = 'uc_cookie'").get().value;
  assert.ok(!raw.includes('SUBID'), '数据库不应存明文');
  assert.ok(raw.startsWith('v1:'), '应带版本前缀');

  // 篡改后解密失败返回空串
  db.prepare("UPDATE settings SET value = 'v1:aaaa' WHERE key = 'uc_cookie'").run();
  assert.equal(ck.getUcCookie(db), '', '损坏密文应返回空');
});

test('uc.extractIds 解析分享链接', async () => {
  const u = await import('../src/services/uc.js');
  const { shareId, pdirFid } = u.extractIds('https://drive.uc.cn/s/abc123xyz?public=1');
  assert.equal(shareId, 'abc123xyz');
  assert.equal(pdirFid, null);

  const withDir = u.extractIds('https://drive.uc.cn/s/abc123xyz?public=1#/list/share/abc123xyz/9f86d081-...');
  // hash 目录格式：/share/<id>/<pdir>-...（取第一段 32 位 hex）
  const deep = u.extractIds('https://drive.uc.cn/s/abc123xyz#/list/share/abc123xyz/9f86d081884c7d659a2feaa0c55ad015-8e0/-1');
  assert.equal(deep.pdirFid, '9f86d081884c7d659a2feaa0c55ad015');
  assert.equal(withDir.shareId, 'abc123xyz');
});

test('uc 解析器对无效链接报错', async () => {
  const u = await import('../src/services/uc.js');
  await assert.rejects(() => u.parse('https://example.com/not-uc', ''), /share_id/);
});

test('uc 解析调用真实 UC 接口（网络可用时）', { skip: true, timeout: 30000 }, async () => {
  // 需要真实分享链接 + cookie，手动运行验证用
});

test('uc.probeDownloadUrl 直链预检分类（本地 mock OSS）', async () => {
  const http = await import('node:http');
  const { probeDownloadUrl } = await import('../src/services/uc.js');
  const cases = [
    // 正常可下载：206 分片响应
    { status: 206, body: 'partial-data', expect: 'ok' },
    // Cookie 过期：OSS 回调拒绝，require login [auth expired]
    { status: 403, body: '<?xml version="1.0"?><Error><Code>RequestDeniedByCallback</Code><Message>Callback deny this request reason: require login [auth expired]</Message></Error>', expect: 'cookie_expired' },
    // 签名不匹配：直链本身有问题，刷新即可
    { status: 403, body: '<?xml version="1.0"?><Error><Code>SignatureDoesNotMatch</Code><Message>The request signature we calculated does not match</Message></Error>', expect: 'url_invalid' },
  ];
  let idx = 0;
  const srv = http.createServer((_req, res) => {
    const c = cases[idx];
    idx += 1;
    res.writeHead(c.status, { 'Content-Type': 'text/xml' });
    res.end(c.body);
  });
  await new Promise((r) => srv.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${srv.address().port}/x`;
  try {
    for (const c of cases) {
      const r = await probeDownloadUrl(base, 'cookie=1');
      assert.equal(r.kind, c.expect, `case ${c.expect} 应分类正确`);
    }
    // 网络异常（服务已关闭）→ network
    await new Promise((r) => srv.close(r));
    const deadPort = base;
    const dead = await probeDownloadUrl(deadPort, 'cookie=1');
    assert.equal(dead.kind, 'network', '连接失败应分类为 network');
    return;
  } finally {
    srv.close();
  }
});
