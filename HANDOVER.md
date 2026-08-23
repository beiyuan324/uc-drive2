# uc-drive2 交接文档（HANDOVER）

> 最后更新：2026-08-23（第 2 轮交接 / 下载参数+速度+提醒）
> 目的：让任何新会话（或隔天继续的会话）无需重读代码即可接续工作。

---

## 一、项目要做什么（愿景）

**uc-drive2**：单用户桌面网盘 + UC 网盘下载工具（原版 `D:\alone\uc-drive` 是 Electron + aria2 的 UC 分享链接解析下载器；uc-drive2 是其重构版）。

- **本体**：本地文件管理（浏览/上传/下载/预览/搜索，`%APPDATA%/uc-drive2/storage/` 为网盘根，SQLite 存元数据）
- **核心差异化**：UC 网盘分享链接解析 → 直链 → **gopeed 引擎**（替代 aria2）快速下载 → 完成自动登记进文件树
- **技术栈**：Tauri v2 壳 + Vue3/TS/Naive UI/Pinia + Node/Express 后端 + `node:sqlite`（无原生模块）+ esbuild 单文件后端 + gopeed-web.exe sidecar

**设计体系**：Emerald `#059669` 单一强调色、Zinc 中性底、圆角 8/12px、Geist 自托管字体、Phosphor 图标、light/dark/auto 三态主题、全中文界面、动效克制适配 `prefers-reduced-motion`。

---

## 二、本次会话做了什么（2026-08-23）

### 1. 功能对齐原版（uc-drive）—— 补齐 UC 下载链路
- 后端 `server/src/services/uc.js`：UC 解析服务（`extractIds`/`getCtoken`/`getStoken`/`getFileList` 自动翻页/`listFolder`/`getDownloadUrl`/`parse`/`resolveDownload`），纯 Node fetch 实现
- 后端 `server/src/services/cookie.js`：UC Cookie AES-256-GCM 加密存储（密钥 `%APPDATA%/uc-drive2/.secret` 首次随机生成）
- 后端路由：`POST /api/uc/parse`、`POST /api/uc/list-folder`、`POST /api/uc/download`、`GET/PUT/DELETE /api/cookie`、`GET/DELETE /api/history`
- 任务服务：`source:'uc'` 任务（headers 传入 gopeed）、`_refreshUcUrl` 直链过期自动刷新（5 次/30s 冷却）、`_classifyUcError`（cookie 失效→`cookie_expired`）、`history()`/`clearHistory()`
- 前端：`ParseView.vue`（批量解析/文件夹浏览/单文件+全部下载）、`HistoryView.vue`（重新下载/复制链接/清空）、Settings 加 UC Cookie 配置、AppLayout 导航加「UC 解析」「历史记录」、Downloads 加下载完成系统通知（Web Notification）
- Tauri：系统托盘（`tray-icon` feature，关闭窗口最小化到托盘、菜单：显示窗口/下载管理/退出）

### 2. 修复「页面转圈 / Failed to fetch」（根因：CORS）
- **根因**：WebView 页面 origin 是 `tauri.localhost`，fetch `http://127.0.0.1:17210` 属跨域；后端**从未返回 CORS 头** → 浏览器拦截所有请求 → 旧版逻辑（探测一次失败永久缓存 base）报 Failed to fetch，新版逻辑（无限重试）转圈
- **修复**：
  - `server/src/app.js`：CORS 中间件（`Access-Control-Allow-Origin: *`、OPTIONS 204、Allow-Headers 含 Range）
  - `src/api/index.ts`：`detectBase` 改为**并行轮询直到后端就绪**（30s 上限）+ `req` 网络错误自动重试 3 次（重置 base 缓存）+ `isNetworkError` 区分网络/业务错误
  - `server/src/index.js`：端口确定后**立即写 port 文件**（不等 HTTP 就绪）
  - `FileManager.vue`：错误提示加「重试」按钮
- 验证：新装版 access.log 显示前端业务请求已到达后端

