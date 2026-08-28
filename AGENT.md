# AGENT.md — AI 助手 / 维护者行动指南

> 本项目面向 **AI 编码代理与后续维护者**。README.md 是 git 对外文档（不写内部记忆），**本项目全部项目记忆、决策与坑见本地 HANDOVER.md**（仅本地维护，已 gitignore，不上传公开仓库）。

## 一、这是什么

uc-drive2：单用户桌面网盘（Tauri v2 + Vue3/TS/Naive UI + Node/Express 后端 + SQLite + gopeed 下载引擎）。
本地文件管理 + 离线下载 + **UC 网盘分享链接解析高速下载**（默认 300 连接突破单连接限速）。完全离线运行。

## 二、动手前必读（按顺序）

1. **本地 HANDOVER.md**（项目根目录，git 忽略）— 压缩提炼后的项目记忆：架构、关键决策/坑、gopeed/UC 接口、TODO、行动指南。**改代码前必读「三、关键决策 / 坑」。**
2. `README.md` — git 对外文档（架构/功能/构建命令，面向读者）。
3. 涉及 Rust 侧时读 `src-tauri/src/lib.rs`（sidecar 拉起、端口轮询、托盘、退出直杀）。

## 三、常用命令

```bash
npm install && npm --prefix server install   # 首次安装依赖
npm run dev                                  # 后端(17210) + Vite(5173, 代理 /api)，浏览器可纯 Web 调试
npm run test                                 # 后端测试（cd server && npm test）
npm run test:unit                            # 前端 Vitest
npx vue-tsc --noEmit                         # 类型检查
npm run build                                # 前端产物 dist/ + 后端 server-dist/（build:app）
npm run tauri:build                          # build:app + NSIS 安装包
```

## 四、铁律（违反会白干或踩坑）

1. **改后端 `server/src/**` 后必须重新 esbuild**，否则打包版不生效：
   `npm run build:server`；整套构建用 `npm run tauri:build`（= 前端 + 后端 + NSIS）。
   （`server-dist/` 与 `dist/` 已 gitignore，是构建产物不是源码）
2. **后端测试基线**：48 项 = 46 过 + 1 跳过 + **1 个 pre-existing 失败**（`uc-e2e` 网络项，UC Cookie 过期所致，与代码无关）；前端 27 项全过。跑测试时别把那个网络失败当成回归。
3. **CORS 头绝不能删**（tauri.localhost → 127.0.0.1 跨域，缺头前端全挂）。
4. **UC 进度必须用 `g.progress.downloaded`**；**速度用 downloaded 增量**；fsutil VDL 与 `g.progress.speed` 都不可靠（详见本地 HANDOVER.md）。
5. **复用测试/真实链路**：UC 真实下载测试读根目录 `ucAuth.txt`（gitignore，用户测试数据，勿外泄、勿提交）。任何改动后验证「解析 → 下载 → 登记」链路（`cd server && node --test tests/uc-e2e.test.js`）。
6. **覆盖安装前先杀旧实例**；Rust 编译需 `export PATH="$HOME/.cargo/bin:$PATH"`。
7. UI 全中文；动效克制适配 `prefers-reduced-motion`；强调色 Emerald `#059669`。

## 五、本轮（第 10 轮）状态速查

已做：全面性能优化（scanDir 不递归算目录大小 / 访问日志缓冲 / 删除异步化 / 目录树单查询 / listDir UPSERT 带 WHERE / 轮询空闲跳过 / ancestors 接口 / tasks store 幂等合并 / 搜索防抖 / get_server_port 改 async）。
详情与验证数据见本地 HANDOVER.md 第〇节。