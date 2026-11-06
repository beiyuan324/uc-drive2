import { spawn, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

// 任一服务退出或收到终止信号时，关闭另一服务及其子进程（包括 Gopeed）。
export function runDev(commands) {
  const children = commands.map(([command, args]) =>
    spawn(command, args, { stdio: 'inherit', windowsHide: true, detached: process.platform !== 'win32' }));
  return new Promise((resolve) => {
    let stopping = false;
    const interrupt = () => finish(130);
    const terminate = () => finish(143);
    async function finish(code) {
      if (stopping) return;
      stopping = true;
      process.off('SIGINT', interrupt);
      process.off('SIGTERM', terminate);
      const running = children.filter((child) => child.pid && child.exitCode === null && child.signalCode === null);
      const closed = running.map((child) => new Promise((done) => child.once('close', done)));
      for (const child of running) {
        if (process.platform === 'win32') {
          const result = spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true });
          if (result.error) { console.error(result.error); code = 1; }
        } else {
          process.kill(-child.pid, 'SIGTERM');
        }
      }
      await Promise.all(closed);
      resolve(code);
    }
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', terminate);
    for (const child of children) {
      child.once('error', (error) => { console.error(error); finish(1); });
      child.once('exit', (code) => finish(code ?? 1));
    }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await runDev([
    ['cargo', ['run', '--manifest-path', 'src-tauri/Cargo.toml', '--example', 'folder-drive-server']],
    [process.execPath, ['node_modules/vite/bin/vite.js']],
  ]);
}
