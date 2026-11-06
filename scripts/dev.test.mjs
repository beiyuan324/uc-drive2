import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { setTimeout } from 'node:timers/promises';
import { runDev } from './dev.mjs';

const node = (source) => [process.execPath, ['--input-type=module', '-e', source]];
const alive = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === 'ESRCH') return false; throw error; }
};

for (const [name, signal, expected] of [
  ['服务正常退出', null, 0],
  ['服务异常退出', null, 7],
  ['Ctrl+C', 'SIGINT', 130],
  ['终止信号', 'SIGTERM', 143],
]) {
  test(name + '会结束另一服务及孙进程', { timeout: 15000 }, async () => {
    const dir = await mkdtemp(join(tmpdir(), 'folder-drive-dev-'));
    const marker = join(dir, 'pids.json'), stop = join(dir, 'stop');
    let pids = [];
    const service = node(`
      import { spawn } from 'node:child_process';
      import { writeFileSync, renameSync } from 'node:fs';
      const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)']);
      writeFileSync(${JSON.stringify(marker + '.tmp')}, JSON.stringify([process.pid, child.pid]));
      renameSync(${JSON.stringify(marker + '.tmp')}, ${JSON.stringify(marker)});
      setInterval(() => {}, 1000);
    `);
    const quitter = node(`
      import { existsSync } from 'node:fs';
      setInterval(() => { if (existsSync(${JSON.stringify(stop)})) process.exit(${expected}); }, 20);
    `);
    const driver = spawn(process.execPath, ['--input-type=module', '-e', `
      import { writeFileSync } from 'node:fs';
      import { runDev } from ${JSON.stringify(new URL('./dev.mjs', import.meta.url).href)};
      const result = runDev(${JSON.stringify([service, quitter])});
      process.once('message', () => {
        ${signal ? `process.emit(${JSON.stringify(signal)});` : `writeFileSync(${JSON.stringify(stop)}, '');`}
        process.disconnect();
      });
      process.exitCode = await result;
    `], { stdio: ['ignore', 'pipe', 'pipe', 'ipc'], windowsHide: true });
    const exit = once(driver, 'exit');
    try {
      for (let i = 0; i < 200; i++) {
        try { pids = JSON.parse(await readFile(marker, 'utf8')); break; }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
        await setTimeout(25);
      }
      assert.equal(pids.length, 2, '服务和孙进程应已启动');
      assert(pids.every(alive));
      driver.send('stop');
      assert.equal((await exit)[0], expected);
      assert(pids.every((pid) => !alive(pid)), '退出后不能遗留服务或孙进程');
    } finally {
      if (driver.exitCode === null) driver.kill();
      for (const pid of pids) if (alive(pid)) spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { windowsHide: true });
      await rm(dir, { recursive: true, force: true });
    }
  });
}

test('命令不存在时返回失败并关闭其他服务', { timeout: 15000 }, async () => {
  assert.equal(await runDev([node('setInterval(() => {}, 1000)'), ['folder-drive-command-does-not-exist', []]]), 1);
});
