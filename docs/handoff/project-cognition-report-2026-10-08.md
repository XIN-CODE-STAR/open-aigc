# OPEN AIGC 项目全景认知报告

> 生成时间：2026-10-08
> 数据来源：仓库源码（src/ · src-tauri/ · services/）、docs/ 全部文档、各 AI Agent 遗留状态目录（.mimosa · .zcode · .claude · .codex-run · .trae）、git 提交历史
> 目的：整合项目背景、架构、核心模块、业务逻辑、已有决策、未解决问题与 Agent 协作关系，形成可接续工作的完整认知

---

## 一、项目定位与背景

**OPEN AIGC**（包名 `aigc-studio`，v0.2.0）是一款 **Windows 桌面端本地优先 AI 创作工作台**，基于 **Tauri 2 + Vue 3 + Rust** 构建。

- **核心理念**：「对话即创作」。用户用自然语言描述需求，内置创意 Agent 负责理解意图 → 规划步骤 → 调用工具 → 完成生成，全程可视、可追踪、可恢复。
- **产品本质**（来自 `docs/product/2026-07-21-creative-os-tech-decisions.md` 终稿）：不是通用 Agent 框架、不是代码工具、不是聊天机器人，而是**本地 AI 创作操作系统**——解决「一个任务需要多个模型、多个阶段、多次返工才能产出完整成品」的**创意生产调度问题**。
- **核心壁垒（三大自研）**：Cognitive Router + Capability Registry（智能调度）、Creative Workflow（DAG + Feedback Loop）、Artifact System（作品资产依赖追踪）。
- **硬约束**：单进程（Agent 与 UI 同进程）、零外部运行时依赖（核心逻辑纯 Rust，仅模型调用联网）、离线可用。

---

## 二、技术栈与整体架构

| 层 | 技术 |
| --- | --- |
| 桌面运行时 | Tauri 2（WebView2，Windows x64 优先） |
| 前端 | Vue 3 · TypeScript · Vite 6 · Pinia 3 · Vue Router 4（Hash 路由）· Vue Flow（画布）· OGL（WebGL 背景）· Zod 4（IPC 校验） |
| 后端 | Rust（DDD 分层：domain / application / adapters / ports / ipc） |
| 数据 | 本地 SQLite（rusqlite + bundled，Refinery 版本化迁移 + checksum 漂移检测） |
| 校验 | 前后端双层 zod / serde 严格校验，IPC 拒绝未知字段 |
| 测试 | Vitest（前端）+ cargo test（Rust，经 `scripts/rust-test.mjs` 外部 manifest 门禁） |

**架构数据流**：
```
用户消息 → GenerationsPage → useAgentConversation → Tauri IPC (agent_v1_send_message)
  → AgentService.send_message() → 规划(Planning) → 执行(Execution/ReAct 循环)
  → BuiltinToolExecutor.execute() → CognitiveRouter 选 Provider → ProviderRegistry.resolve()
  → UnifiedProviderAdapter.submit/poll/download → 结果经 Tauri Event 推送前端
```

**Rust 后端分层**（`src-tauri/src/`，233 个 .rs 文件）：
- `domain/` — 领域模型（agent、creative_plan、creative_state、canvas、runtime_event、workflow、tool_catalog、credentials 等 30 个）
- `application/` — 应用服务（约 75 个：agent_service、model_router_service、generation_pipeline、creative_runtime_service、canvas_memory_rag、memory_service、workflow_service 等）
- `adapters/` — 适配器（sqlite 仓储 25 个、providers 11 个、agent/builtin_tools、everos、embedding/video_parser/file_vault）
- `ports/` — 端口 trait（约 36 个：unified_provider、agent_llm、credential_repository、embedding_port 等）
- `ipc/` — Tauri 命令层（23 个模块）
- `migrations/` — SQLite 版本化迁移

