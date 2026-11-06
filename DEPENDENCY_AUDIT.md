# 依赖精简记录

基线提交：`22c4bc5`。工作分支：`codex/prune-dependencies-audit`。目标平台：Windows x64。

审查了全部 16 个 npm 直接依赖、18 个 Rust 直接依赖（含构建依赖）、Gopeed 固定引擎，以及锁文件中它们的传递依赖来源和重复版本。仅移除已确认无用的直接依赖及其失去引用的传递链，没有修改第三方包或强行删除上游仍需安装的模块。保留包的锁定版本未升级。

## 移除与合并

| 原依赖 | 处理及证据 |
| --- | --- |
| `geist` | 未有包导入；CSS 直接使用 `src/assets/fonts/` 中的 4 个 WOFF2 文件。移除后保留字体外观，同时剪掉其 Next.js、React、Sharp 等依赖链。 |
| `unplugin-auto-import` | Vue、Router、Pinia 和 Naive UI API 均已显式导入。删除 Vite 插件及其生成的 `auto-imports.d.ts`，类型检查与构建通过。 |
| `unplugin-vue-components` | 使用的本地组件与 Naive UI 组件均已显式导入；Router 全局组件由 Vue Router 提供类型。删除插件及 `components.d.ts` 后类型检查与构建通过。 |
| `concurrently` | 仅用于同时启动开发后端和 Vite。以 44 行 Node 原生脚本替代，测试进程退出、终止信号及 Windows 孙进程清理后移除。 |
| `reqwest 0.12` | 更新器已有 `reqwest 0.13.4`。将后端和测试统一到现有版本，并移除无人调用的客户端 `multipart` 特性；服务端 axum 的上传支持保留。Native TLS、hyper-tls 等旧链退出当前 Windows 构建。 |
| `chrono` 直接依赖 | 全项目仅 `now_iso()` 调用。改用已存在的 `time` 并固定输出 UTC、3 位毫秒和 `Z`。当前 Windows 构建树不再包含 chrono；锁文件仍保留其他平台/可选传递依赖需要的记录。 |
| `rand` | 所有用途都是随机字节、随机整数或标识符。复用 AES-GCM 已依赖的 `rand_core::OsRng`，移除 rand、rand_chacha、ppv-lite86 等链。临时任务目录后缀使用随机 u64，不再限制为 0..1000；目录仍沿用 `task-时间戳-随机数` 格式。 |

npm 实际安装包数量：176 → 88；锁文件另删除了其他平台的可选包，共删除 119 条包记录。Windows 实际构建依赖树（普通依赖及构建依赖按包版本去重）从 330 → 317，减少 13 个 crate，没有新增包版本。被移出的记录为 chrono、num-traits、reqwest 0.12、hyper-tls、native-tls、schannel、tokio-native-tls、mime_guess、unicase、rand、rand_chacha、ppv-lite86、zerocopy。Cargo.lock 仍可保留其他平台和可选功能所需的记录；没有删除共享注册表缓存。

## 自写代码与复用

行数按最终源码物理行计，包含空行与注释，不含测试。

| 实现 | 行数 | 范围 |
| --- | ---: | --- |
| `scripts/dev.mjs` | 44 | 直接启动 cargo/Vite；任一退出或收到信号时结束其他服务；Windows 使用系统 taskkill 清理进程树。 |
| `now_iso` + `format_iso` | 17 | 复用 time 的日期计算，自写固定格式输出；没有自写日历算法。 |
| `http_client_builder` | 4 | 复用更新器已有的 Rustls/Ring，显式初始化提供者，避免依赖更新器的启动顺序。 |

随机调用只迁移至现有系统随机源，没有自写加密、伪随机数或 Base64 算法。`time`、`rand_core`、`rustls` 从已有传递依赖变为显式直接依赖，没有引入新的包版本。

## 保留的每个 npm 直接依赖