### 3. 修复「UC 下载 0 进度假象」（根因：gopeed progress 字段不可靠）
- **真相**：gopeed 下载一直正常且飞快（300 连接下 1.98GB 前 12 秒写入 98%）；`g.progress.downloaded` 字段不可靠（实际 2GB 时仍显示 0.2MB）
- **修复**：`server/src/services/tasks.js` 的 `_syncFromGopeed` 改用 `validDataBytes()`（fsutil `queryValidData` 统计 NTFS 有效数据长度）计算真实进度；done 后进度固定 100（文件已移出 target_dir）
- 新增 `server/tests/uc-e2e.test.js`：UC 真实链路 e2e（解析→直链→gopeed 下载→登记，读根目录 `ucAuth.txt`，缺文件自动 skip）+ 限速叠加验证

### 4. 修复「UC 大文件下载慢」（根因：UC 直链每连接限速 ~100KB/s）
- **UC 直链特征**：每连接稳定限速 ~100KB/s（TTFB 1.2s 正常、90s 不断流）；**多连接可线性叠加**（curl 64 连接实测 4.7MB/s）；无并发上限
- gopeed `connections` 参数有效（本地限速服务器实测 32 连接 → 2.2MB/s；http/https 均验证带 header）
- **修复**：`server/src/app.js` `/api/uc/download` 默认 `connections: 300`（原 4 连接 → 分片 495MB → 82 分钟/片是灾难）
- 实测：300 连接下 1.98GB APK 前 12s 到 99.2%，全程 ~5 分钟（尾部因剩余分片少受限于单连接 100KB/s）
- 参考 X 网盘助手（`D:\Program Files\xzhushou`，Electron+aria2+gopeed）：UC 用 aria2（`changeUri` 换链接）、百度盘用 gopeed `protocolConfig.http.connections=300`；与我们"多连接突破限速"思路一致

### 5. 其他
- pi 配置：`~/.pi/agent/settings.json` 加 `compaction: { reserveTokens: 722000 }` → **上下文超过 278k 自动压缩**（用户死命令）
- 后端访问日志中间件（`%APPDATA%/uc-drive2/access.log`，诊断用，保留）
- 测试：后端 34 项全过（新增 CORS 3 项 + UC e2e 2 项）、前端 22 项全过
- 重新打包 NSIS（02:11 版，42.5MB）并覆盖安装到 `C:\Program Files\uc-drive2`，安装后验证：health/CORS/前端请求到达/UC 链路全部正常

### 6. 下载参数设置 + 速度显示修复 + Cookie 失效提醒 + 排队提示（第 2 轮）

**下载参数设置（用户可调，持久化到 DB）**
- 后端 `TaskService.getConfig()/setConfig()`：读/写 `settings` 表 `uc_connections`（默认 300）/ `http_connections`（默认 0=gopeed 默认）/ `max_running`（默认 3），数值自动 clamp（1..1000 / 0..1000 / 1..10）
- `_applyConfigToGopeed()`：把 DB 里的 maxRunning 应用到 gopeed `PUT /api/v1/config`（读原配置→改 maxRunning→写回），gopeed 未就绪时下次 `started` 事件重试
- `gopeed.js` 新增 `getConfig()/putConfig()`；`app.js` 新增 `GET/PUT /api/tasks/config`，`/api/settings` 返回里并入 `download` 字段
- `tasks.create()` 连接数优先级：显式传入 > 设置默认（uc→`uc_connections`、url→`http_connections`）；magnet/torrent 不传（gopeed 按协议处理）；`POST /api/tasks` 透传 `connections`
- 前端：Settings 加「下载参数」卡片（3 个 n-input-number + 保存）；settings store 加 `downloadConfig`/`saveDownloadConfig`，AppLayout 挂载时 `settings.load()` 全局可用；ParseView/Downloads 建任务时带连接数

**下载速度显示修复（`g.progress.speed` 不可靠）**
- 根因：gopeed 的 `progress.speed` 字段与 `progress.downloaded` 同样不可靠（大文件 50MB/s 实速时仅报 56KB/s）
- 修复：`_syncFromGopeed` 改为磁盘真实写入增量估算——每任务维护 `_speedCache {bytes, at}`，速度 = (本次 validDataBytes − 上次) / 间隔，与进度同源；首轮无基线 speed=0，paused/done 清缓存