**前端结构**（`src/`，132 文件）：
- `app/` — 应用壳、路由、Pinia store；`app/shell/` 已拆出 AppSidebar/AppTopbar/ConversationHistoryPanel
- `bridge/` — Tauri IPC 桥接层（约 30 个模块，zod 校验）
- `modules/` — 业务模块：generations（创意工坊，核心）、memory（记忆画布）、assets、prompts、models、backup、settings、plugins
- `shared/` — 共享 UI 原语（BaseButton/Input/Select/Badge/Tabs/Switch、ModalDialog、EmptyState、LoadingSkeleton、useToast）与视觉（AuroraCanvas）
- `styles/` — tokens.css / base.css（设计令牌驱动主题）

---

## 三、核心模块与业务逻辑

### 1. Agent 对话系统（创意工坊核心）
`useAgentConversation.ts` ↔ `agent_service.rs`。两阶段流水线：**规划（Planning）→ 执行（Execution）**，执行采用 ReAct 循环。特性：多轮持久化、工具调用时间线、流式输出、Plan-and-Execute、记忆注入、用户提问中断（UserQuestionAsked）。
- **内置工具**（`adapters/agent/builtin_tools.rs`）：`image_generation`、`video_generation`、`list_credentials`、`current_time`、`analyze_reference_image`，以及画布三件套（`canvas_search`/`canvas_add_note`/`canvas_connect`）等共 12 个。
- **工具分类**：ReadOnly / Draft / Write / Costly，Costly 必须确认。
- **关键优化**：单步快速路径（图片/视频意图直接建 1 步计划，跳过 Director/LLM 规划）；生成完成后**回填** invocation 的 result_json 并推送 ToolInvocationUpdated。

### 2. 模型路由与 Provider 基础设施
- **Cognitive Router**（`model_router_service.rs`）：按认知阶段分配资源，策略有 CostOptimized/QualityFirst/SpeedFirst/Balanced/UserSpecified；**核心原则：LLM 不直接指定模型名**，只输出任务需求，Router 决定资源。
- **Capability Registry**（SQLite `model_capabilities` 表）：能力向量（8 维度）+ 硬约束 + 失败学习；新模型 = INSERT 一行，不改代码。
- **三代 trait 收敛**（见 `docs/product/2026-07-23-provider-infrastructure-decisions.md`）：统一到 `UnifiedProviderAdapter`（submit/poll/download/health），引入 `CredentialType` 枚举 + `CredentialContext` 中间层 + `ProviderDescriptor` 类型化注册 + `ProviderFactory`。
- **凭据安全边界**：API Key 只存 OS 凭据管理器（keyring，service=aigc-studio），数据库只存引用键 `credential:{uuid}`；Adapter 无状态，运行时经 `CredentialManager.resolve()` 实时取密钥。
- **Browser/RPA 型接入**（Midjourney/即梦/可灵网页）划归独立 `AutomationChannel`，不进 GenerationProvider 抽象。

### 3. 生成管线与资产
- `generation_pipeline.rs`：单任务生成 + `project_output::export_generated_file` 导出到用户工作目录。
- `generation_queue_service.rs` / `poll_worker.rs`：单任务队列与异步轮询（重启后按 remote_job_id 续轮）。
- **受管文件流**（Managed Storage）：staging → SHA-256 校验/去重 → 原子移动到 assets/ → 注册 `asset_manifest` 表；失败进 quarantine。`assetKind` 支持 image/video/audio。
- 前端经 `convertFileSrc` 转 asset.localhost 协议展示（CSP 已放行 img/media）。

### 4. Creative 创作流水线（分镜成片）
- `creative_runtime_service.rs`：**四阶段流水线** Submit → Poll → Import → Compose，带 `RuntimeEventBus`。
- `creative_plan_builder.rs` + `domain/creative_plan.rs`：CreativeBrief → CreativePlan（shots 含 duration_secs）。
- `composition_facade.rs`：`CompositionFacade` trait，真实实现 `FfmpegCompositionFacade`（归一化转码 + concat），无 ffmpeg 时 Mock 回退。
- `composite_skill.rs`：`detect_ffmpeg()` 探测链 = sidecar → PATH。
- `project_folder.rs`：`.openaigc/` 项目文件夹（project.json + AIGC.md 项目记忆，随工作目录落地，经 systemPromptOverride 注入 Agent）。