| 依赖 | 保留原因 |
| --- | --- |
| `@phosphor-icons/vue` | 文件、下载、设置和窗口控制等图标大量使用；现有图标体系。 |
| `@tauri-apps/api` | invoke、IPC Channel、窗口事件与原生窗口控制。 |
| `@tauri-apps/plugin-dialog` | 设置页原生目录选择。 |
| `naive-ui` | 全项目使用表格、弹窗、表单、通知、主题和中文本地化等组件。 |
| `pinia` | 文件、任务、设置、更新的响应式共享状态与 store 生命周期。 |
| `vue` | UI 框架、组件及响应式运行时。 |
| `vue-router` | 页面路由、导航和缓存页面生命周期。 |
| `@tauri-apps/cli` | 原生开发、构建及 NSIS 打包命令。 |
| `@vitejs/plugin-vue` | Vue SFC 编译。 |
| `typescript` | TypeScript 编译器与类型系统，vue-tsc 使用。 |
| `vite` | 开发服务器、代理、资源处理及生产打包。 |
| `vue-tsc` | Vue 模板与 TypeScript 类型检查。 |

## 保留的每个 Rust 直接依赖

| 依赖 | 保留原因 |
| --- | --- |
| `tauri-build`（构建） | Tauri 资源、能力和平台构建代码生成。 |
| `tauri` | Windows 窗口、托盘、命令与应用生命周期。 |
| `serde` | 请求/响应类型与序列化派生。 |
| `serde_json` | JSON API、数据库元数据与 Gopeed/网盘协议。 |
| `tauri-plugin-dialog` | 原生目录对话框；与前端插件配套。 |
| `tauri-plugin-updater` | 版本检查、签名验证、更新下载与安装流程。 |
| `axum` | 本地 HTTP 路由、流式响应、上传、提取器、中间件。 |
| `tokio` | 异步网络、文件、子进程、计时器、同步及开发入口 Ctrl+C。清单所列功能均有调用。 |
| `tokio-util` | ReaderStream 为大文件响应提供流式读取，避免整文件缓冲。 |
| `rusqlite` | 数据库、事务、预编译查询及 bundled SQLite。 |
| `reqwest` | Gopeed REST 与四家网盘 HTTP/HTTPS；保留 JSON、query、form、字符集和 HTTP/2 支持。 |
| `rustls` | 复用更新器已有的 Ring 提供者，统一 HTTPS 栈并明确初始化。 |
| `aes-gcm` | 凭据的已发布 AES-256-GCM 存储格式；不自写密码算法。 |
| `base64` | 加密凭据格式中的 IV、tag、data 编解码。体积小且是已有传递依赖。 |
| `time` | 时间戳日期计算；更新器/Tauri 已依赖，替代重复的 chrono。 |
| `rand_core` | 现有加密链已使用的系统安全随机接口，替代 rand 的完整生成器。 |
| `regex` | 分享文本 URL、中文提取码和 UC 文件夹定位；Tauri 同时依赖。重写 Unicode 与文本边界处理无明显体积收益。 |
| `percent-encoding` | 与 JS encodeURIComponent 一致的编码、百度会话解码。共享小依赖，保留既有字符语义。 |
| `url` | 分享链接、Host/Origin 和重定向地址解析；也是 reqwest/Tauri 共用依赖。 |

Gopeed 保留：唯一下载引擎，承担下载、磁力/种子、暂停继续和 REST 任务管理；无法用几十行代码替代。本机二进制 SHA-256 与 `build-deps/gopeed.json` 的固定值一致。

传递依赖中的重复版本也已复核：npm 剩余 `@vue/devtools-api` 6/7 与 `csstype` 3.0.11/3.2.3，来自 Vue/Router/Pinia/Naive UI 的版本要求；Rust 的 bitflags、getrandom、syn、thiserror、toml 和 windows-sys 等多个版本由不同上游 API/平台要求引入。没有应用层可统一的直接调用，保留上游支持的版本约束。Cargo 显示的相同版本重复项还包括构建与运行上下文分离，不能当成可删除的重复包。

## 大依赖的判断

没有遗留用途不明、需要阻塞本次删除决定的大依赖。以下项目明确被使用，因此保留；进一步精简需要另行决定是否更换现有架构或 UI 体系：