**UC Cookie 失效前台提醒 + 排队提示**
- Downloads：`cookie_expired` 任务触发 Web Notification + 8s message 引导去「设置」更新（`notifiedCookieIds` 去重）；状态标签改为「Cookie 失效」+ 任务内提示文字
- Downloads：`queued` 状态显示「排队中，同时最多 N 个任务」；URL 任务创建带 `httpConnections`

**顺带修复**
- `vue-tsc` 类型错误清零：theme.ts 删掉 naive-ui 不存在的 `common.borderRadiusLarge`（组件级圆角已单独设置）、Downloads/HistoryView 的 `metadata.uc` 类型安全取值、torrent 文件窄化
- api.spec.ts 修复 pre-existing unhandled rejection（先挂断言再推进计时器）

**测试**：后端 39 项（38 过 1 跳过）——新增 4 项：并发连接数默认/覆盖、速度增量计算、配置 API（默认值/持久化/maxRunning 应用到 mock gopeed/非法值 500）、POST /api/tasks 透传 connections；前端 24 项全过。冒烟验证：真实启动后端，PUT 配置→gopeed maxRunning=5 生效、非法值 500、CORS 头齐全。

### 7. 进度条虚高修复（fsutil VDL 跳变）+ 任务标题显示文件名（第 3 轮，用户反馈）

**用户反馈 1：进度条一下子到 90 多然后不动**
- **根因（本地实测定位）**：多连接分片下载时，gopeed 会先写文件尾部的分片，而 `fsutil queryValidData` 返回的是 NTFS VDL（最高已写偏移+1）而非已写字节总数 → 256MB 文件 0.5 秒 VDL 就报 98.5%，之后中间空洞慢慢补，进度条看起来卡住不动
- **实测数据**：本地限速 Range 服务器（每连接 100KB/s）+ 100 连接：`g.progress.downloaded` 稳步增长（0.5s=6.3MB → 21s=206MB，≈10MB/s 与理论吻合）且与磁盘真实写入同步；`progress.used` 是累计流量（含重试，虚高不可用）；`progress.speed` 仍不可靠
- **修复**：`_syncFromGopeed` 进度改用 `min(g.progress.downloaded, total)`，速度改用 downloaded 增量估算（不再用 VDL / g.progress.speed）
- **更正旧结论**：HANDOVER 第 1 轮记载「gopeed progress.downloaded 不可靠（2GB 时显示 0.2MB）」是当时的观察；当前 gopeed 版本实测 downloaded 在多连接分片下准确，fsutil VDL 才是虚高源
- 新增回归测试「进度用 gopeed downloaded，防止 fsutil VDL 跳变虚高」：预分配 1000B + 只写尾部 1B → 旧代码进度算 100%，新代码按 downloaded=100 算 10%

**用户反馈 2：离线下载页标题应显示文件名**
- Downloads 任务主标题改为文件名（UC→分享文件名 / URL→链接末段去 query / magnet→「磁力任务」/ torrent→「种子任务」）
- 直链改为次要辅助行（小字截断 + title 完整链接；UC 任务显示「UC 直链」）

**验证**：端到端（真实后端+gopeed+限速 Range 服务器 256MB/100 连接）——首采样 4.9%（修复前 98.5%）、进度平滑 4.9→12→…→99.3→done 100%、速度 9.3-11.9MB/s 准确、30s 完成。测试：后端 40 项（39 过 1 跳过）、前端 24 项、vue-tsc 零错误。

---

## 三、还有哪些没做（TODO / 已知问题）

### 功能缺口（相对原版 uc-drive）
- [x] **下载参数设置**（第 2 轮已做：UC/普通链接并发连接数 + maxRunning，设置页可调，持久化到 DB 并应用到 gopeed）
- [x] **UC Cookie 过期提醒到前台**（第 2 轮已做：Web Notification + message 引导去设置）
- [x] **下载速度显示优化**（第 2 轮已做：磁盘真实写入增量计算，与进度同源）
- [x] **多任务并发**（第 2 轮已做：maxRunning 可调 + 排队提示「同时最多 N 个任务」）
- [ ] **直链刷新在「下载停滞但未报错」时的检测**：现 `_refreshUcUrl` 只在 gopeed 任务 error 时触发；若 OSS 断流不报错会卡住（当前未遇到，因为直链 16h 有效）

