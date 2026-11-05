# folder-drive 架构与 Rust 后端集成

## 目标

将原本独立的本地 HTTP 后端改为 Rust 实现并集成到 Tauri 主进程，移除 Node 后端运行时和 sidecar，保留 Vue 前端、HTTP API、SQLite 数据和 Gopeed 下载引擎；统一支持 UC、夸克、百度、迅雷官方分享下载。

## 最终架构

```text
folder-drive.exe
  ├─ Tauri v2 壳
  ├─ Rust axum HTTP 后端（进程内，127.0.0.1:17210 起步）
  └─ gopeed-web.exe（唯一外部下载子进程）
```

端口冲突时最多尝试 20 个连续端口。Rust 后端绑定成功后写入 `%APPDATA%/folder-drive/server.port`，前端继续使用 Tauri `get_server_port` 和浏览器端口扫描逻辑。

## Rust 模块

- `backend/config.rs`：数据目录、默认/运行时存储目录和路径句柄
- `backend/db.rs`：SQLite WAL、外键、旧表 metadata 迁移
- `backend/crypto.rs`：兼容旧格式的 AES-256-GCM Cookie 存储
- `backend/files.rs`：文件树、上传、Range/HEAD 下载、搜索、移动、重命名、回收站删除
- `backend/gopeed.rs`：gopeed 子进程、随机端口/token、REST 客户端、健康检查和自动重启
- `backend/tasks.rs`：任务生命周期、轮询同步、进度/速度计算、UC 直链刷新和完成登记
- `backend/uc.rs`：UC 分享解析、目录分页、递归文件查找、直链和预检
- `backend/cloud.rs`：夸克、百度、迅雷官方分享会话、目录列表、直链及凭据处理
- `updater.rs`、`backend/update_gate.rs`：签名更新和安装写入互斥
- `backend/routes.rs`：现有 `/api/*` HTTP 接口、CORS、multipart 和错误映射
- `backend/access_log.rs`：批量写入访问日志

## API 兼容要求

保留健康检查、设置、存储目录迁移、文件 CRUD、目录树、搜索、祖先链、Range/HEAD 下载、临时 torrent 上传、任务配置和生命周期、UC 与通用 cloud parse/list-folder/download、cloud redownload/credentials、Cookie、history 全部接口。

响应字段继续使用前端现有的 snake_case 文件/任务字段和 camelCase 设置/UC 字段。仅允许 Tauri 和回环网页来源，校验回环 Host；保留 OPTIONS 预检、Range 响应头及中文错误消息。

## 数据兼容

- 数据库为 `%APPDATA%/folder-drive/data/folder-drive.db`。
- 文件树和任务表结构不变；旧 tasks 表缺失 `metadata` 时自动迁移。
- 存储根默认为 `%APPDATA%/folder-drive/storage`，设置接口支持迁移并同步 DB 路径。
- Cookie 密钥仍为 `.secret` 的 32 字节内容，密文格式为 `v1:{iv_b64}:{tag_b64}:{data_b64}`。
- `storage/offline/` 用于 gopeed 任务临时文件，完成后登记到文件树。

## 构建与运行

- `npm run dev` 同时启动 `folder-drive-server` Rust 开发入口和 Vite。
- `npx tauri dev` 使用 Tauri 主进程内的 Rust 后端。
- `npm run tauri:build` 只构建 Vue 前端，再构建 Release Tauri/NSIS 包。
- 包内保留 `gopeed-web.exe` 资源，不再配置 Node sidecar 或旧后端资源。

## 稳定版本维护

现有 `npm run test:cloud` 通过本地 HTTPS 官方接口契约夹具和真实 Gopeed 覆盖多网盘下载。项目检查、构建和验证命令仅在用户明确要求时执行。
