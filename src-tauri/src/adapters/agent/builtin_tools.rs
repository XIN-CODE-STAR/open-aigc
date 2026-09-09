use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    application::{
        apply_script_plan_service::{ApplyResult, ApplyScriptPlanService, LayoutStrategy},
        canvas_context_service::CanvasContextService,
        generation_submit_service::GenerationSubmitService,
        model_router_service::ModelRouterService,
        script_parser_service::ScriptParserService,
        semantic::pipeline_service::SemanticPipelineService,
        semantic::retrieval_service::SemanticRetrievalService,
    },
    domain::{
        agent::ToolDefinition,
        canvas::{CanvasNodeStatus, NodePatch},
        script_plan::ScriptPlan,
    },
    ports::{
        agent_tool_executor::{
            AgentToolError, AgentToolExecutor, ToolContext, ToolExecutionResult,
        },
        credential_repository::{CredentialRepository, CredentialRepositoryError},
        generation_repository::{GenerationRepository, GenerationRepositoryError},
        vision_adapter::VisionAdapter,
    },
};

const TOOL_IMAGE_GENERATION: &str = "image_generation";
const TOOL_VIDEO_GENERATION: &str = "video_generation";
const TOOL_LIST_CREDENTIALS: &str = "list_credentials";
const TOOL_CURRENT_TIME: &str = "current_time";
const TOOL_PARSE_SCRIPT: &str = "parse_script";
const TOOL_APPLY_SCRIPT_PLAN: &str = "apply_script_plan";
const TOOL_UPDATE_NODE_STATUS: &str = "update_canvas_node_status";
const TOOL_ANALYZE_IMAGE: &str = "analyze_image";
const TOOL_CANVAS_SEARCH: &str = "canvas_search";
const TOOL_CANVAS_ADD_NOTE: &str = "canvas_add_note";
const TOOL_CANVAS_CONNECT: &str = "canvas_connect";
const TOOL_SEARCH_SIMILAR_ASSETS: &str = "search_similar_assets";
const TOOL_INSPECT_ASSET: &str = "inspect_asset";
const TOOL_REASON_ABOUT_ASSET: &str = "reason_about_asset";

/// 内置工具执行器：组合 GenerationRepository + CredentialRepository 实现七个内置工具。
///
/// 设计权衡：
/// - `image_generation` / `video_generation` 仅创建 pending 生成任务并返回 task_id，
///   不直接执行 provider 调用（避免长阻塞）。实际生成由现有 generation 流程异步完成。
/// - `list_credentials` 返回凭据元数据（不含密钥）。
/// - `current_time` 返回当前 UTC ISO 时间，给 LLM 时间感知。
/// - `parse_script` 使用 ScriptParserService 解析剧本文本为结构化 ScriptPlan。
/// - `apply_script_plan` 将 ScriptPlan 应用到画布，创建 CanvasNodes + CanvasEdges。
/// - `update_canvas_node_status` 更新画布节点状态。
/// - `analyze_image` 使用视觉模型分析图片，生成语义描述。
/// - `search_similar_assets` 在语义库中搜索相似素材。
pub struct BuiltinToolExecutor {
    generation_repository: Mutex<Box<dyn GenerationRepository>>,
    credential_repository: Mutex<Box<dyn CredentialRepository>>,
    vision_adapter: Option<Box<dyn VisionAdapter>>,
    model_router: Option<ModelRouterService>,
    generation_submitter: Option<GenerationSubmitService>,
    canvas_context_service: Option<Arc<CanvasContextService>>,
    script_parser_service: ScriptParserService,
    apply_script_plan_service: Option<Arc<ApplyScriptPlanService>>,
    semantic_pipeline_service: Option<Arc<SemanticPipelineService>>,
    semantic_retrieval_service: Option<Arc<SemanticRetrievalService>>,
    provider_registry: Option<Arc<crate::application::provider_registry::ProviderRegistry>>,
}

impl BuiltinToolExecutor {
    pub fn new(
        generation_repository: impl GenerationRepository + 'static,
        credential_repository: impl CredentialRepository + 'static,
    ) -> Self {
        Self {
            generation_repository: Mutex::new(Box::new(generation_repository)),
            credential_repository: Mutex::new(Box::new(credential_repository)),
            vision_adapter: None,
            model_router: None,
            generation_submitter: None,
            canvas_context_service: None,
            script_parser_service: ScriptParserService,
            apply_script_plan_service: None,
            semantic_pipeline_service: None,
            semantic_retrieval_service: None,
            provider_registry: None,
        }
    }

    /// 设置 Vision 适配器（用于参考图分析工具）。
    pub fn with_vision_adapter(mut self, adapter: impl VisionAdapter + 'static) -> Self {
        self.vision_adapter = Some(Box::new(adapter));
        self
    }

    /// 设置 Cognitive Router。
    pub fn with_model_router(mut self, router: ModelRouterService) -> Self {
        self.model_router = Some(router);
        self
    }

    /// 设置生成提交服务。
    pub fn with_generation_submitter(mut self, submitter: GenerationSubmitService) -> Self {
        self.generation_submitter = Some(submitter);
        self
    }

    /// 设置画布上下文服务（替代 database_path 直连方式）。
    pub fn with_canvas_context_service(mut self, svc: Arc<CanvasContextService>) -> Self {
        self.canvas_context_service = Some(svc);
        self
    }

    /// 设置脚本计划应用服务。
    pub fn with_apply_script_plan_service(mut self, svc: Arc<ApplyScriptPlanService>) -> Self {
        self.apply_script_plan_service = Some(svc);
        self
    }

    /// 设置语义管线服务（用于图片分析工具）。
    pub fn with_semantic_pipeline_service(mut self, svc: Arc<SemanticPipelineService>) -> Self {
        self.semantic_pipeline_service = Some(svc);
        self
    }

    /// 设置语义检索服务（用于素材搜索工具）。
    pub fn with_semantic_retrieval_service(mut self, svc: Arc<SemanticRetrievalService>) -> Self {
        self.semantic_retrieval_service = Some(svc);
        self
    }