### 工程改进
- [ ] `npm run dev` 纯浏览器模式的最终人工确认（vite 代理 /api → 17210）
- [x] git 初始化提交（已完成 3 次提交；`*.exe` 已忽略 89MB node.exe + 82MB gopeed.exe；`ucAuth.txt` 已忽略）
- [ ] `%APPDATA%/uc-drive2/` 里测试残留（"测试目录"、offline/task-* 等）可清理，但用户数据目录不要乱动
- [ ] 尾部下载慢优化思路：更大 connections（分片更小 → 尾部更快），需实测 800/1000 连接是否触发 OSS 防滥用
- [x] 第 2 轮改动已重新打包 NSIS（15:13 版，42.5MB）并覆盖安装到 `C:\Program Files\uc-drive2`，安装后验证：health/CORS/前端请求到达/`/api/tasks/config` 全部正常

### 已知环境注意
- 已安装版在 `C:\Program Files\uc-drive2`（最新 02:11 包）；旧的 D:\Program Files\uc-drive2 已不存在
- Rust 编译需 `export PATH="$HOME/.cargo/bin:$PATH"`；cargo 镜像已配 rsproxy（`~/.cargo/config.toml`）
- GitHub 加速代理：`https://githubdog.com/<完整github地址>`（大文件可能截断需 `-C -` 续传）

---

## 四、下次会话怎么做（行动指南）

1. **先跑测试确认基线**：`cd server && npm test`（40 项，39 过 1 跳过网络项）+ `npm run test:unit`（24 项）
2. **UC 功能验证**：确保根目录 `ucAuth.txt` 存在（`[url]` + `[cookie]` 两段），跑 `cd server && node --test tests/uc-e2e.test.js` 验证真实链路（需网络）
3. **修改后重新打包**：`npx esbuild server/src/index.js --bundle --platform=node --format=cjs --outfile=server-dist/server.js` → `npm run tauri build` → NSIS 覆盖安装（`Start-Process ... -Verb RunAs -Wait`，注意先杀旧实例）
4. **验证安装版**：启动 → 读 `%APPDATA%/uc-drive2/server.port` → curl health → 看 `%APPDATA%/uc-drive2/access.log` 确认前端请求到达（CORS 是否被拦）
5. **用户核心诉求**：任何改动后必须验证「UC 解析 → 下载 → 登记」全链路（uc-e2e 测试覆盖）

---

## 五、关键架构 / 决策 / 坑（Critical Context）

### 架构
```
uc-drive2.exe (Tauri)
 └─ sidecar: node.exe → server-dist/server.js (Express, 127.0.0.1:17210, 占用自动+1)
     └─ gopeed-web.exe (headless: -A 127.0.0.1 -P <随机端口> -T <随机token> -d <数据目录>)
```
- 前端 base URL：`window.__UCDRIVE2_BASE__` → Tauri invoke `get_server_port`（Rust 轮询 port 文件 10s）→ 并行探测 17210-17229 → fallback 17210
- 数据目录 `%APPDATA%/uc-drive2/`：`storage/`（网盘根）、`gopeed/`（引擎）、`data/uc-drive.db`、`server.port`、`.secret`（Cookie 密钥）、`access.log`（访问日志）、`tmp/torrents/`（临时 torrent）
- 离线任务：`storage/offline/task-<id>/`，done 后登记进文件树（单文件直挂根去重 / 多文件按任务名建目录）
- 退出：托盘「退出」→ `app.exit(0)` → `RunEvent::Exit` → `taskkill /PID <node> /T /F` 级联杀 gopeed

