<p align="center">
  <img src="app-icon.png" width="96" alt="uc-drive2">
</p>

<h1 align="center">uc-drive2</h1>

<p align="center">
  <b>你的电脑就是网盘</b>：本地文件管理、离线下载、UC 网盘高速下载
  <br>
  单机、单用户、数据不出本机
</p>

## 功能

- 文件浏览、网格/列表视图、拖拽上传、预览、搜索、新建、重命名、移动和删除
- HTTP(S)、磁力和 `.torrent` 离线任务，支持暂停、继续、进度、速度和剩余时间
- UC 分享链接解析、目录浏览和多连接下载；直链过期自动刷新，Cookie 失效明确提示
- 下载完成后自动登记到网盘文件树，历史记录可查看和清理
- 存储目录可迁移，UC Cookie 使用 AES-256-GCM 加密保存在本机
- 关闭窗口进入托盘，下载可在后台继续
- 自动检查 GitHub 正式版本，签名校验后应用内更新；传输进行中可下载更新，完成或暂停后安装

## 架构

```text
uc-drive2.exe (Tauri 壳 + Rust HTTP 后端)
 └─ gopeed-web.exe (headless 下载引擎，唯一外部子进程)
```

Rust 后端在 Tauri 主进程内运行 axum HTTP 服务，只监听 `127.0.0.1`。默认端口为 `17210`，被占用时在 `17210..17229` 中选择可用端口，并将实际端口写入 `%APPDATA%/uc-drive2/server.port`。前端继续通过 Tauri `get_server_port` 或端口扫描发现服务。

SQLite 使用 `rusqlite bundled`，数据库文件为 `%APPDATA%/uc-drive2/data/uc-drive.db`。文件本体保存在 `storage/`，离线任务暂存于 `storage/offline/`，gopeed 数据保存在 `gopeed/`。已有数据库、数据目录和 `.secret` Cookie 密钥可直接继续使用。

## 安装与构建

项目目标为 Windows x64。安装包输出到 `src-tauri/target/release/bundle/nsis/`。

```bash
npm install
npm run tauri:build
```

安装包只携带 Tauri 应用和 `gopeed-web.exe` 下载引擎，不需要额外安装运行时。

首次构建需将 `build-deps/gopeed.json` 指定的固定引擎下载到 `bin/gopeed/gopeed-web.exe`，并核对 SHA-256。正式安装包由自动发布工作流完成签名与依赖准备。

## 开发

浏览器开发模式同时启动 Rust 后端和 Vite：

```bash
npm run dev
```

后端监听 `127.0.0.1`，Vite 默认监听 `5173`，浏览器访问 <http://localhost:5173>。也可以单独启动后端：

```bash
npm run dev:backend
```

如只使用 Tauri 窗口，运行 `npx tauri dev` 即可；Tauri 主进程会自行启动 Rust 后端。

## 测试

```bash
# Rust 后端：单元、文件 API、任务/gopeed mock API
cargo test --manifest-path src-tauri/Cargo.toml --all-targets

# Vue 前端
npm test
npx vue-tsc --noEmit
```

根目录的 `ucAuth.txt` 是本地测试数据，不会提交到仓库。文件包含 `[url]` 和 `[cookie]` 两段时，可以显式运行真实 UC 链路测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test uc_e2e -- --ignored --nocapture
```

该测试覆盖分享解析、递归找文件、直链预检、gopeed 下载、进度同步和完成后登记。真实测试依赖网络、有效 Cookie 和仓库中的 `bin/gopeed/gopeed-web.exe`。

## 应用内更新

从 **2.3.0** 开始支持应用内更新。旧版本首次需手动安装 2.3.0 或之后的版本。

设置页「软件更新」提供自动检查开关（默认开启）、手动检查、发布说明、下载进度和安装入口。启动 10 秒后检查，之后每 6 小时检查一次；每次运行对同一版本只自动提醒一次。更新源固定为本仓库正式 Release 的 `latest.json`，只接受更高的稳定语义版本。

用户确认下载后，Rust updater 校验签名，空闲时自动启动 NSIS 安装并重启。排队、运行中的下载，以及正在处理的上传、文件迁移和其他数据写入会阻止安装；已下载内容保留在本次运行内存中，待任务完成或暂停后手动点击「安装并重启」。关闭应用后需重新下载。签名错误禁止安装，网络失败可重试；安装器启动失败或取消提权时保留更新并恢复下载引擎。更新沿用原安装目录和本地数据目录。

浏览器模式禁用应用内更新；Tauri 开发模式允许检查，禁止下载和安装。

## License

[MIT](./LICENSE)
