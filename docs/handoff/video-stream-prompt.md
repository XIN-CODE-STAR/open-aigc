# OPEN AIGC 视频流开发 · 交接 Prompt

> 使用方法：新开 ZCode 对话，把分隔线以下的全部内容作为第一条消息发送。
> 本文与代码同步维护于 `docs/handoff/video-stream-prompt.md`。

---

你是本仓库（OPEN AIGC，Tauri v2 + Vue3 + Rust）的资深工程师。本次任务：**开发 OPEN AIGC 的视频流业务——从创意到成片的完整视频生产链路**。开工前先读完本文与"现状盘点"列出的文件，再按阶段实施。不要重构与视频无关的代码。

## 一、业务目标

用户输入一个创意（或剧本/参考图），系统产出可播放、可导出的成片：

1. **单镜头闭环**：文本/图片 → 视频生成任务 → 轮询 → 落库为资产 → 工作目录导出 → 应用内播放。
2. **分镜成片**：Agent 规划多镜头（CreativePlan）→ 并行生成各镜头 → ffmpeg 合成统一转码拼接（可选 BGM）→ 最终作品事件 → 对话内与资产库可播放。
3. **过程可视化**：shot 级进度、失败原因、重试，全程事件推送到前端。

非目标（本期不做）：云端渲染、数字人驱动、实时推流、视频编辑时间轴（拖拽剪辑）。

## 二、现状盘点（先读这些文件）

### 后端 Rust（src-tauri/src/）

| 文件 | 现状 |
|---|---|
| `ports/unified_provider.rs` | `UnifiedProviderAdapter` trait：`submit/poll/download/health`，能力用 `CapabilityKind` 声明。**视频必须走这里** |
| `adapters/providers/kling_video.rs` | 快影适配器（AK/SK→JWT，文生视频/图生视频），完整可用 |
| `adapters/providers/seedance_video.rs` | Seedance 适配器，完整可用 |
| `adapters/providers/grok_generation.rs` | grok 生成（含视频能力分支） |
| 即梦视频 | 走本地 jimeng-api Node 代理（常驻服务，remote_job_id 前缀 `proxy:video:`；模型 `jimeng-video-seedance-2.0` 等）。代理代码在 `services/jimeng-api/` |
| `application/creative_runtime_service.rs` | 四阶段流水线编排器：Submit → Poll → Import → Compose，带 `RuntimeEventBus` |
| `application/creative_poll_worker.rs` | 轮询 + 下载（`poll_once/wait_and_download`） |
| `application/creative_plan_builder.rs` | CreativeBrief → CreativePlan（shots 含 duration_secs） |
| `domain/creative_plan.rs` | CreativePlan/shots 领域模型 |
| `application/composition_facade.rs` | **`CompositionFacade` trait 已有真实实现 `FfmpegCompositionFacade`**（归一化转码 + concat）+ Mock 回退 |
| `application/composite_skill.rs` | 合成技能，`detect_ffmpeg()` 探测 PATH |
| `application/generation_pipeline.rs` | 单任务生成 + `project_output::export_generated_file` 导出到用户工作目录 |
| `application/generation_queue_service.rs` / `poll_worker.rs` | 单任务队列与轮询（图片已在用） |
| `application/planning_engine.rs` | Agent 规划。已有图片任务约束（`has_image` → 只建图片步骤），**视频无对应约束** |
| `application/project_folder.rs` | `.openaigc/` 项目文件夹（project.json + AIGC.md 项目记忆，随工作目录落地） |

### 前端（src/）

- `modules/generations/components/PromptComposer.vue`：视频模式已存在（creationMode === "video"，含时长/分辨率/帧率参数，模型选择器带「生成来源」标识，展示全部 AI 账号 + API Key 模型）。
- `modules/generations/components/AgentStepTimeline.vue`：`renderContent()` 已渲染 markdown 图片（`![alt](url)` → `<img>`），**不支持视频**。
- `modules/assets/`：资产库，图片缩略图用 `convertFileSrc`（asset.localhost 已在 CSP 白名单）。
- `app/stores/projectDirectory.ts`：项目输出目录 + 最近目录 + `.openaigc/AIGC.md` 项目记忆（经 `systemPromptOverride` 注入 Agent）。