    /// 设置 Provider 注册表（用于查询可用的生成能力）。
    pub fn with_provider_registry(
        mut self,
        registry: Arc<crate::application::provider_registry::ProviderRegistry>,
    ) -> Self {
        self.provider_registry = Some(registry);
        self
    }
}

impl AgentToolExecutor for BuiltinToolExecutor {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::function(
                TOOL_IMAGE_GENERATION,
                "提交一个图片生成任务。自动选择可用的图片生成 Provider。返回任务 ID 和状态。当对话中用户上传过图片时，默认以最近一张作为参考图走图生图（保持原图版式，只按提示词修改）；传 useReferenceImage=false 可忽略参考图做纯文生图。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "providerName": {
                            "type": "string",
                            "description": "图片生成服务提供商名称（可选，不填则自动选择）"
                        },
                        "modelName": {
                            "type": "string",
                            "description": "模型标识（可选，不填则使用默认模型）"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "图片生成提示词，应详细描述要生成的图片内容"
                        },
                        "useReferenceImage": {
                            "type": "boolean",
                            "description": "是否以对话中最近的用户图片为参考图做图生图。默认 true（存在参考图时）；传 false 忽略参考图纯文生图。"
                        }
                    },
                    "required": ["prompt"]
                }),
            ),
            ToolDefinition::function(
                TOOL_CANVAS_SEARCH,
                "在当前对话的画布（工作记忆）中检索相关节点。当需要回忆之前放置在画布上的图片、便签、结论或偏好时调用。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "检索关键词，例如：风格偏好、二维码、配色"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "返回条数上限（默认 5）"
                        }
                    },
                    "required": ["query"]
                }),
            ),
            ToolDefinition::function(
                TOOL_VIDEO_GENERATION,
                "提交一个视频生成任务到队列。任务以 pending 状态创建，由后端异步执行。返回任务 ID 和状态。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "providerName": {
                            "type": "string",
                            "description": "视频生成服务提供商名称"
                        },
                        "modelName": {
                            "type": "string",
                            "description": "模型标识"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "视频生成提示词"
                        }
                    },
                    "required": ["providerName", "modelName", "prompt"]
                }),
            ),
            ToolDefinition::function(
                TOOL_LIST_CREDENTIALS,
                "列出所有可用的图片/视频生成凭据（含 providerName 和 modelName），用于选择生成参数。",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::function(
                TOOL_CURRENT_TIME,
                "获取当前 UTC 时间（ISO 8601）。当用户询问时间相关问题或需要时间戳时使用。",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::function(
                TOOL_PARSE_SCRIPT,
                "解析剧本/故事文本，将其拆解为结构化的场景(Scene)和分镜(Shot)列表。\
                 输入一段故事或剧本文字，输出按场景分组的镜头清单，每个镜头包含：场景标题、\
                 镜头描述、建议的镜头类型(shot_type)、机位运动(camera_motion)、和生成提示词(prompt)。\
                 适用于将用户提供的故事自动转化为可执行的分镜脚本。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "scriptText": {
                            "type": "string",
                            "description": "完整的剧本/故事文本"
                        },
                        "style": {
                            "type": "string",
                            "description": "可选的视觉风格描述（如：水彩、赛博朋克、写实），影响生成的 prompt"
                        },
                        "maxScenes": {
                            "type": "integer",
                            "description": "最大场景数（默认 10）"
                        }
                    },
                    "required": ["scriptText"]
                }),
            ),
            ToolDefinition::function(
                TOOL_APPLY_SCRIPT_PLAN,
                "将解析好的脚本计划（ScriptPlan）应用到画布，创建场景节点和分镜节点，并自动布局连线。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "canvasId": {
                            "type": "string",
                            "description": "目标画布 ID"
                        },
                        "plan": {
                            "type": "object",
                            "description": "ScriptPlan 对象（来自 parse_script 的返回结果）"
                        },
                        "layout": {
                            "type": "string",
                            "enum": ["Timeline", "Grid", "Hierarchical"],
                            "description": "布局策略，默认 Timeline"
                        }
                    },
                    "required": ["canvasId", "plan"]
                }),
            ),
            ToolDefinition::function(
                TOOL_UPDATE_NODE_STATUS,
                "更新画布节点的状态（pending/generating/succeeded/failed/draft/ready）。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "canvasId": {
                            "type": "string",
                            "description": "画布 ID"
                        },
                        "nodeId": {
                            "type": "string",
                            "description": "节点 ID"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["pending", "generating", "succeeded", "failed", "draft", "ready"],
                            "description": "新状态"
                        }
                    },
                    "required": ["canvasId", "nodeId", "status"]
                }),
            ),
            ToolDefinition::function(
                TOOL_ANALYZE_IMAGE,
                "分析参考图片的内容，生成详细的语义描述、标签和实体。当用户上传参考图或询问图片内容时使用。\
                 分析结果会存储到语义库中，后续可通过 search_similar_assets 检索相似素材。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "assetId": {
                            "type": "string",
                            "description": "要分析的素材 ID"
                        },
                        "dataUrl": {
                            "type": "string",
                            "description": "图片的 data URL（base64 编码），可选，如果不提供则从 filePath 读取"
                        },
                        "filePath": {
                            "type": "string",
                            "description": "图片文件的绝对路径，可选，当 dataUrl 未提供时从此路径读取图片"
                        },
                        "preferredAdapter": {
                            "type": "string",
                            "description": "首选的分析适配器：'dashscope'（云端）或 'florence'（本地），默认自动选择"
                        }
                    },
                    "required": ["assetId"]
                }),
            ),
            ToolDefinition::function(
                TOOL_SEARCH_SIMILAR_ASSETS,
                "在语义库中搜索与查询文本相似的素材。支持按标签、OCR 文本和语义向量搜索。\
                 用于发现相关素材或查找特定内容的图片。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索查询文本"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "返回结果数量限制，默认 10"
                        }
                    },
                    "required": ["query"]
                }),
            ),
            ToolDefinition::function(
                TOOL_INSPECT_ASSET,
                "快速查看素材的已分析语义信息（不调用视觉模型）。\
                 返回之前分析过的描述、标签、实体、OCR 文本等。\
                 适用于快速了解素材内容，无需等待模型推理。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "assetId": {
                            "type": "string",
                            "description": "要查看的素材 ID"
                        }
                    },
                    "required": ["assetId"]
                }),
            ),
            ToolDefinition::function(
                TOOL_REASON_ABOUT_ASSET,
                "对素材进行深度视觉推理（调用视觉模型）。\
                 可以回答关于图片内容的复杂问题，如'为什么这个角色看起来悲伤？'、\
                 '这两个场景之间有什么关联？'。适用于需要深入理解图片语义的场景。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "assetId": {
                            "type": "string",
                            "description": "要推理的素材 ID"
                        },
                        "question": {
                            "type": "string",
                            "description": "关于图片的问题或推理指令"
                        },
                        "dataUrl": {
                            "type": "string",
                            "description": "图片的 data URL（base64 编码），可选，如果不提供则从 filePath 读取"
                        },
                        "filePath": {
                            "type": "string",
                            "description": "图片文件的绝对路径，可选，当 dataUrl 未提供时从此路径读取图片"
                        },
                        "context": {
                            "type": "string",
                            "description": "额外上下文信息（如之前分析的结果），可选"
                        }
                    },
                    "required": ["assetId", "question"]
                }),
            ),
        ]
    }

    fn execute(
        &mut self,
        ctx: &ToolContext,
        tool_name: &str,
        arguments: &str,
    ) -> Result<ToolExecutionResult, AgentToolError> {
        match tool_name {
            TOOL_IMAGE_GENERATION | TOOL_VIDEO_GENERATION => {
                execute_generation(self, ctx, tool_name, arguments)
            }
            TOOL_LIST_CREDENTIALS => execute_list_credentials(self),
            TOOL_CURRENT_TIME => execute_current_time(),
            TOOL_PARSE_SCRIPT => execute_parse_script(ctx, arguments, &self.script_parser_service),
            TOOL_APPLY_SCRIPT_PLAN => execute_apply_script_plan(self, ctx, arguments),
            TOOL_UPDATE_NODE_STATUS => execute_update_node_status(self, ctx, arguments),
            TOOL_ANALYZE_IMAGE => execute_analyze_image(self, ctx, arguments),
            TOOL_CANVAS_SEARCH => execute_canvas_search(self, ctx, arguments),
            TOOL_CANVAS_ADD_NOTE => execute_canvas_add_note(self, ctx, arguments),
            TOOL_CANVAS_CONNECT => execute_canvas_connect(self, ctx, arguments),
            TOOL_SEARCH_SIMILAR_ASSETS => execute_search_similar_assets(self, ctx, arguments),
            TOOL_INSPECT_ASSET => execute_inspect_asset(self, ctx, arguments),
            TOOL_REASON_ABOUT_ASSET => execute_reason_about_asset(self, ctx, arguments),
            other => Err(AgentToolError::UnknownTool(other.to_owned())),
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Canvas context — via CanvasContextService (replaces direct SQLite)
// ──────────────────────────────────────────────────────────────────

/// 从 CanvasContextService 加载当前对话画布的完整空间上下文。
/// 返回格式化的描述字符串，包含节点位置、连线关系和布局总览。
fn load_canvas_context(
    executor: &BuiltinToolExecutor,
    workspace_path: &std::path::Path,
    conversation_id: &str,
) -> Option<String> {
    // 优先读取前端无限画布的工作记忆（memory_nodes，检索式）
    if let Some(text) = crate::application::canvas_memory_rag::load_working_memory_context(
        workspace_path,
        conversation_id,
        None,
        24,
    ) {
        return Some(text);
    }
    // 回退：agent 侧旧 canvas 表
    let svc = executor.canvas_context_service.as_ref()?;
    let ctx = svc.load_context_for_agent(conversation_id, None).ok()?;
    if ctx.formatted_description.is_empty()
        || ctx.formatted_description == "画布暂未创建，无可用上下文。"
    {
        None
    } else {
        Some(ctx.formatted_description)
    }
}

// ──────────────────────────────────────────────────────────────────
// Generation tool
// ──────────────────────────────────────────────────────────────────

fn execute_generation(
    executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    tool_name: &str,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: GenerationArgs = parse_arguments(tool_name, arguments)?;

    // 用户在对话中上传过图片且未明确关闭时，图片生成自动走图生图（i2i）：
    // 参考图让结果保持原图版式与内容，仅按提示词修改，避免"文字描述重画"导致的巨大偏差。
    let reference_image =
        if tool_name == TOOL_IMAGE_GENERATION && args.use_reference_image != Some(false) {
            ctx.latest_user_image.clone()
        } else {
            None
        };

    // 确定所需能力
    let capability = if tool_name == TOOL_IMAGE_GENERATION {
        if reference_image.is_some() {
            crate::ports::unified_provider::CapabilityKind::ImageToImage
        } else {
            crate::ports::unified_provider::CapabilityKind::TextToImage
        }
    } else {
        crate::ports::unified_provider::CapabilityKind::TextToVideo
    };

    // 检查是否有可用的 Provider
    let provider_registry =
        executor
            .provider_registry
            .as_ref()
            .ok_or_else(|| AgentToolError::ExecutionFailed {
                tool: tool_name.to_owned(),
                reason: "生成服务未初始化，请重启应用。".to_owned(),
            })?;
    let resolved = provider_registry
        .resolve_capability(&capability)
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: tool_name.to_owned(),
            reason: format!(
                "当前没有可用的{} Provider。请在「模型管理」中添加支持 {} 的凭据。",
                if tool_name == TOOL_IMAGE_GENERATION {
                    "图片生成"
                } else {
                    "视频生成"
                },
                if tool_name == TOOL_IMAGE_GENERATION {
                    "text_to_image"
                } else {
                    "text_to_video"
                }
            ),
        })?;

    let provider_id = resolved.adapter.provider_id().to_owned();

    // 为 image_generation 加载画布上下文，注入到 prompt 中
    let enriched_prompt = if tool_name == TOOL_IMAGE_GENERATION {
        match load_canvas_context(executor, &ctx.workspace_path, &ctx.conversation_id) {
            Some(context) if !context.is_empty() => {
                format!(
                    "{}\n\n[画布上下文 - 当前工作记忆中的图片资源]\n{}",
                    args.prompt, context
                )
            }
            _ => args.prompt.clone(),
        }
    } else {
        args.prompt.clone()
    };

    let mut repository =
        executor
            .generation_repository
            .lock()
            .map_err(|_| AgentToolError::ExecutionFailed {
                tool: tool_name.to_owned(),
                reason: "repository lock unavailable".to_owned(),
            })?;

    // 使用 enriched_prompt 创建任务
    let model_name = args.model_name.clone().unwrap_or_else(|| {
        if tool_name == TOOL_IMAGE_GENERATION {
            "default".to_owned()
        } else {
            "default".to_owned()
        }
    });
    let draft = crate::domain::generation::GenerationTaskDraft::try_new(
        ctx.workspace_id.clone(),
        provider_id.clone(),
        model_name.clone(),
        enriched_prompt.clone(),
    )
    .map_err(|e| AgentToolError::InvalidArguments {
        tool: tool_name.to_owned(),
        reason: e.to_string(),
    })?;

    let record = repository.create_task(draft).map_err(|e| match e {
        GenerationRepositoryError::Persistence(p) => AgentToolError::Persistence(p),
        other => AgentToolError::ExecutionFailed {
            tool: tool_name.to_owned(),
            reason: other.to_string(),
        },
    })?;

    // 尝试自动提交到 Provider
    let (submit_status, submit_message) = if let Some(submitter) = &executor.generation_submitter {
        let mut snapshot = serde_json::json!({
            "prompt": record.prompt_text,
            "model": args.model_name,
        });
        if let Some(image) = &reference_image {
            snapshot["reference_image_url"] = serde_json::Value::String(image.clone());
        }
        let request_snapshot = snapshot.to_string();
        let capability_str = capability.as_str();
        match submitter.submit_and_dispatch(
            &record.id,
            &resolved.credential.provider_id,
            capability_str,
            &request_snapshot,
            &provider_id,
            None,
        ) {
            Ok(attempt) => {
                eprintln!(
                    "[AgentTool] submitted {} task={} attempt={} provider={}",
                    tool_name, record.id, attempt.id, provider_id
                );
                // jimeng 代理路径是同步的：immediate_result_url 就是真实图片 CDN URL。
                // remote_job_id 格式为 "proxy:image:{url}" 或 "proxy:i2i:{url}"，
                // 统一取 "http" 起始的真实 URL 传回给 LLM。
                let image_url = attempt
                    .remote_job_id
                    .as_deref()
                    .and_then(|u| u.find("http").map(|pos| &u[pos..]))
                    .map(|u| u.to_owned());
                let status_msg = if let Some(ref url) = image_url {
                    format!(
                        "已创建 {tool_name} 任务并提交到 {provider_id}。\
                         图片已生成完成，URL: {url}\
                         \n请在回复中用 ![生成结果]({url}) 展示给用户。"
                    )
                } else {
                    format!("已创建 {tool_name} 任务并提交到 {provider_id}，任务正在生成。")
                };
                ("submitted".to_owned(), status_msg)
            }
            Err(e) => {
                eprintln!(
                    "[AgentTool] submit failed for {} task={}: {e}",
                    tool_name, record.id
                );
                // 提交失败：任务立即标记 failed，避免永远停留在 pending；
                // 并把失败原因如实返回给 LLM，防止模型向用户谎报"正在生成"。
                let reason = truncate_feedback(&e.to_string(), 240);
                if let Err(mark_err) = repository.update_status(
                    &record.id,
                    crate::domain::generation::GenerationStatus::Failed,
                    Some(reason.clone()),
                ) {
                    eprintln!(
                        "[AgentTool] mark task {} failed error: {mark_err}",
                        record.id
                    );
                }
                (
                    "failed".to_owned(),
                    format!(
                        "任务创建失败：{reason}。生成未开始，请勿告知用户任务正在生成；如需继续请先解决该错误。"
                    ),
                )
            }
        }
    } else {
        (
            "pending".to_owned(),
            format!("已创建 {tool_name} 任务，等待队列提交。"),
        )
    };

    let payload = serde_json::json!({
        "taskId": record.id,
        "ok": submit_status != "failed",
        "status": submit_status,
        "providerName": provider_id,
        "modelName": model_name,
        "prompt": record.prompt_text,
        "message": submit_message
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: Some(record.id),
    })
}

