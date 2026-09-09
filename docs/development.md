# 开发指南

## 开发环境

OPEN AIGC 当前以 Windows x64 为首要开发与发布环境。开始前运行：

```powershell
./scripts/check-prereqs.ps1
```

最低基线为 Node.js 20.10.0、pnpm 11.7.0、Rust stable MSVC、Visual Studio C++ 工具链、Windows SDK 和 WebView2 Runtime。仓库通过 `pnpm-lock.yaml` 与 `src-tauri/Cargo.lock` 固定依赖解析结果。

## 安装与启动

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm tauri dev` 是日常开发入口，会同时启动 Vite 与 Tauri 窗口。`pnpm dev` 只适合不依赖原生能力的界面调试，不代表桌面集成已验证。

## 质量门禁

```powershell
pnpm check
pnpm check:rust
pnpm test:rust
pnpm licenses:check
pnpm tauri build --no-bundle --ci
```

`pnpm check` 依次执行格式检查、零警告 Lint、Vitest、TypeScript 检查和前端生产构建。`pnpm check:rust` 执行 Rust 格式与零警告 Clippy，`pnpm test:rust` 使用真实临时 SQLite 文件验证 migration、升级前快照、失败恢复、事务、重开、版本上限和 checksum 漂移。提交前至少运行与改动相关的检查；合并共享边界前运行以上全部命令。

单独调试时可使用：

```powershell
pnpm test:watch
pnpm typecheck
pnpm lint
pnpm format
```

## 前端职责

| 路径 | 职责 |
|---|---|
| `src/app/` | 应用启动、路由、壳层和跨页面界面偏好 |
| `src/modules/` | 按用户能力组织的页面与模块私有代码 |
| `src/shared/` | 至少被多个模块稳定复用的无领域 UI 基元 |
| `src/styles/` | 设计令牌、密度、主题与全局基础样式 |
| `src/test/` | Vitest/jsdom 测试环境配置 |
| `src-tauri/` | Rust 桌面宿主及后续原生领域边界 |

界面偏好可以保存在 `localStorage`；对话、任务、凭据和重要草稿等业务数据不得存入浏览器存储。领域数据必须通过 typed IPC 与原生持久化服务读写。

## 直接依赖理由

| 依赖 | 用途与边界 |
|---|---|
| Vue 3 | 桌面管理界面的响应式视图层 |
| Vue Router | Hash 路由，兼容 Tauri 打包后的静态资源入口 |
| Pinia | 会话和查询投影；不充当领域数据库 |
| Lucide Vue | 一致、可访问的操作图标，避免维护自定义 SVG 集合 |
| Tauri JavaScript API | 仅调用版本化 Rust 命令和检测桌面运行时 |
| Zod 4 | 在 WebView 边界验证 IPC 请求、响应和结构化错误 |
| TanStack Table | 页面级排序和表格状态；无样式，不拥有领域数据或全局缓存 |
| Vue Flow | 无限画布的节点/连线渲染与交互，配合自研持久化与语义检索 |
| OGL | 轻量 WebGL 渲染，用于创意工坊背景光效 |
| Tauri 2 | Windows 宿主、安全 capability 和 Rust 原生边界 |
| Rusqlite + bundled SQLite | Rust 内部的本地领域数据访问，不向 WebView 暴露 SQL |
| Refinery | 嵌入不可变 SQL migration，记录 checksum 并检测漂移 |
| Vitest/Vue Test Utils | Store、路由和组件行为测试 |
| ESLint/Prettier | 严格静态检查和确定性格式化 |

未出现真实使用场景前，不引入新的表单、图表、数据库或 Provider 客户端依赖。新增依赖需要说明所有权、许可证、替换成本及其不能由现有依赖完成的原因。

## 当前能力边界

当前版本提供可运行的应用壳、创意工坊（Agent 对话 + 图片生成）、无限画布、模型与凭据管理、提示词库、受管资产库与备份恢复。生成任务与资产通过类型化 IPC 读写 SQLite，密钥经 OS 凭据管理器托管，应用不写入 `localStorage` 业务数据。音乐、配音等更多创作模式与插件能力按路线图推进。
