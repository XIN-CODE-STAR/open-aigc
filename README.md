# OPEN AIGC

**OPEN AIGC** 是一款基于 Tauri 2 构建的 Windows 桌面 AI 创作工作台。它把 Agent 对话、图片生成、无限画布、资产管理和提示词库整合进一个本地优先的应用，面向希望在一个受控环境中完成 AI 创作流程的内容创作者。

核心理念：**对话即创作**。你在创意工坊里用自然语言描述需求，内置的创意 Agent 负责理解意图、规划步骤、调用工具并完成生成，全过程可视、可追踪、可恢复。

## 功能总览

### 创意工坊（Agent 模式）

- **对话式创作**：与创意 Agent 多轮对话，Agent 自动拆解需求并执行
- **执行过程可视化**：规划、工具调用、生成、整理结果每一步都有时间线展示
- **图片生成**：文生图与图生图，生成结果自动进入受管资产库
- **上下文记忆**：对话历史、画布内容与资产语义纳入 Agent 工作记忆，支持连续创作

### 无限画布

- 节点式创作空间：便签、图片、参考素材自由排布
- 语义连线：拖拽建立节点关联，双击删除
- Agent 可直接操作画布（创建节点、检索内容、自动关联），把"思考过程"落到画布上
- 视口（缩放/平移）与节点位置自动持久化

### 资产管理

- **受管文件库**：所有导入与生成的文件统一收纳，SHA-256 完整性校验
- 资产清单（manifest）、隔离区、批量复检
- 与生成任务双向关联：每个结果都能追溯到任务与提示词

### 提示词库

- 可复用提示词模板、变量占位与版本管理
- 创意工坊中通过 `/` 快速插入模板

### 模型与凭据

- 多模型管理：对话模型、图片模型统一配置与切换
- 凭据安全：API 密钥存入操作系统凭据管理器（Windows Credential Manager），数据库只保存引用键，明文密钥永不落库
- 账号健康状态监控

### 系统能力

- **备份与恢复**：一键打包数据库与受管文件，恢复前自动校验 schema 版本并创建安全备份
- **服务热重载**：恢复备份后无需重启应用
- 深色/浅色主题、界面密度与导航偏好

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面运行时 | Tauri 2（WebView2） |
| 前端 | Vue 3 · TypeScript · Vite 6 · Pinia 3 · Vue Router 4 |
| 后端 | Rust（领域驱动分层：domain / application / adapters / ports / ipc） |
| 数据 | 本地优先 SQLite（checksum 迁移 + 事务性备份） |
| 校验 | 前后端双层 zod / serde 严格校验，IPC 拒绝未知字段 |
| 视觉 | OGL WebGL 背景（棱镜光效）、Lucide 图标 |
| 测试 | Vitest（前端）+ cargo test（Rust） |

## 架构一览

```
┌──────────────────────────────────────────────┐
│      前端 Vue 3（创意工坊 / 画布 / 资产库）      │
└───────────────────┬──────────────────────────┘
                    │ Tauri IPC（类型化命令 + 事件）
┌───────────────────┴──────────────────────────┐
│   Rust 应用层（Agent / 生成管线 / 凭据 / 备份）  │
├──────────────────────────────────────────────┤
│   适配层（SQLite 仓储 · Provider 适配器 · 画布） │
└───────────────────┬──────────────────────────┘
                    │
        SQLite 工作库 + 受管文件目录 + OS 凭据管理器
```

关键设计决策：

- **端口与适配器**：领域逻辑只依赖 `ports` trait，SQLite/Provider 实现可替换
- **凭据边界**：密钥只存在于 OS keychain，业务表仅存引用键
- **受管文件流**：生成结果强制走 staging → hash 校验 → 原子落库，失败自动隔离
- **可恢复性**：长任务状态持久化，应用重启后可继续；备份恢复带 schema 一致性快照

## 本地开发

环境要求：

- Windows x64 + WebView2 Runtime
- Node.js ≥ 20.10.0、pnpm 11.7.0（`packageManager` 已固定）
- Rust stable MSVC（rustfmt + clippy）、Visual Studio C++ 工具链与 Windows SDK

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev      # 启动桌面开发窗口
```

质量校验：

```powershell
pnpm lint           # ESLint
pnpm test           # Vitest
pnpm typecheck      # vue-tsc
pnpm build          # 类型检查 + 生产构建
pnpm test:rust      # Rust 测试（含 Common-Controls v6 manifest 处理）
pnpm check:rust     # cargo fmt --check + clippy
```

浏览器模式可用 `pnpm dev` 快速调试前端；涉及文件、凭据、数据库等系统能力时请使用 `pnpm tauri dev`。更多细节见[开发指南](docs/development.md)。

## 目录结构

```
src/                    # Vue 3 前端
  app/                  # 应用壳、路由、导航、Pinia store
  bridge/               # Tauri IPC 桥接层（zod 严格校验）
  modules/              # 业务模块（generations / memory / assets / prompts / models / settings / backup）
  shared/               # 共享 UI 组件与视觉组件
src-tauri/              # Rust 后端
  domain/               # 领域模型与校验
  application/          # 应用服务（Agent、生成、凭据、备份等）
  adapters/             # SQLite 仓储与 Provider 适配器
  ports/                # 仓储/服务端口 trait
  ipc/                  # Tauri 命令层
  migrations/           # SQLite 版本化迁移
```

## 路线图

- 音乐与配音生成模式
- 画布多人协作素材组织
- 插件系统与受控 Host API
- 安装包签名与自动更新

## 参与贡献

见 [CONTRIBUTING.md](CONTRIBUTING.md)。提交遵循 Conventional Commits。

## License

MIT License，详见 [LICENSE](LICENSE)。
