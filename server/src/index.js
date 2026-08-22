import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { BASE_PORT, PORT_FILE, DATA_DIR, ensureDirs, STORAGE_DIR } from './config.js';
import { openDb } from './db.js';
import { createApp } from './app.js';
import { GopeedManager } from './services/gopeed.js';
import { TaskService } from './services/tasks.js';

const log = {
  log: (...a) => console.log(...a),
  warn: (...a) => console.warn(...a),
  error: (...a) => console.error(...a),
};

/**
 * 从 startPort 起探测可用端口（EADDRINUSE 自动 +1，最多 maxTries 次）。
 * 注意：app.listen 的 EADDRINUSE 是异步 error 事件，无法 try/catch，
 * 因此必须先监听 net 探测端口，再真正 listen。
 */
function findFreePort(startPort, maxTries = 20) {
  return new Promise((resolve, reject) => {
    const tryListen = (port) => {
      const srv = net.createServer();
      srv.once('error', (err) => {
        if (err.code === 'EADDRINUSE' && port < startPort + maxTries - 1) {
          tryListen(port + 1);
        } else {
          reject(err);
        }
      });
      srv.listen(port, '127.0.0.1', () => {
        srv.close(() => resolve(port));
      });
    };
    tryListen(startPort);
  });
}

async function main() {
  ensureDirs();
  fs.mkdirSync(path.join(DATA_DIR, 'tmp'), { recursive: true });

  const db = openDb();
  log.log(`[uc-drive2] 数据目录: ${process.env.APPDATA || '~'}\\uc-drive2`);
  log.log(`[uc-drive2] 存储目录: ${STORAGE_DIR}`);

  const gopeed = new GopeedManager({ log });
  const tasks = new TaskService(db, gopeed);

  try {
    await gopeed.start();
    log.log(`[uc-drive2] gopeed 就绪: ${gopeed.base}`);
  } catch (err) {
    log.warn(`[uc-drive2] gopeed 启动失败（离线下载不可用）: ${err.message}`);
  }
  gopeed.startPolling(2000);

  const app = createApp({ db, gopeed, tasks });

  // 端口占用自动 +1（先用 net 探测，避免 listen 的异步 EADDRINUSE 崩溃）
  const port = await findFreePort(BASE_PORT);
  // 端口确定后立即写文件：Rust 侧 get_server_port 第一时间拿到端口，不等 HTTP 完全就绪
  fs.writeFileSync(PORT_FILE, String(port));
  const server = app.listen(port, '127.0.0.1');
  await new Promise((resolve, reject) => {
    server.once('listening', resolve);
    server.once('error', reject);
  });
  fs.writeFileSync(PORT_FILE, String(port));
  log.log(`[uc-drive2] 后端已监听 http://127.0.0.1:${port}（写入 ${PORT_FILE}）`);

  const shutdown = async (sig) => {
    log.log(`[uc-drive2] 收到 ${sig}，正在退出…`);
    server.close();
    await gopeed.stop().catch(() => {});
    process.exit(0);
  };
  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
}

main().catch(err => {
  console.error('[uc-drive2] 启动失败:', err);
  process.exit(1);
});
