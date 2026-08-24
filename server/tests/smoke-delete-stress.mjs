// 压力测试：嵌套目录删除是否偶发失败（用户反馈删除有问题）
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

const TMP = mkdtempSync(path.join(os.tmpdir(), 'ucd2-delstress-'));
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
let srvOut = '';
child.stdout.on('data', d => { srvOut += d; });
child.stderr.on('data', d => { srvOut += d; });

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

let fail = 0;
try {
  await waitReady();
  console.log('重复 5 轮：建嵌套目录（2 层 + 文件）→ 删除根目录 → 验证');
  for (let round = 1; round <= 5; round++) {
    const d1 = await api('POST', '/api/dirs', { name: `甲${round}`, parent: 'root' });
    const d2 = await api('POST', '/api/dirs', { name: '乙', parent: String(d1.body.id) });
    const d3 = await api('POST', '/api/dirs', { name: '丙', parent: String(d2.body.id) });
    const form = new FormData();
    form.append('parent', String(d3.body.id));
    form.append('files', new Blob([new TextEncoder().encode(`round ${round}`)]), 'f.txt');
    const up = await fetch(BASE + '/api/files', { method: 'POST', body: form });
    if (up.status !== 200) { fail++; console.log(`  轮${round}: 上传失败 ${up.status}`); continue; }
    const del = await api('DELETE', `/api/files/${d1.body.id}`);
    if (del.status !== 200) {
      fail++;
      console.log(`  轮${round}: 删除失败 status=${del.status} body=${JSON.stringify(del.body)}`);
      continue;
    }
    // 验证磁盘已清理
    let gone = true;
    try { readFileSync(path.join(STORAGE, `甲${round}`, '乙', '丙', 'f.txt')); gone = false; } catch {}
    if (!gone) { fail++; console.log(`  轮${round}: 磁盘残留`); continue; }
    // 后端进程是否还活着
    const health = await api('GET', '/api/health');
    if (health.status !== 200) { fail++; console.log(`  轮${round}: 后端疑似崩溃`); continue; }
    console.log(`  轮${round}: ✓ 删除成功，后端存活`);
  }
  console.log(fail === 0 ? '\n全部通过，无偶发失败' : `\n共 ${fail} 轮失败`);
} catch (e) {
  console.error('\n异常:', e.message);
  console.error('服务端日志尾部:\n' + srvOut.slice(-1500));
  fail++;
} finally {
  child.kill();
  await new Promise(r => setTimeout(r, 800));
  try { rmSync(TMP, { recursive: true, force: true }); } catch {}
  process.exitCode = fail ? 1 : 0;
}
