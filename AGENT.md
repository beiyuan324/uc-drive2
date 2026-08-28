# AGENT.md

> 面向 AI 编码代理与维护者的项目指南。对外说明见 `README.md`；长期实现记录见本地 `HANDOVER.md`（已 gitignore）。

## 项目概览

uc-drive2 是 Windows x64 单用户桌面网盘：Tauri v2 + Vue3/TypeScript/Naive UI + Rust/axum 后端 + SQLite + gopeed 下载引擎。

Rust HTTP 后端运行在 Tauri 主进程内，只监听 `127.0.0.1`。`gopeed-web.exe` 仍作为下载引擎子进程保留；应用不再依赖或分发 Node 后端运行时。前端 API 契约和 `src/api/index.ts` 的端口发现逻辑保持不变。

## 修改前阅读

1. `HANDOVER.md`：架构、兼容性约束、UC/gopeed 接口和已知坑。
2. `README.md`：构建、运行和测试入口。
3. 涉及 Tauri/Rust 生命周期时阅读 `src-tauri/src/lib.rs`、`src-tauri/src/backend/mod.rs`。
4. 涉及前端请求时阅读 `src/api/index.ts` 和 `src/types/index.ts`。

## 常用命令

```bash
npm install                                  # 前端依赖
npm run dev                                  # Rust 后端 + Vite
npm test                                     # 前端 Vitest
npx vue-tsc --noEmit                         # Vue/TypeScript 类型检查
npm run build                                # Vite 生产构建
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
npm run tauri:build                          # Release + NSIS 安装包
```

Rust 编译前确保 `cargo` 在 PATH 中；当前 Windows 环境通常需要：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## 维护约束

1. 后端 HTTP API、字段名、状态码和错误文本需兼容现有前端。修改路由时同步更新 `src-tauri/tests/backend_api.rs` 或 `tasks_api.rs`。
2. CORS 中间件不能删除：允许 `GET,POST,PUT,PATCH,DELETE,OPTIONS`、`Content-Type,Range`，并暴露 `Content-Range,Content-Length,Accept-Ranges`。
3. 服务默认从 `17210` 开始，端口冲突最多尝试 20 个端口；绑定后写入 `server.port`。不要破坏前端 Tauri `get_server_port` 和浏览器端口扫描。
4. SQLite 使用 `rusqlite` bundled；保持既有表结构、WAL、外键、`metadata` 迁移和旧数据路径。
5. UC Cookie 继续使用数据目录 `.secret` 中的 32 字节密钥和 `v1:{iv}:{tag}:{data}` AES-256-GCM 格式，不能记录或提交明文 Cookie。
6. UC 任务进度使用 gopeed `progress.downloaded`；速度使用下载字节增量，不使用 `progress.speed`。
7. gopeed 是唯一下载引擎：启动参数、随机端口/token、REST 管理、暂停/继续、完成登记和异常重启需保持可用。测试通过 `GopeedSpawner` 注入 mock。
8. 文件删除优先进入 Windows 回收站；失败时永久删除，并保留文件占用提示。路径必须经过存储根越界校验。
9. Windows 构建和安装验证要确认资源中保留 `gopeed-web.exe`，不包含 Node sidecar 或旧后端产物。
10. `ucAuth.txt` 是本地用户测试数据，已 gitignore；真实 UC 验证用 Rust 的 `uc_e2e` ignored test，不要把 Cookie 写进源码、日志或提交。

## 验证基线

Rust 默认测试覆盖 Rust 单元、文件 HTTP API 和 gopeed mock 任务 API；真实 UC 测试默认 ignored。前端测试使用 Vitest。完成后至少运行 Rust `--all-targets`、`npm test`、`vue-tsc`、Vite 构建和 Tauri Release/NSIS 构建，并检查安装包文件列表。

## UI 约定

界面保持全中文、Naive UI + Phosphor Icons、Emerald 强调色与 Zinc 基底；动效需兼容 `prefers-reduced-motion`。保持已有功能和交互，不在后端迁移中顺手改动视觉层。