### 5. 记忆系统（画布 + RAG + 长期记忆）
- **无限画布**（`modules/memory/`，vue-flow）：节点（便签/图片）、语义连线、视口持久化、命令模式 Undo/Redo、右键菜单、缩放、导出 PNG/JSON。
- `canvas_memory_rag.rs` + `canvas_memory_repository.rs`：RAG 检索注入 Agent 工作记忆；DB 表 memory_canvases/memory_nodes/memory_edges/memory_canvas_viewports。
- **混合向量检索**：嵌入基础设施 + 画布索引 + 优雅降级（凭据不支持 /embeddings 时自动降级为关键词+邻接）。
- **EverOS 长期记忆**：Python 记忆服务（`adapters/everos/`，端口 18000），存储 `<workspace>/everos-memory/`，可选启用。

### 6. 工作流与多 Agent
- `workflow_service.rs`：9 阶段状态机（Generating → Evaluating → WaitingFeedback → … → Completed/Failed）+ 事件追踪 + 暂停/恢复。
- `domain/agents.rs`：11 种 AgentType（Supervisor/Requirement/VisualDirector/Story/Character/ImageGeneration/VideoGeneration/VoiceMusic/Editing/Review/Optimization）+ `supervisor_agent.rs`（Director → Agent → Critic 管线）。**当前仅单向编排，无动态子 Agent、无 Agent 间消息传递**。

### 7. 其他
- **沙箱**：未实现（工具直接在 Rust 进程执行，无隔离）。
- **备份恢复**：数据库 + 受管文件打包，schema 一致性校验 + 安全备份 + 服务热重载。
- **本地即梦代理**：`services/jimeng-*`（Node 常驻服务，端口 5100），已被 v1.2.8 便携版替换。

---

## 四、各 AI Agent 的遗留记忆与角色

项目由**多个 AI 编码 Agent 接力开发**，各自在隐藏目录留下状态/历史。这些目录大多被 `.gitignore` 忽略，属于「本地工作痕迹」而非仓库产物。

| Agent / 工具 | 遗留目录 | 角色与内容 |
| --- | --- | --- |
| **Mimosa**（安全/质量扫描 Hook） | `.mimosa/` | 代码变更**扫描与质量门禁** Agent。hook 在 PostToolUse 触发，产出 `reports/`（37 份 task-review）、`finding-ledger/`（34 事件）、`hook-state/`、`hook-status/`。**实际成效极低**：所有报告 `run_status=inconclusive`、`findings.total=0`，原因几乎全是 `scanner_failed: spawnSync D:\X\nodejs\node.exe ETIMEDOUT`（扫描器超时）——即门禁**从未真正跑通**。`hook-state` 有 57 个文件（大量 `.tmp`），说明 hook 被高频反复触发。另有嵌套副本于 `services/.mimosa`、`src-tauri/.mimosa`。 |
| **ZCode**（编码 Agent / IDE） | `.zcode/plans/` | 产出**长时迭代计划** `plan-sess_68b7d643….md`——「客户端 UI 全面重设计」（P0 死代码清场 → P5 无限打磨循环）。**未纳入 git**（`?? .zcode/`）。其会话 ID 与 Mimosa 的 `sess_68b7d643…` **一致**，说明二者共享同一宿主会话。 |
| **Claude**（Claude Code） | `.claude/` | 仅 `launch.json`（vite 端口 1430）与 `settings.local.json`（放行 `tasklist`、`curl localhost:5100/ping`）。 |
| **Codex**（OpenAI Codex CLI） | `.codex-run/` | 一次 `tauri dev` 的日志（端口 1421），暴露 Rust 编译告警（`supervisor_agent.rs:155` 不换行空格 `\u{a0}` 告警、中文注释乱码）。 |
| **Trae** | `.trae/EverOS/` | **完整克隆的开源参考项目 EverOS**（EverMind-AI，Python 本地优先 Markdown 记忆框架，LanceDB+SQLite），作为「Agent 记忆系统」的技术参考与可选依赖（`scripts/setup-everos.ps1` 安装）。 |
| **Git 历史备份** | `.git-history-backup/` | git 对象/引用备份（含 `COMMIT_EDITMSG_PHASE2A`）。 |
| **Serial MCP** | `.serial_mcp/` | MCP 跟踪目录（trace.jsonl 为空）。 |
| **Playwright** | `.playwright-cli/` | 一次控制台日志 + 页面快照（视觉验收痕迹）。 |

