# uc-drive2 交接文档（HANDOVER）

> 最后更新：2026-08-28（第 10 轮：全面性能优化）
> 面向后续会话/维护者的**内部项目记忆**（git 对外请读 README；本文件入口见 AGENT.md）。
> 各轮实现细节均已实装并验证，不再逐轮记述；只保留还会用到的常识、决策、坑与行动指南。

---

## 〇、当前状态（v1.1.0，2026-08-28）

- 功能全部实装：文件管理 / UC 解析下载（默认 300 连接）/ 离线任务（url/magnet/torrent）/ 历史记录 / 托盘 / 存储目录自定义 / 下载参数设置 / 主题三态
- 第 10 轮 = 全面性能优化（已全部落地并通过测试）：
  - 后端：`scanDir` 不再递归算目录大小；访问日志改缓冲批量落盘；回收站删除异步化；目录树单查询；`listDir` UPSERT 带 WHERE（无变化零写入）；`_syncFromGopeed` 语句只 prepare 一次；**gopeed 轮询空闲感知**（无 queued/running 任务时零请求）；新增 `GET /api/files/:id/ancestors`（面包屑一次请求）
  - 前端：tasks store 轮询幂等合并（无变化保留引用不重渲染）；搜索防抖 250ms
  - Rust：`get_server_port` 改 async（不再 `thread::sleep` 阻塞主线程）
- **测试基线**：后端 48 项（46 过 / 1 跳过 / 1 个 pre-existing 失败 = `uc-e2e` 网络项，UC Cookie 过期所致）；前端 27 项全过；vue-tsc 零错误；`cargo check` 通过

---

## 一、项目愿景（一句话）

单用户桌面网盘：本地文件管理 + 离线下载（HTTP/磁力/torrent）+ **UC 网盘分享链接高速下载**（多连接突破单连接限速），完全离线运行。

**技术栈**：Tauri v2 壳 · Vue3 + TS + Naive UI + Pinia · Node/Express 后端（esbuild 单文件）· SQLite（node:sqlite，零原生模块）· gopeed 下载引擎（sidecar）

---

## 二、架构 / 运行时

```
uc-drive2.exe (Tauri)
 └─ sidecar: node.exe → server-dist/server.js (Express, 127.0.0.1:17210, 占用自动+1)
     └─ gopeed-web.exe (headless: -A 127.0.0.1 -P <随机端口> -T <随机token> -d <数据目录>)
```

- 前端 base URL 解析链：`window.__UCDRIVE2_BASE__` → Tauri `invoke(get_server_port)`（Rust 异步轮询 `%APPDATA%/uc-drive2/server.port`）→ 并行探测 17210-17229 → fallback 17210；`req()` 网络错误自动重试 3 次并重置 base 缓存
- 数据目录 `%APPDATA%/uc-drive2/`：`storage/`（网盘根，**用户可自定义迁移**）、`gopeed/`、`data/uc-drive.db`、`server.port`、`.secret`（UC Cookie 密钥）、`access.log`（缓冲写）、`tmp/torrents/`、`backend-state.json`（{node,gopeed} pid，退出直杀用）
- 离线任务：`storage/offline/task-<id>/`，done 后登记进文件树（单文件直挂根去重 / 多文件按任务名建目录）
- 退出：托盘「退出」→ 读 `backend-state.json` 直接 `TerminateProcess(node/gopeed/自身)` 零等待（实测 4ms），taskkill /T 仅留不等待兜底
- 轮询：gopeed 每 2s 轮询；`TaskService.hasSyncWork()` 为空闲开关（队列里没有 queued/running 任务时跳过整轮 HTTP + DB 同步）

---

## 三、关键决策 / 坑（Critical Context，改代码前必读）

