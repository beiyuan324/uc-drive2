# uc-drive2

单用户桌面网盘 —— 本地文件管理 + 离线下载 + UC 网盘高速下载，完全离线运行。

**技术栈**：Tauri v2 壳 · Vue3 + TypeScript + Naive UI + Pinia · Node.js/Express 后端 · SQLite 元数据（node:sqlite，零原生模块）· gopeed 下载引擎

## 架构

```
uc-drive2.exe (Tauri)
 └─ sidecar: node.exe → server-dist/server.js (Express 后端, 127.0.0.1:17210, 占用自动 +1)
     └─ gopeed-web.exe (下载引擎, headless, 随机端口 + 随机 token)
```

- 后端仅监听 `127.0.0.1`，端口被占用时自动 +1（探测式分配）
- **CORS 已内置**：WebView（origin=tauri.localhost）→ 127.0.0.1 属跨域，后端统一放行（含 Range 头）
- 退出级联关闭：托盘「退出」→ taskkill 进程树（node 及 gopeed）
- 数据目录：`%APPDATA%/uc-drive2/`（`storage/` 默认网盘根、`gopeed/` 引擎数据、`data/uc-drive.db`、`access.log` 访问日志）
- **网盘存储目录可自定义**：设置页可改为任意磁盘路径（支持目录选择器），保存时可选把现有文件迁移到新目录（支持跨盘），重启后自动恢复；"恢复默认目录"一键回到 `%APPDATA%/uc-drive2/storage`
- 离线任务下载到 `storage/offline/<taskId>/`，完成后自动登记进文件树

## 功能

- **文件管理**：浏览（网格/列表）、上传（多选/拖拽）、新建目录、重命名、移动、删除（确认弹窗）、
  Range 下载（206 断点续传）、预览（图片/视频/音频/文本）、全局搜索
- **UC 网盘解析**：粘贴分享链接（支持批量）→ 会话建立 → 文件列表（文件夹浏览/递归）→ 创建下载任务；
  **默认 300 连接并发下载**（UC 直链每连接限速 ~100KB/s，多连接线性叠加，1.98GB 实测主体 12s 完成）；
  直链过期自动刷新重试（最多 5 次/30s 冷却），Cookie 失效自动识别（`cookie_expired` 状态）
- **离线下载**：HTTP(S) 链接 / 磁力链接 / .torrent 文件，真实进度（NTFS 有效数据长度统计，规避 gopeed 进度字段缺陷）、
  速度、暂停/继续/删除；gopeed 异常退出自动换端口重启并恢复任务；下载完成弹系统通知
- **历史记录**：已完成/失败任务归档，支持重新下载、复制链接、清空
- **系统托盘**：关闭窗口最小化到托盘后台运行，托盘菜单（显示窗口/下载管理/退出）
- **设置**：存储目录自定义（浏览/输入/迁移文件/恢复默认）、服务状态（后端/gopeed 端口）、UC Cookie（AES-256 加密存储）、
  主题（light/dark/auto）

## 开发

```bash
npm install                # 前端依赖
npm --prefix server install # 后端依赖

npm run dev                # 同时启动 Express 后端(17210) + Vite(5173, 代理 /api)
npm run test               # 后端测试（node --test，含真实 gopeed e2e）
npm run test:unit          # 前端 Vitest
npm run build              # 前端产物 (dist/)
```

浏览器访问 http://localhost:5173 即可在纯 Web 模式下开发调试；
打包后前端自动通过 `get_server_port` 读取后端真实端口。

## 桌面端构建

```bash
npm run tauri build        # NSIS 安装包 → src-tauri/target/release/bundle/nsis/
```

构建说明：
- sidecar：Node 24 官方 win-x64 二进制改名为 `src-tauri/binaries/node-x86_64-pc-windows-msvc.exe`
- 后端用 esbuild 打包为单文件 `server-dist/server.js`（零 node_modules 分发）
- gopeed：`bin/gopeed/gopeed-web.exe`（v1.9.3 web 构建，headless CLI 模式）
- 无任何原生模块（node:sqlite 内置驱动，规避原生模块打包）
- resources 对象映射扁平化 + Rust 多布局探测（兼容 tauri 2.5+ `_up_/` 布局，自动剥离 `\\?\` 长路径前缀）

## 设计体系

Geist 自托管字体（中文回退系统栈）· Emerald `#059669` 单一强调色 · Zinc 中性基底 ·
控件圆角 8px / 面板 12px · Phosphor 图标 · 动效克制并适配 `prefers-reduced-motion` · 全中文界面

## 测试

- 后端 34 项（`cd server && npm test`）：文件 CRUD / Range / 上传一致性 / 路径穿越防护 / 移动循环防护 /
  gopeed 管理器（含异常重启）/ 任务服务 / 真实 gopeed e2e / CORS 回归 / cookie 加解密 / UC 链接解析 /
  **UC 真实链路 e2e**（解析→直链→gopeed 下载→登记，需根目录 `ucAuth.txt` 含 `[url]`+`[cookie]`，缺省自动 skip）
- 前端 22 项 Vitest（`npm run test:unit`）：图标映射、settings 主题三态、tasks 轮询与操作流转、api 启动竞态回归

## 交接

详细的项目记忆、待办与下次会话指南见 [HANDOVER.md](./HANDOVER.md)。