### 已知缺口（= 本任务要填的坑）

1. 运行环境无 ffmpeg，启动日志显示 `CreativeRuntime: ffmpeg not found, using mock composition` —— 合成是假的。
2. 视频模式没有镜像图片的规划约束：Agent 收到"生成视频"类请求时规划行为不受控。
3. `AgentStepTimeline`/资产库不渲染视频（mp4 无预览、无播放）。
4. 视频任务的 shot 级进度/失败重试未打通到前端 UI。

## 三、技术决策（已定，按此执行；有异议先在回复中说明再动手）

- **D1 生成通道**：视频生成一律经 `UnifiedProviderAdapter`（kling/seedance 原生；即梦经 jimeng-api 代理）。业务层禁止直连厂商 API。新增供应商 = 新增 adapter + 注册进 `ProviderRegistry`。
- **D2 ffmpeg 落地**：用 Tauri `externalBin`（sidecar）随应用分发静态 ffmpeg（Windows x64 GPL build）。启动时探测顺序：sidecar → PATH（现有 `detect_ffmpeg()`）→ UI 提示"合成不可用"。`CompositionFacade` trait 不动，只让探测命中 sidecar。理由：合成（转码/拼接/封面帧/混音）是硬依赖；纯 Rust 编解码 crate 不成熟；Remotion 需 Node 运行时过重。
- **D3 编排复用**：分镜走 creative 四阶段流水线 + `RuntimeEventBus`（已有 RuntimeEvent/ShotStatus 域模型）；单镜头（无分镜意图）走 generation_queue + PollWorker 既有路径，不另起炉灶。视频与图片的差异只在 adapter 与合成阶段。
- **D4 幂等与续传**：轮询沿用 remote_job_id 持久化（重启后 PollWorker 续轮）；合成输出以输入哈希命名，已存在则跳过——重试不重复计费。
- **D5 前端播放**：`<video>` + `convertFileSrc`（CSP 已放行 asset.localhost，与图片同机制）。`renderContent` 扩展：markdown 视频语法渲染为 `<video controls>`。分镜进度第一版用**分镜卡片列表**（缩略图+状态+时长），不引入重型时间轴库。
- **D6 数据模型**：不新增 SQLite 迁移。分镜已在 creative_plans；产物走 assets（assetKind=video）+ `project_output` 导出到 `.openaigc` 同级工作目录的 `generated/`。封面帧（P3）存 assets 缩略图位。
- **D7 规划约束**：镜像图片的做法——`is_creative_task` 判视频意图时在 planning prompt 注入"[当前任务限制]只创建视频生成步骤"级别的约束；单镜头请求 1 步，禁止擅自加镜头。
- **D8 安全边界**：API Key 仍只走凭据库（keyring/加密 vault），adapter 无状态持 `CredentialContext`；`.openaigc/AIGC.md` 与技能一样经 `systemPromptOverride` 注入，不落库。

## 四、实施阶段（每阶段独立可验收，完成后跑门禁并提交）

### P0 环境与通道打通（先行）
1. ffmpeg sidecar：`src-tauri/tauri.conf.json` externalBin 配置 + 下载脚本（放 `scripts/`，不入库二进制）+ 启动探测接入 `detect_ffmpeg` 回退链。
2. 写一个集成测试：给 `FfmpegCompositionFacade` 两段样例视频（测试夹具生成），断言合成产物存在且时长≈两段之和。
3. 三通道健康检查脚本化：kling / seedance / jimeng 代理各跑一次 health + 提交一条 1s 测试任务（有凭据时）。
验收：启动日志不再出现 mock composition；合成测试绿。

### P1 单镜头视频闭环
1. 视频意图的规划约束（D7）+ 单镜头快速路径：非分镜请求直接建 1 步视频任务。
2. 资产库视频卡片：播放悬停预览（`<video>` 静音 hover）、时长/分辨率展示。
3. `AgentStepTimeline.renderContent` 支持视频渲染；生成完成事件注入 `![视频](url)`。
4. 导出链路验证：生成完成 → `generated/<对话标题>/` 出现 mp4。
验收：输入"生成一段 5 秒的海浪视频"→ 对话内可播放 → 资产库可见 → 工作目录有文件。