### 关键决策
- **node:sqlite 替代 better-sqlite3**（零原生模块，规避打包）
- **esbuild 单文件后端**（server-dist/server.js 1.6MB，零 node_modules 分发）
- **gopeed 2s 轮询**（无 WebSocket）
- **tauri 2.5+ resources 布局**：resources 进 exe 旁 `_up_/`；用对象映射扁平化（`{"../server-dist/server.js":"server.js",...}`）+ Rust `find()` 多布局探测 + `strip_verbatim()` 剥 `\\?\` 前缀
- **CORS 必须存在**（tauri.localhost → 127.0.0.1 跨域）—— 历史最大坑
- **UC 进度用 g.progress.downloaded**（多连接分片下准确；fsutil queryValidData 是 VDL=最高已写偏移，尾部先写会虚高到 98%+ 卡住）—— 第 3 轮用户反馈坑
- **UC 速度用 downloaded 增量**（g.progress.speed 不可靠）
- **下载参数存 settings 表**（uc_connections/http_connections/max_running），maxRunning 启动时经 `started` 事件应用到 gopeed
- **UC 下载 connections 默认 300**（每连接限速 100KB/s，多连接叠加；设置页可调 1..1000）
- **NSIS-only 打包**（用户要求，MSI/WiX 已清理）
- **上下文 278k 自动压缩**（compaction.reserveTokens=722000，用户死命令）

### gopeed 关键 API
- 创建：`POST /api/v1/tasks` `{ req: { url, extra: { header } }, opts: { name, path, extra: { connections } } }`
- 列表/详情：`GET /api/v1/tasks`、`GET /api/v1/tasks/<id>`；`X-Api-Token` 头
- 任务状态：`ready/running/pause/done/error/wait`；**progress.downloaded 不可靠**，`progress.used` 含重试累计
- 全局配置：`GET/PUT /api/v1/config`（`protocolConfig.http.connections` 默认 500、`maxRunning` 默认 3）

### UC 接口（已验证真实可用）
- 解析：`https://pc-api.uc.cn/1/clouddrive/share/sharepage/detail?...`（stoken + pdir_fid）
- 直链：`POST https://pc-api.uc.cn/1/clouddrive/file/download?entry=ft&fr=pc&pr=UCBrowser`（fids + pwd_id + stoken + fids_token）
- 错误码：31001 未登录、23018 超出大小限制、41006 分享不存在
- UA 必须用 `uc-cloud-drive/2.5.20 Chrome/100... Electron/18.3.5.4`；Referer `https://drive.uc.cn/`
- 直链特征：**每连接限速 ~100KB/s，多连接叠加**；直链有效期 ~16h（Expires 参数）；OSS 流量限制 `x-oss-traffic-limit`

### 测试数据
- `ucAuth.txt`（项目根，已 gitignore）：`[url]` UC 分享链接（碧蓝航线/明日方舟 APK 等）+ `[cookie]`（__pugs=...）—— **用户提供的真实测试数据，勿外泄**

### 下载参数（settings 表，设置页可调）
- `uc_connections`：UC 直链并发连接数，默认 300（每连接限速 ~100KB/s，多连接叠加），范围 1..1000
- `http_connections`：普通 HTTP 链接并发连接数，默认 0（= gopeed 全局默认 500），范围 0..1000
- `max_running`：同时下载任务数，默认 3，范围 1..10（gopeed 全局 `maxRunning`，超出的任务 wait 排队）

### 打包 / 环境
- 安装包：`src-tauri/target/release/bundle/nsis/uc-drive2_1.0.0_x64-setup.exe`（42.5MB）
- sidecar：`src-tauri/binaries/node-x86_64-pc-windows-msvc.exe`（Node 24.12.0，89MB，tauri-build 要求相对 src-tauri）
- gopeed：`bin/gopeed/gopeed-web.exe`（82MB，headless CLI：`-A -P -T -d`）
- 后端测试：`cd server && npm test`；前端：`npm run test:unit`；打包命令见上
- 环境变量：Rust `PATH="$HOME/.cargo/bin:$PATH"`；rsproxy 镜像已配

### 参考项目
- 原版 `D:\alone\uc-drive`：Electron + aria2/gopeed 双引擎，UC 解析器 `src/main/modules/parser/uc-parser.js`、下载管理器 `download-manager.js`（aria2.changeUri 换链接不中断下载）
- X 网盘助手 `D:\Program Files\xzhushou`：已上线产品，`resources/app.asar` 可解包（`npx @electron/asar extract`），UC 走 aria2、gopeed 只用于百度盘（300 连接）