**关键结论**：所谓「多 Agent」实为**同一开发者用不同 AI 编码工具（Claude Code / Codex / ZCode / Trae / Mimosa）在同一仓库上接力**。它们之间**没有程序化协作协议**，协作靠**共享文件系统**：
- 通过 **git 提交**传递代码变更；
- 通过 **`docs/` 下的 handoff / progress / product 文档**传递设计与上下文（如 `docs/handoff/video-stream-prompt.md` 明确写给「新开 ZCode 对话」）；
- 通过 **文件领地划分**避免冲突（如 UI 重设计计划明令「只动 `src/`，不碰 `src-tauri/` 和 `services/`，因并行会话正在开发视频流 P0」）；
- **ZCode 计划** 与 **Mimosa hook** 共享会话 ID，说明 ZCode 是主编码 Agent，Mimosa 作为其质量 Hook 挂载。

---

## 五、演进脉络（git 时间线）

| 阶段 | 时间 | 关键事件 |
| --- | --- | --- |
| **奠基** | 2026-07 上旬 | Phase 0 脚手架、架构决策（ADR 0010–0013：原生 SQLite、headless 表格、迁移备份、受管存储健康）、Provider 基础设施决策 |
| **创意 OS 定调** | 2026-07-21 | `creative-os-tech-decisions.md` 终稿锁定：自研 Cognitive Router / Creative Workflow / Artifact System，全部不采用外部框架 |
| **Provider 重构** | 2026-07-23 | 三代 trait 收敛、Credential 类型化、ProviderFactory 规划 |
| **首个公开版** | 2026-09-10 | **v0.2.0** 发布：创意工坊、图片生成、无限画布、提示词库、模型/凭据、资产库、备份恢复、SQLite 迁移、类型化 IPC |
| **画布五阶段** | 2026-09 | 无限画布 P1–P5 优化（交互重设计、CanvasStore、命令模式、Agent 深度集成、视觉打磨、性能）；混合向量检索；EverOS 长期记忆接入 |
| **视频流 P0–P1** | 2026-09-19 | ffmpeg sidecar、三通道健康检查、单镜头视频闭环（时长透传、规划快速路径、视频播放渲染、导出链） |
| **UI 重设计** | 2026-09-27 | 死代码清场（漫剧分支/PrismaticBurst 等）、设计系统补完、AppShell 拆分、composer 合并、token 化清扫、会话时间线重设计 |
| **迭代 loop（风控攻坚）** | 2026-10-01~10-03 | 即梦 4013 风控排查 → 换代理 → v1.2.8 浏览器传输攻克风控 → **终局结论：赠送积分不覆盖视频（平台定价策略）**；回填、单步快速路径、画布可用性套件 |
| **当前** | 2026-10-03 后 | main 领先 origin **2 个提交**；工作区有未提交改动（`scripts/rebuild-generations-style.mjs`）与未跟踪的 `.zcode/` |

---

## 六、已确定的架构决策（不可轻易推翻）