### P2 分镜成片
1. 视频版 CreativePlan：分镜意图（"做一支 30 秒的产品宣传片，3 个镜头"）→ shots（文案/时长/参考图）。
2. 并行提交全部 shot（流水线 Phase 1 已支持），PollWorker 收齐后进入 Compose。
3. 合成参数决策：统一 1080p/30fps H.264 + AAC；BGM 可选（用户工作目录 `.openaigc/bgm/` 有文件则混音）。
4. 成片事件：`RuntimeEvent::Compose` 完成 → 对话内注入成片播放卡片 + 资产库入库。
验收：3 镜头脚本一句话出片，产物可在应用内播放并导出。

### P3 体验完善
1. shot 级进度推送 UI（分镜卡片状态机：待生成/生成中/已就绪/失败，失败可单镜头重试）。
2. ffmpeg 封面帧提取（`-frames:v 1`）作为视频缩略图。
3. 压力与超时：单 shot 轮询上限 15 分钟，超时标记失败并给出可读原因。

## 五、工程约定（本仓库门禁，违反即返工）

- 提交前全部通过：`npx pnpm check`（format+lint+vitest+build）与 `node scripts/rust-test.mjs`（Rust 测试需外部 manifest，勿直接 cargo test）；clippy `-D warnings`。
- 新 Tauri 命令要注册三处：`lib.rs` 的 use + handler 列表、`build.rs` app_manifest。
- 注释用中文、说清"为什么"；测试放同文件 `#[cfg(test)]`；禁止 `unwrap()` 进业务路径。
- 环境注意：Windows + Git Bash；pnpm 用 `npx pnpm`；tauri dev 启动前确保无残留 `aigc-studio.exe`（文件锁会导致 EBUSY/拒绝访问）；vite watch 已排除 `.mimosa/`。
- 即梦代理已常驻（开机自启），勿重复起服务；积分计费真实发生，测试任务用最低参数。

## 六、第一个动作

1. 读"现状盘点"列出的文件，输出一份 ≤30 行的现状确认（含你发现的与我描述不符之处）。
2. 给出 P0 的实施清单，等我确认后动工。

## 七、实施进度（随阶段同步维护）

### P0 环境与通道打通 — 已完成（2026-09-19）

- **ffmpeg sidecar**：`scripts/fetch-ffmpeg.mjs`（默认 BtbN GitHub GPL 构建，curl 优先以遵循本机代理，支持 `--zip/--url` 换源），产物部署在 `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`（已 gitignore）；`tauri.conf.json` 配置 `bundle.externalBin`；`detect_ffmpeg()` 回退链 = 可执行文件同目录 sidecar（存在 + ≥1MB 门禁）→ PATH。tauri dev/build 会自动把 sidecar 拷到 target 目录并随包分发。
- **合成集成测试**：`composition_facade.rs` 两个真机测试（双视频拼接时长≈和、图片归一化时长≈声明值），时长断言直接解析 mp4 容器 moov/mvhd；缺 ffmpeg 自动跳过。
- **三通道健康检查**：`scripts/channel-health.mjs`（默认只做免费检查，`--submit` 才提交最低档真实生成）；kling/seedance 经 `src/tests/channel_health.rs` 的 `#[ignore]` 冒烟测试复用 adapter（`node scripts/rust-test.mjs -- --ignored channel_health`），凭据只走环境变量。
- **门禁修复**：`sequential_executor` 两个编排测试钉住 Mock 合成（`with_facade_mock_composite`）——sidecar 落地后 cargo test 会把 target/debug 注入测试 PATH 命中 ffmpeg，真实 CompositeSkill 过滤 mock 产物会让测试环境依赖化。
- **验收状态**：`pnpm check` / `rust-test.mjs`（518 通过）/ `clippy -D warnings` 全绿；启动日志实证因运行中的旧实例占用 1421 端口未完成，下次应用启动确认应出现 `engine=ffmpeg` 且不再出现 mock composition。