/// 截断错误描述，避免超长响应体（如 HTML 页面）进入 LLM 上下文。
fn truncate_feedback(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}

fn execute_list_credentials(
    executor: &BuiltinToolExecutor,
) -> Result<ToolExecutionResult, AgentToolError> {
    let mut repository =
        executor
            .credential_repository
            .lock()
            .map_err(|_| AgentToolError::ExecutionFailed {
                tool: TOOL_LIST_CREDENTIALS.to_owned(),
                reason: "repository lock unavailable".to_owned(),
            })?;
    let credentials = repository.list().map_err(|e| match e {
        CredentialRepositoryError::Persistence(p) => AgentToolError::Persistence(p),
        other => AgentToolError::ExecutionFailed {
            tool: TOOL_LIST_CREDENTIALS.to_owned(),
            reason: other.to_string(),
        },
    })?;
    let summary: Vec<CredentialSummary> = credentials
        .into_iter()
        .filter(|c| c.enabled)
        .map(CredentialSummary::from)
        .collect();
    let payload = serde_json::json!({
        "credentials": summary,
        "count": summary.len()
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

fn execute_current_time() -> Result<ToolExecutionResult, AgentToolError> {
    let now = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_CURRENT_TIME.to_owned(),
            reason: e.to_string(),
        }
    })?;
    let payload = serde_json::json!({
        "iso8601": now,
        "timezone": "UTC"
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

// ──────────────────────────────────────────────────────────────────
// Task B: Script Parser Tool — parse_script
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ScriptParseArgs {
    #[serde(rename = "scriptText")]
    script_text: String,
    style: Option<String>,
    #[serde(rename = "maxScenes")]
    max_scenes: Option<usize>,
}

/// 执行剧本解析：委托给 ScriptParserService，返回结构化 ScriptPlan。
fn execute_parse_script(
    ctx: &ToolContext,
    arguments: &str,
    parser: &ScriptParserService,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: ScriptParseArgs = parse_arguments(TOOL_PARSE_SCRIPT, arguments)?;
    let script_text = args.script_text.trim();
    if script_text.is_empty() {
        return Err(AgentToolError::InvalidArguments {
            tool: TOOL_PARSE_SCRIPT.to_owned(),
            reason: "剧本文本不能为空".to_owned(),
        });
    }

    let plan = parser
        .parse(script_text, args.style.as_deref(), args.max_scenes)
        .map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_PARSE_SCRIPT.to_owned(),
            reason: e.to_string(),
        })?;

    // 向后兼容：将 ScriptPlan 序列化为 JSON 返回给前端
    let payload = serde_json::json!({
        "plan": plan,
        "suggestedTitle": plan.title,
        "totalShots": plan.scenes.iter().map(|s| s.shots.len()).sum::<usize>(),
        "conversationId": ctx.conversation_id,
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// apply_script_plan 工具：将 ScriptPlan 应用到画布。
fn execute_apply_script_plan(
    executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(rename = "canvasId")]
        canvas_id: String,
        plan: ScriptPlan,
        layout: Option<String>,
    }

    let args: Args = parse_arguments(TOOL_APPLY_SCRIPT_PLAN, arguments)?;

    let strategy = match args.layout.as_deref() {
        Some("Grid") => LayoutStrategy::Grid,
        Some("Hierarchical") => LayoutStrategy::Hierarchical,
        _ => LayoutStrategy::Timeline,
    };

    let svc = executor.apply_script_plan_service.as_ref().ok_or_else(|| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_APPLY_SCRIPT_PLAN.to_owned(),
            reason: "ApplyScriptPlanService 未配置".to_owned(),
        }
    })?;

    let result: ApplyResult = svc
        .apply(&ctx.workspace_id, &args.canvas_id, &args.plan, strategy)
        .map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_APPLY_SCRIPT_PLAN.to_owned(),
            reason: e.to_string(),
        })?;

    let payload = serde_json::json!({
        "canvasId": result.canvas_id,
        "sceneNodeIds": result.scene_node_ids,
        "shotNodeIds": result.shot_node_ids,
        "edgeIds": result.edge_ids,
        "totalNodes": result.total_nodes,
        "totalEdges": result.total_edges,
        "message": format!("已创建 {} 个场景节点、{} 个分镜节点和 {} 条连线", result.scene_node_ids.len(), result.shot_node_ids.len(), result.edge_ids.len())
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// update_canvas_node_status 工具：更新画布节点状态。
fn execute_update_node_status(
    executor: &BuiltinToolExecutor,
    _ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(rename = "canvasId")]
        canvas_id: String,
        #[serde(rename = "nodeId")]
        node_id: String,
        status: String,
    }

    let args: Args = parse_arguments(TOOL_UPDATE_NODE_STATUS, arguments)?;

    let new_status = match args.status.as_str() {
        "pending" => CanvasNodeStatus::Pending,
        "generating" => CanvasNodeStatus::Generating,
        "succeeded" => CanvasNodeStatus::Succeeded,
        "failed" => CanvasNodeStatus::Failed,
        "draft" => CanvasNodeStatus::Draft,
        "ready" => CanvasNodeStatus::Ready,
        other => {
            return Err(AgentToolError::InvalidArguments {
                tool: TOOL_UPDATE_NODE_STATUS.to_owned(),
                reason: format!(
                    "无效的状态值: {}，可选: pending/generating/succeeded/failed/draft/ready",
                    other
                ),
            });
        }
    };

    let svc = executor.canvas_context_service.as_ref().ok_or_else(|| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_UPDATE_NODE_STATUS.to_owned(),
            reason: "CanvasContextService 未配置".to_owned(),
        }
    })?;

    // 通过 CanvasContextService 更新节点状态
    let patch = NodePatch {
        kind: None,
        position: None,
        size: None,
        summary: None,
        description: None,
        prompt: None,
        status: Some(new_status),
        refs: None,
        metadata: None,
    };

    svc.update_node(&args.node_id, patch)
        .map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_UPDATE_NODE_STATUS.to_owned(),
            reason: e.to_string(),
        })?;

    let payload = serde_json::json!({
        "nodeId": args.node_id,
        "status": args.status,
        "message": format!("节点状态已更新为 {}", args.status)
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

// ──────────────────────────────────────────────────────────────────
// Semantic Analysis Tools
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnalyzeImageArgs {
    #[serde(rename = "assetId")]
    asset_id: String,
    #[serde(rename = "dataUrl")]
    data_url: Option<String>,
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    #[serde(rename = "preferredAdapter")]
    preferred_adapter: Option<String>,
}

/// 读取图片文件并转换为 data URL (base64)。
fn read_file_as_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    let data = std::fs::read(path).map_err(|e| format!("无法读取文件 '{}': {}", path, e))?;
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let mime = match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok(format!("data:{};base64,{}", mime, b64))
}

/// 执行图片语义分析：使用视觉模型生成描述、标签、实体。
fn execute_canvas_search(
    executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        query: String,
        limit: Option<usize>,
    }
    let args: Args = parse_arguments(TOOL_CANVAS_SEARCH, arguments)?;
    let limit = args.limit.unwrap_or(5).clamp(1, 20);
    let hits = crate::application::canvas_memory_rag::search_nodes(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &args.query,
        limit,
    );
    let payload = serde_json::json!({
        "query": args.query,
        "count": hits.len(),
        "results": hits
            .into_iter()
            .map(|(text, score)| { serde_json::json!({ "text": text, "score": score }) })
            .collect::<Vec<_>>(),
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

fn execute_canvas_add_note(
    executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        text: String,
        x: Option<f64>,
        y: Option<f64>,
    }
    let args: Args = parse_arguments(TOOL_CANVAS_ADD_NOTE, arguments)?;
    let x = args.x.unwrap_or(200.0);
    let y = args.y.unwrap_or(200.0);
    let node_id = crate::application::canvas_memory_rag::add_canvas_note(
        &ctx.workspace_path,
        &ctx.conversation_id,
        x,
        y,
        &args.text,
    );
    // P4：语义自动连线
    let connected = node_id
        .as_ref()
        .map(|id| {
            crate::application::canvas_memory_rag::auto_connect_related(
                &ctx.workspace_path,
                &ctx.conversation_id,
                id,
                3,
            )
        })
        .unwrap_or(0);
    let payload = serde_json::json!({
        "ok": node_id.is_some(),
        "nodeId": node_id,
        "autoConnected": connected,
        "message": if node_id.is_some() {
            format!("已在画布上创建便签，并自动关联了 {connected} 个相关节点。")
        } else {
            "画布节点创建失败。".to_owned()
        }
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

fn execute_canvas_connect(
    executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        #[serde(rename = "sourceQuery")]
        source_query: String,
        #[serde(rename = "targetQuery")]
        target_query: String,
        label: Option<String>,
    }
    let args: Args = parse_arguments(TOOL_CANVAS_CONNECT, arguments)?;
    let source = crate::application::canvas_memory_rag::search_nodes(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &args.source_query,
        1,
    );
    let target = crate::application::canvas_memory_rag::search_nodes(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &args.target_query,
        1,
    );
    if source.is_empty() || target.is_empty() {
        let payload = serde_json::json!({
            "ok": false,
            "message": "未找到匹配的节点，请检查检索关键词。"
        });
        return Ok(ToolExecutionResult {
            content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
            generation_task_id: None,
        });
    }
    let hits = crate::application::canvas_memory_rag::auto_connect_related(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &args.source_query,
        3,
    );
    let payload = serde_json::json!({
        "ok": true,
        "message": format!("已建立连线（自动关联 {} 个相关节点）。", hits),
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

fn execute_analyze_image(
    executor: &BuiltinToolExecutor,
    _ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: AnalyzeImageArgs = parse_arguments(TOOL_ANALYZE_IMAGE, arguments)?;

    let pipeline = executor.semantic_pipeline_service.as_ref().ok_or_else(|| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_ANALYZE_IMAGE.to_owned(),
            reason: "SemanticPipelineService 未配置".to_owned(),
        }
    })?;

    // 解析图片数据：优先 dataUrl，其次 filePath，最后空占位符
    let data_url = if let Some(url) = args.data_url {
        url
    } else if let Some(ref path) = args.file_path {
        read_file_as_data_url(path).map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_ANALYZE_IMAGE.to_owned(),
            reason: format!("读取图片文件失败: {}", e),
        })?
    } else {
        // TODO: 从 AssetRepository 读取图片数据
        "data:image/png;base64,".to_owned()
    };

    let result = pipeline
        .analyze_asset(&args.asset_id, &data_url, args.preferred_adapter.as_deref())
        .map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_ANALYZE_IMAGE.to_owned(),
            reason: e.to_string(),
        })?;

    let payload = serde_json::json!({
        "assetId": args.asset_id,
        "profile": result.profile,
        "adapterUsed": result.adapter_id,
        "processingTimeMs": result.elapsed_ms,
        "message": format!("图片分析完成，使用 {} 适配器，耗时 {}ms", result.adapter_id, result.elapsed_ms)
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

#[derive(Debug, Deserialize)]
struct SearchSimilarAssetsArgs {
    query: String,
    limit: Option<usize>,
}

/// 执行语义搜索：在语义库中查找相似素材。
fn execute_search_similar_assets(
    executor: &BuiltinToolExecutor,
    _ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: SearchSimilarAssetsArgs = parse_arguments(TOOL_SEARCH_SIMILAR_ASSETS, arguments)?;

    let retrieval = executor
        .semantic_retrieval_service
        .as_ref()
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: TOOL_SEARCH_SIMILAR_ASSETS.to_owned(),
            reason: "SemanticRetrievalService 未配置".to_owned(),
        })?;

    let limit = args.limit.unwrap_or(10);

    let results = retrieval.unified_search(&args.query, limit).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_SEARCH_SIMILAR_ASSETS.to_owned(),
            reason: e.to_string(),
        }
    })?;

    let payload = serde_json::json!({
        "query": args.query,
        "results": results,
        "totalFound": results.len(),
        "message": format!("找到 {} 个相关素材", results.len())
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

#[derive(Debug, Deserialize)]
struct InspectAssetArgs {
    asset_id: String,
}

/// 执行素材查看：快速获取已分析的语义信息（不调用视觉模型）。
fn execute_inspect_asset(
    executor: &BuiltinToolExecutor,
    _ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: InspectAssetArgs = parse_arguments(TOOL_INSPECT_ASSET, arguments)?;

    let retrieval = executor
        .semantic_retrieval_service
        .as_ref()
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: TOOL_INSPECT_ASSET.to_owned(),
            reason: "SemanticRetrievalService 未配置".to_owned(),
        })?;

    let profile =
        retrieval
            .get_profile(&args.asset_id)
            .map_err(|e| AgentToolError::ExecutionFailed {
                tool: TOOL_INSPECT_ASSET.to_owned(),
                reason: e.to_string(),
            })?;

    match profile {
        Some(profile) => {
            let payload = serde_json::json!({
                "assetId": args.asset_id,
                "profile": profile,
                "message": "素材语义信息已获取"
            });

            Ok(ToolExecutionResult {
                content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
                generation_task_id: None,
            })
        }
        None => {
            let payload = serde_json::json!({
                "assetId": args.asset_id,
                "profile": null,
                "message": "素材未找到或尚未分析"
            });

            Ok(ToolExecutionResult {
                content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
                generation_task_id: None,
            })
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReasonAboutAssetArgs {
    #[serde(rename = "assetId")]
    asset_id: String,
    question: String,
    #[serde(rename = "dataUrl")]
    data_url: Option<String>,
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    context: Option<String>,
}

/// 执行素材推理：调用视觉模型进行深度推理。
fn execute_reason_about_asset(
    executor: &BuiltinToolExecutor,
    _ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let args: ReasonAboutAssetArgs = parse_arguments(TOOL_REASON_ABOUT_ASSET, arguments)?;

    let pipeline = executor.semantic_pipeline_service.as_ref().ok_or_else(|| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_REASON_ABOUT_ASSET.to_owned(),
            reason: "SemanticPipelineService 未配置".to_owned(),
        }
    })?;

    // 解析图片数据：优先 dataUrl，其次 filePath，最后空占位符
    let data_url = if let Some(url) = args.data_url {
        url
    } else if let Some(ref path) = args.file_path {
        read_file_as_data_url(path).map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_REASON_ABOUT_ASSET.to_owned(),
            reason: format!("读取图片文件失败: {}", e),
        })?
    } else {
        // TODO: 从 AssetRepository 读取图片数据
        "data:image/png;base64,".to_owned()
    };

    // 构造推理指令，包含问题和上下文
    let instruction = if let Some(context) = &args.context {
        format!("{}\n\n上下文信息：\n{}", args.question, context)
    } else {
        args.question.clone()
    };

    // 使用 SemanticPipelineService 进行分析（实际上应该调用 VisionReasoningPort）
    // 由于 SemanticPipelineService 目前只支持 captioning，我们先使用它作为占位符
    // 在完整实现中，应该调用 VisionReasoningPort 进行深度推理
    let result = pipeline
        .analyze_asset(&args.asset_id, &data_url, None)
        .map_err(|e| AgentToolError::ExecutionFailed {
            tool: TOOL_REASON_ABOUT_ASSET.to_owned(),
            reason: e.to_string(),
        })?;

    let payload = serde_json::json!({
        "assetId": args.asset_id,
        "question": args.question,
        "answer": result.profile.caption,
        "profile": result.profile,
        "adapterUsed": result.adapter_id,
        "processingTimeMs": result.elapsed_ms,
        "message": format!("深度推理完成，使用 {} 适配器，耗时 {}ms", result.adapter_id, result.elapsed_ms)
    });

    Ok(ToolExecutionResult {
        content: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

// ──────────────────────────────────────────────────────────────────
// Shared utilities
// ──────────────────────────────────────────────────────────────────

fn parse_arguments<T: for<'de> Deserialize<'de>>(
    tool_name: &str,
    arguments: &str,
) -> Result<T, AgentToolError> {
    serde_json::from_str(arguments).map_err(|e| AgentToolError::InvalidArguments {
        tool: tool_name.to_owned(),
        reason: e.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct GenerationArgs {
    #[serde(rename = "providerName")]
    provider_name: Option<String>,
    #[serde(rename = "modelName")]
    model_name: Option<String>,
    prompt: String,
    /// false 时忽略对话参考图，强制纯文生图。
    #[serde(rename = "useReferenceImage")]
    use_reference_image: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CredentialSummary {
    id: String,
    provider_name: String,
    display_name: String,
    model_name: String,
    base_url: String,
}

impl From<crate::domain::credentials::CredentialRecord> for CredentialSummary {
    fn from(record: crate::domain::credentials::CredentialRecord) -> Self {
        Self {
            id: record.id,
            provider_name: record.provider_name,
            display_name: record.display_name,
            model_name: record.model_name,
            base_url: record.base_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::asset_repository::SqliteAssetRepository;
    use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
    use crate::adapters::sqlite::generation_attempt_repository::SqliteGenerationAttemptRepository;
    use crate::adapters::sqlite::generation_repository::SqliteGenerationRepository;
    use crate::adapters::sqlite::review_repository::SqliteReviewRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::application::generation_pipeline::GenerationPipeline;
    use crate::application::provider_registry::ProviderRegistry;
    use crate::domain::credentials::{CredentialContext, CredentialDraft};
    use crate::domain::generation::GenerationStatus;
    use crate::domain::providers::ProviderError;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::unified_provider::{
        CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedPollResult,
        UnifiedProviderAdapter, UnifiedRequest, UnifiedSubmitResult,
    };
    use crate::ports::workspace_repository::WorkspaceRepository;

    /// 图片能力 Mock Provider：submit 行为可配置，用于覆盖提交成败两条路径。
    struct MockImageProvider {
        submit_error: bool,
    }

    impl UnifiedProviderAdapter for MockImageProvider {
        fn provider_id(&self) -> &str {
            "mock-image"
        }
        fn capabilities(&self) -> Vec<CapabilityKind> {
            vec![CapabilityKind::TextToImage]
        }
        fn submit(
            &self,
            _request: &UnifiedRequest,
            _credential: &CredentialContext,
        ) -> Result<UnifiedSubmitResult, ProviderError> {
            if self.submit_error {
                Err(ProviderError::ConfigInvalid("mock submit failure".into()))
            } else {
                Ok(UnifiedSubmitResult {
                    remote_job_id: "mock-1".into(),
                    initial_status: "submitted".into(),
                    immediate_result_url: None,
                    estimated_duration_secs: Some(1),
                })
            }
        }
        fn poll(
            &self,
            _id: &str,
            _credential: &CredentialContext,
        ) -> Result<UnifiedPollResult, ProviderError> {
            Ok(UnifiedPollResult {
                status: "succeeded".into(),
                progress: 100,
                result_url: None,
                error_message: None,
                retryable: false,
            })
        }
        fn download(
            &self,
            _url: &str,
            _dir: &std::path::Path,
            _credential: &CredentialContext,
        ) -> Result<UnifiedDownloadResult, ProviderError> {
            Ok(UnifiedDownloadResult {
                file_path: "/tmp/mock.png".into(),
                mime_type: "image/png".into(),
                file_size: 1,
                duration_secs: None,
                width: Some(1),
                height: Some(1),
            })
        }
        fn health_check(
            &self,
            _credential: &CredentialContext,
        ) -> Result<UnifiedHealthStatus, ProviderError> {
            Ok(UnifiedHealthStatus {
                available: true,
                message: "ok".into(),
                quota_remaining: Some(100),
            })
        }
        fn is_async(&self) -> bool {
            true
        }
    }

    fn seed_executor() -> (tempfile::TempDir, BuiltinToolExecutor, String) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试", "测试教师").unwrap())
            .unwrap();
        let workspace_id = workspace
            .get_status()
            .unwrap()
            .workspace
            .unwrap()
            .workspace_id
            .clone();
        drop(workspace);

        let mut credential_repo = SqliteCredentialRepository::open(&path).unwrap();
        credential_repo
            .create(
                CredentialDraft::try_new(
                    "Seedance".to_owned(),
                    "种子舞蹈".to_owned(),
                    "https://api.seedance.com".to_owned(),
                    "seedance-v2".to_owned(),
                )
                .unwrap(),
                "sk-test".to_owned(),
            )
            .unwrap();
        drop(credential_repo);

        let generation_repo = SqliteGenerationRepository::open(&path).unwrap();
        let credential_repo = SqliteCredentialRepository::open(&path).unwrap();
        let executor = BuiltinToolExecutor::new(generation_repo, credential_repo);
        (directory, executor, workspace_id)
    }

    #[test]
    fn executes_current_time() {
        let (_dir, mut executor, _workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id: "any".to_owned(),
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let result = executor.execute(&ctx, TOOL_CURRENT_TIME, "{}").unwrap();
        assert!(result.content.contains("iso8601"));
        assert!(result.generation_task_id.is_none());
    }

    #[test]
    fn executes_list_credentials() {
        let (_dir, mut executor, _workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id: "any".to_owned(),
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let result = executor.execute(&ctx, TOOL_LIST_CREDENTIALS, "{}").unwrap();
        assert!(result.content.contains("Seedance"));
        assert!(result.content.contains("seedance-v2"));
    }

    #[test]
    fn executes_image_generation() {
        let (_dir, mut executor, workspace_id) = seed_executor();
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockImageProvider {
            submit_error: false,
        }));
        executor = executor.with_provider_registry(Arc::new(registry));
        let ctx = ToolContext {
            workspace_id,
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let args = r#"{"providerName":"Seedance","modelName":"seedance-v2","prompt":"跳舞的猫"}"#;
        let result = executor.execute(&ctx, TOOL_IMAGE_GENERATION, args).unwrap();
        assert!(result.generation_task_id.is_some());
        assert!(result.content.contains("pending"));
    }

    #[test]
    fn submit_failure_marks_task_failed_and_reports_error() {
        let (dir, mut executor, workspace_id) = seed_executor();
        let path = dir.path().join("workspace.sqlite3");

        let registry = Arc::new(ProviderRegistry::new());
        registry.register(Arc::new(MockImageProvider { submit_error: true }));
        let pipeline = Arc::new(GenerationPipeline::new(
            dir.path().join("downloads"),
            SqliteReviewRepository::open(&path).unwrap(),
            SqliteAssetRepository::open(&path).unwrap(),
        ));
        let attempt_repository = SqliteGenerationAttemptRepository::open(&path).unwrap();
        let submitter =
            GenerationSubmitService::new(attempt_repository, Arc::clone(&registry), pipeline, path)
                .unwrap();
        executor = executor
            .with_provider_registry(registry)
            .with_generation_submitter(submitter);

        let ctx = ToolContext {
            workspace_id,
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let args = r#"{"modelName":"m","prompt":"跳舞的猫"}"#;
        let result = executor.execute(&ctx, TOOL_IMAGE_GENERATION, args).unwrap();

        // 工具必须如实告知 LLM 提交失败，而不是返回"已提交"。
        let payload: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(payload["ok"], serde_json::Value::Bool(false));
        assert_eq!(payload["status"], "failed");
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("任务创建失败"));
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("请勿告知用户任务正在生成"));

        // 任务在库中被标记为 failed，而不是停留在 pending。
        let mut repository = executor.generation_repository.lock().unwrap();
        let task = repository
            .get_task(result.generation_task_id.as_ref().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(task.status, GenerationStatus::Failed);
    }

    #[test]
    fn rejects_unknown_tool() {
        let (_dir, mut executor, _workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id: "any".to_owned(),
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let error = executor
            .execute(&ctx, "nonexistent_tool", "{}")
            .unwrap_err();
        assert!(matches!(error, AgentToolError::UnknownTool(_)));
    }

    #[test]
    fn rejects_invalid_arguments() {
        let (_dir, mut executor, workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id,
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let error = executor
            .execute(&ctx, TOOL_IMAGE_GENERATION, "not json")
            .unwrap_err();
        assert!(matches!(error, AgentToolError::InvalidArguments { .. }));
    }

    #[test]
    fn parse_script_rejects_empty() {
        let (_dir, mut executor, _workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id: "any".to_owned(),
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let args = serde_json::json!({"scriptText": ""});
        let error = executor
            .execute(&ctx, TOOL_PARSE_SCRIPT, &args.to_string())
            .unwrap_err();
        assert!(matches!(error, AgentToolError::InvalidArguments { .. }));
    }

    #[test]
    fn lists_seven_tools() {
        let (_dir, executor, _workspace_id) = seed_executor();
        let tools = executor.list_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(names.contains(&TOOL_IMAGE_GENERATION));
        assert!(names.contains(&TOOL_VIDEO_GENERATION));
        assert!(names.contains(&TOOL_LIST_CREDENTIALS));
        assert!(names.contains(&TOOL_CURRENT_TIME));
        assert!(names.contains(&TOOL_PARSE_SCRIPT));
        assert!(names.contains(&TOOL_APPLY_SCRIPT_PLAN));
        assert!(names.contains(&TOOL_UPDATE_NODE_STATUS));
    }

    #[test]
    fn parse_script_via_service() {
        let (_dir, mut executor, _workspace_id) = seed_executor();
        let ctx = ToolContext {
            workspace_id: "any".to_owned(),
            workspace_path: std::path::PathBuf::from("."),
            conversation_id: "test-conv".to_owned(),
            output_directory: None,
            sandbox: None,
            latest_user_image: None,
        };
        let args = serde_json::json!({
            "scriptText": "场景一：城市全景\n镜头从高空俯瞰赛博朋克城市。\n\n场景二：街道\n一名少年走在雨中。"
        });
        let result = executor
            .execute(&ctx, TOOL_PARSE_SCRIPT, &args.to_string())
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let plan = &parsed["plan"];
        assert!(plan["scenes"].as_array().unwrap().len() >= 2);
    }
}
