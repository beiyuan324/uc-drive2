\# uc-drive2 轻量化重构方案：Tauri + Vue3/Naive UI + Node/Express + gopeed



\## 摘要



以 `D:\\alone\\uc-drive` 为功能参考，在 `D:\\alone\\uc-drive2` 全新重写单用户桌面网盘：桌面壳 Electron 换 Tauri v2，渲染层 Vue3 + Vite + TypeScript + Naive UI，后端 Node.js + Express，存储 SQLite，下载引擎完全弃用 aria2、改用 gopeed（Node 后端托管子进程 + REST API）。UI 按 Design Taste Frontend 技能的设计体系重构（原项目代码不迁移，只参考功能与交互）。



\## 目标架构



进程模型（三级级联，随应用启停）：



\- Tauri 主进程通过 externalBin 拉起 Node 后端（sidecar），Node 后端再拉起 gopeed.exe；退出时 Tauri 关闭 Node 后端，Node 负责关闭 gopeed，保证无残留进程。

\- 后端绑定 `127.0.0.1:17210`，端口被占用自动 +1；前端通过 Tauri command `get\_server\_port()` 获取实际端口，不暴露公网。

\- 用户数据目录 `%APPDATA%/uc-drive2/`，含 `data/uc-drive.db`（SQLite）、`storage/`（网盘文件根目录）、`gopeed/`（配置与临时文件）。



```

uc-drive2/

├── src-tauri/          # Tauri v2 壳，externalBin 声明 node 运行时与 gopeed.exe

├── src/                # Vue3 渲染层（views: 文件管理 / 离线下载 / 设置）

├── server/             # Express 后端（routes / services / gopeed 托管模块）

└── package.json        # workspace 根

```



\## 视觉设计体系（按 Design Taste Frontend 技能）



设计读法（先声明）：\*Reading this as: 单用户桌面网盘产品（file manager / product UI），面向个人效率用户，calm-premium 语言，leaning toward 定制化 Naive UI 主题 + Geist 西文字体 + 单一 Emerald accent + Zinc 中性基底。\*



三档调参（覆盖基线，理由：产品 UI 信息密集、动效克制）：`DESIGN\_VARIANCE: 4`、`MOTION\_INTENSITY: 3`、`VISUAL\_DENSITY: 6`。



\- 字体：西文自托管 Geist（woff2，`font-display: swap`），中文走系统栈（PingFang SC / Microsoft YaHei / Noto Sans SC）；不使用 Inter；字号体系：标题 20/24px semibold、正文 14px、辅助 12/13px。

\- 色彩：单一 accent Emerald `#059669`（悬停 `#047857`），中性基底 Zinc（浅色 `#FAFAFA` 底 / `#18181B` 文字，深色 `#18181B` 底 / `#E4E4E7` 文字）；语义色仅用于状态（成功/警告/错误）；无渐变、无紫色发光、无玻璃拟态泛滥、无纯黑投影。

\- 形状：统一圆角系统，控件 8px、面板与列表容器 12px（规则文档化并全局一致）；阴影只在使用层级处出现且带底色 tint。

\- 布局：左侧导航栏（文件 / 离线下载 / 设置）+ 顶栏（面包屑、搜索、视图切换、主题切换）+ 内容区；文件区支持网格/列表两种视图，空状态、加载 skeleton、错误态齐全；上传区拖拽高亮。

\- 图标：`@phosphor-icons/vue` 单一图标体系，stroke 全局统一，不用 emoji、不手绘 SVG 路径；文件类型图标用 Phosphor 对应 glyph。

\- 主题：light / dark / auto 三态，整页单主题锁定，不混排；动效仅用于状态反馈与层级过渡，适配 `prefers-reduced-motion`。

\- 文案：全中文，无英文 em-dash，无装饰性页脚/版本号/滚动提示；所有文本与控件过 WCAG AA 对比度检查。



\## 关键实现



1\. 桌面壳（Tauri v2）：sidecar 采用「官方 node.exe + server 目录」形态（规避原生模块单文件打包问题）；gopeed 使用官方 Windows x64 二进制；两者随 NSIS/MSI 安装包分发。开发模式 `tauri dev` + Vite + 本地 node。



2\. 前端（Vue3 + Vite + Pinia + Vue Router + Naive UI）：Naive UI 用 unplugin 按需自动导入，通过 `themeOverrides` 注入上述设计 token（accent、中性色、圆角、字号、组件密度），自定义布局用 scoped CSS，不引入 Tailwind 与 Element 相关依赖；三个页面：文件管理（浏览/上传/下载/新建目录/重命名/移动/删除/预览）、离线下载（新建 URL/磁力/torrent 任务、进度/速度/暂停/删除）、设置（存储目录、端口信息、关于）。



3\. 后端（Express + better-sqlite3）：文件模块直接操作文件系统，SQLite 只存元数据；上传用 multipart，下载支持 HTTP Range（图片/视频/音频预览与断点续传）；对外仅本机监听。



4\. gopeed 集成：后端启动时生成随机 token，headless 拉起 gopeed（127.0.0.1 随机端口），health 轮询就绪后再提供服务；异常退出自动重启并重新同步任务。离线任务固定下载到 `storage/offline/<taskId>/`，优先订阅 gopeed WebSocket 事件（不支持则轮询），完成后把文件登记进文件树；创建任务支持 URL、磁力、torrent 三种来源。



5\. 接口与数据：

&#x20;  - REST API：`GET/POST /api/files`、`GET /api/files/:id/download`（Range）、`POST /api/dirs`、`PATCH /api/files/:id`、`DELETE /api/files/:id`、`POST /api/tasks`、`GET /api/tasks`、`POST /api/tasks/:id/{pause,resume,delete}`、`GET /api/health`。

&#x20;  - SQLite：`files(id, name, parent\_id, is\_dir, path, size, mime, created\_at, updated\_at)`、`tasks(id, gopeed\_id, source, status, progress, target\_dir, created\_at)`。



\## 测试计划



\- 后端：supertest 覆盖文件 CRUD、Range 下载、上传元数据一致性；gopeed 客户端用 mock 服务测建任务/暂停/删除/异常重启；本地 HTTP 文件服务器做真实端到端（URL 任务 → 完成 → 登记入树）。

\- 前端：Vitest 覆盖状态与关键组件；手动验收覆盖上传下载预览、离线任务全流程、暗色模式。

\- UI 质量门：按技能 Pre-Flight 中适用于产品 UI 的条目逐项验收，包括主题锁定、单 accent 颜色一致、圆角一致、按钮/表单对比度、图标体系单一、无 em-dash、loading/empty/error 三态齐全、reduced-motion、暗色模式 token、无 AI 默认观感（无紫渐变、无三等分卡片、无装饰性元素）。

\- 桌面端：安装包安装、冷启动、端口冲突自增、退出后无 node/gopeed 残留进程；磁力/BT 手动冒烟（依赖网络）。



\## 假设与默认值



\- 原项目仅作功能参考，代码全部重写，不做旧数据/数据库迁移，首次启动建新库。

\- 平台目标 Windows x64；Tauri 打包 NSIS 与 MSI。

\- gopeed 具体启动参数与 API 端点以官方文档（gopeed.com/zh/docs）为准，实施第一步核对；接口语义按本方案固定，不一致处由后端封装层收敛。

\- gopeed 仅承担离线下载；普通文件上传/下载/预览由 Express 直接处理文件系统。

\- 单用户本地网盘，无登录鉴权；后端只监听 127.0.0.1。

\- Design Taste Frontend 技能仅应用于其适用范围（产品 UI 的通用设计工程规则），营销落地页专属规则不套用。