- **CORS 必须存在**：WebView origin=tauri.localhost → 127.0.0.1 属跨域，缺头则前端所有请求被拦（曾表现为 Failed to fetch / 无限转圈）。后端中间件统一放行（含 Range 头、EXPOSE Content-Range）
- **node:sqlite**：零原生模块，规避打包；`node:sqlite` 仍标 experimental（启动有 ExperimentalWarning，正常）
- **esbuild 单文件后端**：改 `server/src/**` 后必须重新打包，否则打包版不生效：`npm run build:server`（等价 `npx esbuild server/src/index.js --bundle --platform=node --format=cjs --outfile=server-dist/server.js`）
- **tauri 2.5+ resources 布局**：resources 进 exe 旁 `_up_/`（updater 布局还有 `_up_/<hash>/`）；用对象映射扁平化 + Rust `find()` 多布局探测 + `strip_verbatim()` 剥 `\\?\` 前缀；**图标坑**：tauri-build 不声明 rerun-if-changed，换 ico 后旧图标仍嵌在 exe → build.rs 显式声明 `cargo:rerun-if-changed=icons/*`
- **UC 进度 = `g.progress.downloaded`**（多连接分片下准确，实测与磁盘写入同步）；**不要用 fsutil queryValidData**（VDL=最高已写偏移+1，gopeed 先写尾部分片 → 瞬间虚高 98%+ 卡住）；**速度 = downloaded 增量估算**（`g.progress.speed` 不可靠，实测 50MB/s 报 56KB/s）；`progress.used` = 累计流量（含重试，可达下载量 180 倍），仅诊断用
- **UC 尾部慢 = 限速本质，无解**：OSS 均分带宽下尾部时长 ≈ 文件大小/总带宽（300/1000 连接实测一致）；resume 不重新调度分片、删除重建不续传（gopeed 静态分片）；已做前端「正在收尾 · 剩余约 X」
- **UC 直链**：每连接限速 ~100KB/s、多连接线性叠加；有效期 ~16h；直链带 OSS 回调（auth-cdn.uc.cn）校验 Cookie 登录态 → Cookie 过期 = 403 `require login`，解析分享页则不需要登录
- **下载参数存 settings 表**：`uc_connections`(默认 300) / `http_connections`(默认 0=gopeed 500) / `max_running`(默认 3)；maxRunning 经 `started` 事件应用到 gopeed
- **回收站删除**：PowerShell + VisualBasic FileIO 是唯一走回收站的方式，必须异步执行（同步会阻塞整个事件循环）
- **NSIS-only 打包**（用户要求）；**托盘退出必须 TerminateProcess**（taskkill 同步等待实测 5.4s）
- **UI 约定**：全中文、Naive UI + 自定义 CSS 变量（Emerald 强调色 / Zinc 基底）、动效克制适配 `prefers-reduced-motion`

---

## 四、gopeed 关键 API

- 创建：`POST /api/v1/tasks` `{ req: { url, extra: { header } }, opts: { name, path, extra: { connections } } }`
- 列表/详情：`GET /api/v1/tasks`、`GET /api/v1/tasks/<id>`；HEAD 头 `X-Api-Token`
- 状态：`ready/running/pause/done/error/wait`；`progress.downloaded` 准确、`progress.speed` 不可靠、`progress.used` 含重试
- 全局配置：`GET/PUT /api/v1/config`（`protocolConfig.http.connections` 默认 500、`maxRunning` 默认 3）
- gopeed 无 WebSocket → 后端 2s 轮询驱动（空闲自动跳过）

## 五、UC 接口（已验证真实可用）

- 解析：`POST https://pc-api.uc.cn/1/clouddrive/share/sharepage/v2/detail`（拿 stoken；分页 `_page/_size`）与 `share/sharepage/detail`（文件列表，`pdir_fid` 浏览子目录）
- 直链：`POST https://pc-api.uc.cn/1/clouddrive/file/download?entry=ft&fr=pc&pr=UCBrowser`（`fids + pwd_id + stoken + fids_token`）
- 错误码：31001 未登录、23018 超出大小限制、41006 分享不存在
- UA 必须 `uc-cloud-drive/2.5.20 Chrome/100... Electron/18.3.5.4`；Referer/Origin `https://drive.uc.cn/`
- Cookie 过期探测 `probeDownloadUrl()`：带 Cookie GET 4KB，403+`require login|auth expired`=cookie_expired、403+`SignatureDoesNotMatch`=url_invalid（直链签名失效，刷新即可）

## 六、TODO / 已知问题（仅未完成项）

- [ ] **直链刷新在「下载停滞但未报错」时的检测**：`_refreshUcUrl` 只在 gopeed 任务 error 时触发；OSS 断流不报错会卡住（当前未遇到，直链 16h 有效）
- [ ] `npm run dev` 纯浏览器模式最终人工确认（vite 代理 /api → 17210）
- [ ] 尾部下载慢优化思路：更大 connections（分片更小→尾部更快），需实测 800/1000 是否触发 OSS 防滥用
- 环境注意：已安装版在 `C:\Program Files\uc-drive2`；Rust 编译需 `export PATH="$HOME/.cargo/bin:$PATH"`（cargo 镜像已配 rsproxy `~/.cargo/config.toml`）；GitHub 加速 `https://githubdog.com/<完整github地址>`（大文件加 `-C -` 续传）

## 七、下次会话怎么做（行动指南）

1. **先跑测试确认基线**：`cd server && npm test`（48 项，46 过 / 1 跳过 / 1 个 pre-existing 失败 = uc-e2e 网络项，需有效 UC Cookie）+ `npm run test:unit`（27 项）+ `npx vue-tsc --noEmit`
2. **UC 功能验证**：根目录 `ucAuth.txt`（已 gitignore）含 `[url]`+`[cookie]` 两段；`cd server && node --test tests/uc-e2e.test.js` 验证真实链路（需网络 + 有效 Cookie）
3. **改后端后必须重打包**：`npm run build:server`（或 `npm run tauri:build` 一键整套）；Rust 侧改动 `cargo check` 通过后 → `npm run tauri:build` → NSIS 覆盖安装（`Start-Process ... -Verb RunAs -Wait`，先杀旧实例）
4. **验证安装版**：启动 → 读 `%APPDATA%/uc-drive2/server.port` → curl health → 看 access.log 确认前端请求到达（CORS 是否被拦）
5. **用户核心诉求**：任何改动后必须验证「UC 解析 → 下载 → 登记」全链路（uc-e2e 覆盖）

## 八、打包 / 环境 / 参考

- sidecar：`src-tauri/binaries/node-x86_64-pc-windows-msvc.exe`（Node 24，89MB，tauri-build 要求放在 src-tauri 下）
- gopeed：`bin/gopeed/gopeed-web.exe`（82MB，headless）：`-A 127.0.0.1 -P <port> -T <token> -d <dir>`
- 安装包：`src-tauri/target/release/bundle/nsis/uc-drive2_*_x64-setup.exe`（~42.5MB）
- 参考：原版 `D:\alone\uc-drive`（Electron + aria2/gopeed，uc-parser 思路同源）；X 网盘助手 `D:\Program Files\xzhushou`（已上线产品，UC 走 aria2 changeUri 换链接）