| 大项 | 本机包文件大小 | 判断 |
| --- | ---: | --- |
| `naive-ui` | 49.48 MiB | 全界面依赖，不能按小工具库移除。其传递依赖 date-fns（10.40 MiB）和 highlight.js（5.25 MiB）由上游模块图引用，不能单独卸载。 |
| `@phosphor-icons/vue` | 14.82 MiB | 多页面大量使用并按需打包。手工维护整套图标超出几十行替代的范围。 |
| TypeScript | 22.53 MiB | 开发期必需，不随桌面安装包分发。 |
| Gopeed | 78.24 MiB | 桌面安装内容的主要体积，维持现有下载引擎。 |

## 测量与验证

临时基线目录的递归清理命令被自动审批策略拒绝，仅返回 `blocked by policy`，未提供进一步原因。临时快照保留在 `%TEMP%/folder-drive-npm-baseline-Tn0iJq/`，表中数据是安装体积对比，并非本机净释放空间。

npm 比较在同一 Windows/Node 环境下对基线与修改后的锁文件分别执行 `npm ci --ignore-scripts --no-audit --no-fund`，累加 node_modules 内文件逻辑字节，跳过符号链接。不包含 npm 全局缓存、历史 Vite 缓存、Cargo target、Cargo 注册表缓存和文件系统簇开销。

| 项目 | 修改前 | 修改后 | 减少 |
| --- | ---: | ---: | ---: |
| 全新 node_modules | 522,406,782 B（498.21 MiB） | 173,959,461 B（165.90 MiB） | 348,447,321 B（332.31 MiB，66.70%） |
| 正式生产主程序 | 8,899,072 B | 8,645,120 B | 253,952 B（248 KiB） |
| 安装内容：主程序 + Gopeed | 90,939,392 B | 90,685,440 B | 253,952 B（248 KiB） |
| NSIS 压缩安装包 | 21,556,547 B | 21,488,507 B | 68,040 B（66.45 KiB） |

前端 `dist` 在隔离的基线源码与精简源码下逐文件比较 SHA-256，完全一致，均为 1,249,393 B。桌面主程序与 NSIS 安装包使用 Tauri CLI 的正式生产模式（包含 custom-protocol）构建，基线来自原始提交的独立临时快照；两次均使用相同临时配置关闭更新签名产物，未安装或发布。安装内容统计主程序与 Gopeed 文件的逻辑大小，不含动态生成的用户数据、外部 WebView2 运行时或文件系统簇开销。npm 开发依赖不随桌面应用分发，因此其 332.31 MiB 的减少量单独列出。

已按逻辑变更分组运行测试和构建。新增测试覆盖进程管理的 5 种场景、UTC 格式的日历/精度/时区边界，以及独立 AES-GCM 旧格式样本、新 IV 和新密钥兼容性。网盘测试使用本地 HTTPS 夹具及真实 Gopeed，覆盖解析、加密保存、分页、错误处理、转存、下载、暂停继续、登记和重新下载。

- `npm test`
- `npm run check`
- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib --test dependency_replacements`
- `npm run test:cloud`
- `cargo build --manifest-path src-tauri/Cargo.toml --locked`
- `cargo build --manifest-path src-tauri/Cargo.toml --locked --release`
- `tauri bundle --bundles nsis --ci --no-sign`（临时配置关闭更新签名产物）
- `npm run tauri -- build --config <临时配置> --bundles nsis --ci --no-sign`（基线与修改后均成功，包含生产模式和 NSIS 打包）

首次合并 reqwest 时，HTTPS 测试发现缺少 Rustls 提供者；已改为显式共用 Ring 并重跑全部 12 项网盘测试通过。最终共通过 19 项测试（5 项 Node、2 项时间/凭据、12 项网盘集成），类型检查、前端构建、Rust 构建和 Tauri 生产打包均通过。首次正式构建遇到旧 Tauri 缓存中的失效绝对路径，定向清理相应 release 编译缓存后重建成功。新测试已接入发布工作流。未执行真实账号联网验收或安装器安装。