### P1 单镜头视频闭环 — 代码完成（2026-09-19，待真机验收）

- **CSP**：csp/devCsp 补 `media-src`（asset.localhost + blob），`<video>` 播放不再被 default-src 拦截。
- **时长透传**：`video_generation` 工具新增 `durationSeconds`，随请求快照落入 `UnifiedRequest.parameters`；kling adapter 的 duration 兼容字符串/数字两种来源（修复数字被静默回落 5s 的隐患）。
- **单镜头快速路径（D7）**：`planning_engine` 新增 `wants_video`/`is_storyboard_request`/`parse_video_duration_secs`（钳制 1-10s，数字后必须带单位防止"5个镜头"误判）。非分镜视频请求直接建 1 步 VideoGeneration 计划（跳过 Director/LLM 规划）；分镜请求维持 Director→Planner；通用规划降级路径注入视频约束（只建视频步骤、1 步、传 durationSeconds）。
- **前端渲染**：AgentStepTimeline 的 markdown 视频扩展名（mp4/webm/mov/m4v）渲染 `<video controls>`；结果区提取 proxy:video:/localAsset 视频 URL；资产库视频卡片静音悬停预览 + loadedmetadata 读时长/分辨率（不动库表）。
- **导出链**：`generation_pipeline` Stage 3b 按类型无差别导出，无需改动，video 尝试自动落 `generated/<对话标题>/`。
- **门禁**：`pnpm check` / `rust-test.mjs`（524 通过，含 4 个新规划测试）/ `clippy -D warnings` 全绿。
- **待真机验收**：应用内输入"生成一段 5 秒的海浪视频"→ 对话内可播放 → 资产库可见 → 工作目录有 mp4（需要已配置的视频凭据，计费真实发生）。

### 迭代 loop（10/1-10/2，4013 风控攻坚 + 现场问题修复）

现场测试暴露并修复的问题（均已提交）：
- **代理整体替换**：旧 iptag/jimeng-api（D:\jimeng-api 部署，v1.6.3，已归档）被其开机自启拉回抢占 5100，替换为 **zhizinan1997/jimeng-free-api-all v1.2.7 便携版**（API 兼容免改连接器，sha256 校验，端口 5100）；自启 vbs 已同步切换（旧项改名 .disabled 可逆）。新代理运行时目录 `services/jimeng-free-api-all/` 已 gitignore。
- **完成后回填**：invocation 的 result_json 停留在提交时占位导致对话不显示产物——生成管线新增回填阶段（按 generation_task_id 定位调用记录，写回 attempt/localAsset + 推送 ToolInvocationUpdated）。
- **时间线提取补 imageUrl**：同步通道的提交载荷自带最终 URL，前端提取器此前只认异步回填字段。
- **图片单步快速路径 + 提交失败短路**：镜像 D7 消除单图多步刷屏；提交被拒后终止剩余步骤（已有成功产出时保持部分成功语义）。
- **video 工具 schema 对齐多模型编排设计**：providerName/modelName 改可选（编排者无需知道生成模型名）。
- **健康检查 jm_ Key 兼容**：jm_ 托管 Key 不再被误标 need_login。

**4013 风控排查结论（迭代 loop 模型族全面判别，10/2）**：
同一会话、同一端点 `/mweb/v1/aigc_draft/generate`：图片（high_aes_general_v50）两次 ret=0 成功；视频各模型族实测——**seedance-2.0 通道（dreamina_seedance_40_pro）稳定 4013**（跨两套代理、4 天复现）；**seedance-2.5 / 2.0-mini / wan-3.0 / minimax-h3 / happyhorse-1.1 全部通过风控**到达积分校验；3.0 标准版已下线（2061）。据此连接器默认视频模型已切 **seedance-2.5**（ff7ebe3）。

**当前唯一阻塞：即梦账户视频积分余额**。账户 totalCredit=66（每日赠送），但视频各模型族降级到最低档仍 -2009 积分不足（视频权益池与图片赠送积分可能不通用）。解决途径：即梦充值积分 / 开通 VIP / 等待赠送刷新后由 loop 自动重试。视频一旦生成成功，展示/入库/导出链路已全部就绪。
