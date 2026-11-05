// 本地官方接口夹具 + 真实 Gopeed 集成测试；不访问真实账号、不修改用户数据。
import assert from 'node:assert/strict';
import { createServer as httpsServer } from 'node:https';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const temp = await mkdtemp(join(tmpdir(), 'uc-drive2-cloud-'));
const hosts = ['drive-h.quark.cn', 'drive-pc.quark.cn', 'pan.baidu.com', 'api-pan.xunlei.com'];
const payload = Buffer.from(Array.from({ length: 524288 }, (_, i) => i % 251));
const failures = [];
const calls = new Map();
const restores = new Map();
let tls, files, child, exitCode = 1;
const count = (key) => calls.set(key, (calls.get(key) ?? 0) + 1);
function json(res, data, status = 200) {
  res.writeHead(status, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(data));
}
const listen = (server) => new Promise((done, reject) => {
  server.once('error', reject); server.listen(0, '127.0.0.1', () => done(server.address().port));
});
const close = (server) => new Promise((done) => {
  if (!server) return done(); server.closeAllConnections(); server.close(done);
});

try {
  const openssl = [process.env.OPENSSL_BIN, 'openssl',
    join(process.env.ProgramFiles ?? 'C:/Program Files', 'Git/usr/bin/openssl.exe')]
    .filter(Boolean).find((exe) => spawnSync(exe, ['version'], { windowsHide: true }).status === 0);
  assert(openssl, '需要 OpenSSL（Windows 可使用 Git 自带的 openssl.exe）');
  const key = join(temp, 'mock.key'), cert = join(temp, 'mock.pem');
  const generated = spawnSync(openssl, ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
    '-subj', '/CN=uc-drive2-cloud-tests', '-addext', 'subjectAltName=' + hosts.map((h) => 'DNS:' + h).join(','),
    '-keyout', key, '-out', cert], { windowsHide: true, encoding: 'utf8' });
  assert.equal(generated.status, 0, '生成临时测试证书失败：' + generated.stderr);

  files = createServer((req, res) => {
    count('blob');
    const platform = new URL(req.url, 'http://localhost').pathname.split('/')[2];
    try {
      assert.equal(req.headers.referer, { quark: 'https://pan.quark.cn/', baidu: 'https://pan.baidu.com/', xunlei: 'https://pan.xunlei.com/' }[platform]);
      assert(req.headers['user-agent']);
      if (platform !== 'xunlei') assert(req.headers.cookie?.includes('test-secret'), 'Gopeed 丢失 Cookie');
      else assert.equal(req.headers.authorization, undefined, 'OAuth 不可发送给下载 CDN');
    } catch (e) { failures.push(e.message); json(res, { error: e.message }, 400); return; }
    const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range ?? '');
    const start = range ? Number(range[1]) : 0;
    const end = range?.[2] ? Math.min(Number(range[2]), payload.length - 1) : payload.length - 1;
    if (start >= payload.length || end < start) {
      res.writeHead(416, { 'Content-Range': 'bytes */' + payload.length }); res.end(); return;
    }
    res.writeHead(range ? 206 : 200, { 'Content-Type': 'application/octet-stream', 'Accept-Ranges': 'bytes',
      'Content-Length': end - start + 1,
      ...(range ? { 'Content-Range': 'bytes ' + start + '-' + end + '/' + payload.length } : {}) });
    if (req.method === 'HEAD') { res.end(); return; }
    let offset = start;
    function send() {
      if (res.destroyed) return;
      const next = Math.min(offset + 8192, end + 1);
      res.write(payload.subarray(offset, next)); offset = next;
      if (offset > end) res.end(); else setTimeout(send, 8);
    }
    send();
  });
  const httpPort = await listen(files);
  const blob = (platform, fid) => 'http://127.0.0.1:' + httpPort + '/blob/' + platform + '/' + fid;
  const qFile = (fid) => ({ fid, file_name: fid + '.bin', file: true, size: payload.length, share_fid_token: 'qtok-' + fid });
  const bdFile = (fid) => ({ fs_id: fid, server_filename: 'bd-' + fid + '.bin', path: '/资料/' + fid + '.bin', isdir: 0, size: payload.length });
  const xlFile = (id) => ({ id, kind: 'drive#file', name: id + '.bin', size: String(payload.length) });

  tls = httpsServer({ key: await readFile(key), cert: await readFile(cert) }, async (req, res) => {
    const host = req.headers.host.split(':')[0], url = new URL(req.url, 'https://' + host);
    count(host + url.pathname);
    let raw = ''; for await (const chunk of req) raw += chunk;
    const body = raw.startsWith('{') ? JSON.parse(raw) : Object.fromEntries(new URLSearchParams(raw));
    const cookie = req.headers.cookie ?? '', scenario = /scenario=([^;]+)/.exec(cookie)?.[1] ?? '';
    const p = Object.fromEntries(url.searchParams);
    try {
      assert(hosts.includes(host), '仅能请求官方主机');
      if (scenario === 'http403') return json(res, {}, 403);
      if (scenario === 'expired') return json(res, host.includes('quark') ? { code: '31001', message: '登录失效' }
        : host.includes('baidu') ? { errno: '-6', errmsg: '登录失效' } : { error_code: '9', error_description: 'captcha token expired' });
      if (host.includes('quark')) {
        assert(cookie.includes('test-secret')); assert.equal(req.headers.origin, 'https://pan.quark.cn');
        if (url.pathname.endsWith('/token')) {
          assert.equal(req.method, 'POST'); assert.equal(body.pwd_id, 'qMock'); assert.equal(body.passcode, 'a1b2');
          return json(res, { code: 0, data: { stoken: 'q-stoken' } });
        }
        if (url.pathname.endsWith('/detail')) {
          assert.equal(p.stoken, 'q-stoken');
          if (p.pdir_fid === 'q-folder') return json(res, { code: 0, data: { list: [qFile('q-child')] } });
          if (scenario === 'quark-pages') return json(res, { code: 0, data: { list: p._page === '1'
            ? Array.from({ length: 50 }, (_, i) => qFile('q-' + i)) : [qFile('q-last')] } });
          return json(res, { code: 0, data: { list: [
            { fid: 'q-folder', file_name: '目录', file: false, dir: true, size: 0 }, qFile('q-main')] } });
        }
        if (url.pathname.endsWith('/download')) {
          assert.equal(body.pwd_id, 'qMock'); assert.equal(body.stoken, 'q-stoken');
          const fid = body.fids[0]; assert.deepEqual(body.fids_token, ['qtok-' + fid]);
          return json(res, { code: 0, data: [{ fid: scenario === 'wrongfid' ? 'unrelated' : fid,
            download_url: scenario === 'badurl' ? 'file:///C:/secret.txt' : blob('quark', fid) }] });
        }
      }
      if (host === 'pan.baidu.com') {
        assert(cookie.includes('test-secret'));
        if (url.pathname === '/api/gettemplatevariable') return json(res, { errno: 0, result: { bdstoken: 'bd-token' } });
        if (url.pathname === '/share/verify') {
          assert.equal(p.surl, 'bdMock');
          if (body.pwd !== 'a1b2') return json(res, { errno: -9, errmsg: '提取码错误' });
          return json(res, { errno: 0, randsk: 'decoded%2Fshare%3D%3D' });
        }
        if (url.pathname === '/share/wxlist') {
          assert.equal(body.shorturl, '1bdMock'); assert.equal(body.pwd, 'a1b2');
          assert(cookie.includes('BDCLND=decoded%2Fshare%3D%3D'), 'BDCLND 不能二次编码');
          const folder = { fs_id: '8000', server_filename: '资料', path: '/资料', isdir: 1, size: 0 };
          const list = body.dir === '/资料' ? [bdFile('9007199254740995')] :
            body.page === '1' ? [folder, bdFile('9007199254740993')] : [bdFile('9007199254740994')];
          return json(res, { errno: 0, data: { uk: '10086', shareid: '20002', seckey: 'decoded/share==',
            list, has_more: body.dir === '/' && body.page === '1' ? 1 : 0 } });
        }
        if (url.pathname === '/share/tplconfig') {
          assert.equal(p.surl, '1bdMock', '签名请求不能去掉 surl 前缀'); assert.equal(p.bdstoken, 'bd-token');
          return json(res, { errno: 0, data: { sign: 'bd-sign', timestamp: 1793880000 } });
        }
        if (url.pathname === '/api/sharedownload') {
          assert.equal(p.sign, 'bd-sign'); assert.equal(p.clienttype, '5'); assert.equal(body.timestamp, p.timestamp);
          assert.equal(body.uk, '10086'); assert.equal(body.primaryid, '20002');
          assert.deepEqual(JSON.parse(body.extra), { sekey: 'decoded/share==' });
          assert(/^\[900719925474099[345]\]$/.test(body.fid_list), '大整数 fs_id 不能丢失精度');
          const fid = body.fid_list.slice(1, -1);
          return json(res, { errno: 0, list: [{ fs_id: fid, dlink: blob('baidu', fid) }] });
        }
      }
      if (host === 'api-pan.xunlei.com') {
        assert.equal(req.headers.authorization, 'Bearer test-secret-xl');
        assert.equal(req.headers['x-captcha-token'], 'test-captcha'); assert.equal(req.headers['x-device-id'], 'test-device');
        if (url.pathname === '/drive/v1/share' || url.pathname === '/drive/v1/share/detail') {
          assert.equal(p.share_id, 'xlMock');
          const list = p.parent_id === 'xl-folder' ? [xlFile('xl-child')] : p.page_token ? [xlFile('xl-restore')]
            : [{ id: 'xl-folder', name: '目录', kind: 'drive#folder', size: '0' }, xlFile('xl-direct')];
          return json(res, { files: list, pass_code_token: 'xl-pass',
            next_page_token: scenario === 'repeat-token' || !p.parent_id && !p.page_token ? 'next' : '' });
        }
        if (url.pathname === '/drive/v1/share/file_info') {
          assert.equal(p.pass_code_token, 'xl-pass');
          return json(res, { file_info: { id: scenario === 'xl-wrong-file' ? 'unrelated' : p.file_id,
            web_content_link: p.file_id === 'xl-restore' ? '' : blob('xunlei', p.file_id) } });
        }
        if (url.pathname === '/drive/v1/share/restore') {
          assert.equal(body.parent_id, ''); assert.equal(body.specify_parent_id, true);
          assert.deepEqual(body.file_ids, ['xl-restore']); assert.equal(body.pass_code_token, 'xl-pass');
          restores.set(scenario, 0);
          return json(res, { restore_task_id: 'restore-task', restore_status: 'RESTORE_PENDING' });
        }
        if (url.pathname === '/drive/v1/tasks/restore-task') {
          restores.set(scenario, (restores.get(scenario) ?? 0) + 1);
          if (scenario === 'restore-error') return json(res, { phase: 'PHASE_TYPE_ERROR' });
          return json(res, { phase: 'PHASE_TYPE_COMPLETE', params: { trace_file_ids: JSON.stringify(
            scenario === 'wrong-map' ? { unrelated: ['saved-unrelated'] } : { 'xl-restore': ['saved-xl-restore'] }) } });
        }
        if (url.pathname.startsWith('/drive/v1/files/')) {
          assert.equal(p.usage, 'CONSUME'); assert.equal(url.pathname, '/drive/v1/files/saved-xl-restore');
          assert((restores.get(scenario) ?? 0) > 0, '转存完成前不能取链');
          return json(res, { id: 'saved-xl-restore', web_content_link: blob('xunlei', 'xl-restore') });
        }
      }
      throw new Error('未预期的官方接口：' + req.method + ' ' + host + url.pathname);
    } catch (e) { failures.push(e.message); json(res, { error: e.message }, 500); }
  });
  const tlsPort = await listen(tls);
  console.log('本地 HTTPS 官方接口夹具已就绪；运行 Rust 集成测试和真实 Gopeed 下载。');
  child = spawn('cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked',
    '--test', 'cloud_downloads', '--', '--test-threads=1', '--include-ignored'], {
    cwd: root, stdio: 'inherit', windowsHide: true, env: { ...process.env,
      UCDRIVE2_TEST_HTTPS_PORT: String(tlsPort), UCDRIVE2_TEST_CA: cert, UCDRIVE2_TEST_ROOT: temp },
  });
  const timer = setTimeout(() => child.kill(), 15 * 60_000);
  try { exitCode = await new Promise((done, reject) => {
    child.once('exit', (code) => done(code ?? 1)); child.once('error', reject);
  }); } finally { clearTimeout(timer); }
  assert.deepEqual(failures, [], '官方接口或 Gopeed 请求头存在契约错误');
  if (exitCode === 0) {
    for (const key of ['/share/tplconfig', '/api/sharedownload', '/drive/v1/share/restore', '/drive/v1/files/saved-xl-restore'])
      assert([...calls.keys()].some((k) => k.endsWith(key)), '未覆盖关键接口：' + key);
    assert((calls.get('blob') ?? 0) > 3, '未实际下载和校验文件');
    console.log('官方接口契约与真实 Gopeed 下载均通过；文件请求次数：' + calls.get('blob'));
  }
} catch (e) { console.error(e.stack ?? e); exitCode = 1; }
finally {
  await close(tls); await close(files);
  if (temp.startsWith(join(tmpdir(), 'uc-drive2-cloud-')))
    await rm(temp, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
}
process.exitCode = exitCode;