1. **单进程、零外部运行时**：不引入 LangGraph/LiteLLM/AutoGen/CrewAI/Python sidecar（唯一例外：EverOS 作为可选记忆服务、即梦代理作为外部常驻服务）。
2. **Provider 通道唯一**：视频/图片生成一律经 `UnifiedProviderAdapter`；业务层禁止直连厂商 API；新增供应商 = 新增 adapter + 注册进 ProviderRegistry。
3. **凭据边界**：密钥只存 OS keychain，业务表只存引用键；`Credential ≠ Secret`，Provider 只见 `CredentialContext`。
4. **ffmpeg 落地**：Tauri `externalBin`（sidecar）随应用分发，探测链 sidecar → PATH → UI 提示。
5. **不新增 SQLite 迁移**（视频流 D6）：分镜复用 creative_plans，产物走 assets + project_output 导出。
6. **画布不 Fork，模式提取**：从 tldraw/Excalidraw/OpenBoard/drawdb/Plat 提取架构模式，在 vue-flow 上实现。
7. **UI 重设计只动前端**：不碰 Rust/服务，定向 `git add`（禁止 `add -A`，前有教训）。

---

## 七、未解决问题与风险

### 阻塞 / 高优先级
1. **即梦免费通道不能出视频**（唯一业务阻塞）：赠送积分只覆盖图片，视频权益池需充值/VIP。应用侧链路 100% 就绪，**用户在即梦充值或配置火山 Seedance 官方 API Key 后即可出片，无需改代码**。
2. **kling 官方通道未打通**：凭据表单仅支持单 Key，kling 需 AK/SK 双密钥（`CredentialContext::AccessSecret`），记为后续项。
3. **ffmpeg 真机验收未完成**：P0 验收时旧实例占用 1421 端口，启动日志未实证 `engine=ffmpeg`（需下次启动确认不再出现 mock composition）。

### 架构欠账
4. **沙箱完全缺失**：无进程/文件/网络隔离，工具执行无强制超时中断。
5. **子 Agent 系统半成品**：11 种 AgentType 已定义、Supervisor 可用，但无动态生成/销毁、无 Agent 间通信、无独立 ReAct 循环。
6. **Mimosa 质量门禁失效**：扫描器 `ETIMEDOUT` 导致所有报告 inconclusive、零 finding——安全/质量扫描实际**未提供保障**。
7. **嵌入凭据缺失**：现有 xiaomi（无 /embeddings）、dashscope（401）均不支持嵌入，向量检索自动降级为关键词模式。
8. **记忆注入预算分配**：长期记忆 / 画布 / 插件三段上下文争抢提示词空间，优先级策略未定。
9. **EverOS 配置界面缺失**：长期记忆目前仅能通过配置文件启用。

### 产品/工程待办
10. 生成图 URL 时效：即梦 CDN 签名 URL 会过期（已并存 dataUrl 缓解，待评估转存受管资产）。
11. Agent 工具选择校准（canvas 工具命中率待观察）。
12. 音乐/配音生成模式、画布多人协作、插件系统与受控 Host API、安装包签名与自动更新（路线图）。

### 工作区卫生风险
13. **`.zcode/` 未纳入 `.gitignore`**（当前显示为未跟踪），`.mimosa/` 有 57 个 hook-state 文件（含大量 `.tmp` 残留），`services/` 下堆积 4 个即梦代理目录（jimeng-api / jimeng-free-api-all / -src / -v128，旧版待清理）。

---

## 八、当前状态一句话总结

项目已从「AI 生成工具」演进为**架构清晰的本地创作 OS 雏形**：**v0.2.0 已发布**，Agent 对话 + 图片生成 + 无限画布 + 记忆 RAG + 资产库 + 备份恢复**全部可用**；**视频流链路（单镜头 + 分镜成片）代码 100% 就绪，唯一阻塞是即梦账户的视频积分（平台定价策略，属用户计费决策）**；UI 已完成一轮系统化重设计。技术债集中在**沙箱缺失、子 Agent 半成品、Mimosa 门禁失效、嵌入凭据缺失**。多 Agent 协作靠**共享文件系统 + git + docs 交接文档**，无程序化协议。
