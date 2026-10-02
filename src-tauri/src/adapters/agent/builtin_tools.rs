use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    application::{
        apply_script_plan_service::{ApplyResult, ApplyScriptPlanService, LayoutStrategy},
        canvas_context_service::CanvasContextService,
        generation_submit_service::GenerationSubmitService,
        model_router_service::ModelRouterService,
        provider_registry::ProviderRegistry,
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
const TOOL_CANVAS_ADD_IMAGE: &str = "canvas_add_image";
const TOOL_CANVAS_UPDATE_NODE: &str = "canvas_update_node";
const TOOL_CANVAS_AUTO_LAYOUT: &str = "canvas_auto_layout";
const TOOL_CANVAS_EXPORT: &str = "canvas_export";
const TOOL_LIST_SKILLS: &str = "list_skills";
const TOOL_USE_SKILL: &str = "use_skill";
const TOOL_MCP_LIST_TOOLS: &str = "mcp_list_tools";
const TOOL_MCP_CALL: &str = "mcp_call";
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
                "提交一个图片生成任务，生成一张真实的 AI 图像。仅在用户明确要求生成图片/图像/插画时使用——记录文字信息、结论或待办请改用 canvas_add_note。
当对话中用户上传过图片时，默认以最近一张作为参考图走图生图（保持原图版式，只按提示词修改）；传 useReferenceImage=false 可忽略参考图做纯文生图。返回任务 ID 与状态，出图后自动挂到工作记忆画布。",
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
                TOOL_CANVAS_ADD_NOTE,
                "在当前对话的画布（工作记忆）上创建一条纯文字便签节点，用于记录信息、结论、待办或偏好。                 这是记录文字信息的唯一正确方式——不要为此调用图片生成。返回节点 ID 并自动关联画布上的相关节点。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "便签文字内容"
                        },
                        "x": { "type": "number", "description": "画布 x 坐标（可选，默认自动排列）" },
                        "y": { "type": "number", "description": "画布 y 坐标（可选）" }
                    },
                    "required": ["text"]
                }),
            ),
            ToolDefinition::function(
                TOOL_CANVAS_CONNECT,
                "在画布的两个节点之间建立语义连线。用 canvas_search 找到源与目标节点后调用。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sourceQuery": {
                            "type": "string",
                            "description": "源节点检索关键词"
                        },
                        "targetQuery": {
                            "type": "string",
                            "description": "目标节点检索关键词"
                        },
                        "label": {
                            "type": "string",
                            "description": "连线标签（可选，如：风格参考、内容补充）"
                        }
                    },
                    "required": ["sourceQuery", "targetQuery"]
                }),
            ),
            ToolDefinition::function(
                TOOL_VIDEO_GENERATION,
                "提交一个视频生成任务到队列。任务以 pending 状态创建，由后端异步执行。返回任务 ID 和状态。\
                 即梦通道会同步返回视频 URL，此时请在回复中用 ![生成结果](视频URL) 展示给用户。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "providerName": {
                            "type": "string",
                            "description": "视频生成服务提供商名称（可选，不填则自动选择可用的视频生成 Provider，无需先查询凭据）"
                        },
                        "modelName": {
                            "type": "string",
                            "description": "模型标识（可选，不填则使用默认视频模型；不要填自己的对话模型名）"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "视频生成提示词"
                        },
                        "durationSeconds": {
                            "type": "number",
                            "description": "视频时长（秒，可选）。当前供应商单镜头支持 5 或 10 秒，默认 5。从用户消息中的时长要求解析（如\"5秒\"）。"
                        }
                    },
                    "required": ["prompt"]
                }),
            ),
            ToolDefinition::function(
                TOOL_LIST_CREDENTIALS,
                "列出所有可用的图片/视频生成凭据（含 providerName 和 modelName），以及已注册的生成 \
                 Provider 及其能力（generationProviders，含即梦等 session 类资源账号）。\
                 凭据为空但 generationProviders 非空时，生成工具依然可用，直接调用即可。",
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
            ToolDefinition::function(
                TOOL_CANVAS_ADD_IMAGE,
                "在当前对话的画布（工作记忆）上创建一个图片节点。url 填可公开访问的图片地址（http/https 或 data URL）。                 仅用于放置已有图片；需要生成新图像请用 image_generation，记录文字请用 canvas_add_note。                 创建后会自动与画布上语义相关的节点建立连线。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "图片地址（http/https 或 data URL）"
                        },
                        "description": {
                            "type": "string",
                            "description": "图片说明（可选，会作为节点摘要展示）"
                        },
                        "x": { "type": "number", "description": "画布 x 坐标（可选，默认自动排列）" },
                        "y": { "type": "number", "description": "画布 y 坐标（可选）" }
                    },
                    "required": ["url"]
                }),
            ),
            ToolDefinition::function(
                TOOL_CANVAS_UPDATE_NODE,
                "更新画布上已有节点的内容（便签文本、节点颜色）。先用 canvas_search 检索得到 nodeId，再调用本工具修改。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nodeId": {
                            "type": "string",
                            "description": "要更新的节点 ID（来自 canvas_search 的结果）"
                        },
                        "text": {
                            "type": "string",
                            "description": "新的节点文本（可选）"
                        },
                        "color": {
                            "type": "string",
                            "description": "节点颜色十六进制值（可选，如 #fbbf24）"
                        }
                    },
                    "required": ["nodeId"]
                }),
            ),
            ToolDefinition::function(
                TOOL_CANVAS_AUTO_LAYOUT,
                "自动整理当前对话的画布：按节点类型分组排列成整齐的网格。画布杂乱时调用。返回移动的节点数。",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::function(
                TOOL_CANVAS_EXPORT,
                "导出当前对话画布的完整 JSON（全部节点、连线与内容，图片二进制省略）。                 需要整体分析画布内容、汇总用户想法或备份画布时调用。",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::function(
                TOOL_LIST_SKILLS,
                "列出用户已安装的技能（含名称与用途说明）。用户要求使用某个技能、或任务看起来与某个技能描述相关时，先调用本工具查看可用技能。",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::function(
                TOOL_USE_SKILL,
                "加载一个技能的完整指令并按其执行任务。先用 list_skills 查看可用技能，确定要用的技能后调用本工具获取指令正文。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "技能名称（list_skills 返回的 name）"
                        }
                    },
                    "required": ["name"]
                }),
            ),
            ToolDefinition::function(
                TOOL_MCP_LIST_TOOLS,
                "查看 MCP 服务器及其工具。不传 server 时列出所有已启用的 MCP 服务器；传 server 名时列出该服务器提供的工具清单。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server": {
                            "type": "string",
                            "description": "MCP 服务器名称（可选，不填则列出所有服务器）"
                        }
                    }
                }),
            ),
            ToolDefinition::function(
                TOOL_MCP_CALL,
                "调用 MCP 服务器提供的工具。先用 mcp_list_tools 查看可用工具，再调用本工具执行。",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server": {
                            "type": "string",
                            "description": "MCP 服务器名称"
                        },
                        "tool": {
                            "type": "string",
                            "description": "工具名称（mcp_list_tools 返回的 name）"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "工具参数对象（按工具的入参说明构造，可选）"
                        }
                    },
                    "required": ["server", "tool"]
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
            TOOL_CANVAS_ADD_IMAGE => execute_canvas_add_image(self, ctx, arguments),
            TOOL_CANVAS_UPDATE_NODE => execute_canvas_update_node(self, ctx, arguments),
            TOOL_CANVAS_AUTO_LAYOUT => execute_canvas_auto_layout(self, ctx, arguments),
            TOOL_CANVAS_EXPORT => execute_canvas_export(self, ctx, arguments),
            TOOL_LIST_SKILLS => execute_list_skills(self, ctx, arguments),
            TOOL_USE_SKILL => execute_use_skill(self, ctx, arguments),
            TOOL_MCP_LIST_TOOLS => execute_mcp_list_tools(self, ctx, arguments),
            TOOL_MCP_CALL => execute_mcp_call(self, ctx, arguments),
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
    query: Option<&str>,
) -> Option<String> {
    // 有提示词时优先按词项检索（含连线邻接加权，让与相关节点连线的上下文优先），
    // 检索无命中再回退全量概览
    if let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) {
        if let Some(text) = crate::application::canvas_memory_rag::load_working_memory_context(
            workspace_path,
            conversation_id,
            Some(query),
            12,
        ) {
            return Some(text);
        }
    }
    // 读取前端无限画布的工作记忆（memory_nodes）
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
    //（按生成提示词检索：命中的节点 + 其连线邻居优先，作为生成资源的上下文）
    let enriched_prompt = if tool_name == TOOL_IMAGE_GENERATION {
        // 参考图定向上下文优先：参考图对应的画布节点及其连线邻居是本次生成的直接依据
        let reference_context = reference_image.as_deref().and_then(|reference| {
            crate::application::canvas_memory_rag::load_reference_context(
                &ctx.workspace_path,
                &ctx.conversation_id,
                reference,
            )
        });
        let general_context = load_canvas_context(
            executor,
            &ctx.workspace_path,
            &ctx.conversation_id,
            Some(&args.prompt),
        );
        let canvas_context = match (reference_context, general_context) {
            (Some(reference), Some(general)) => Some(format!(
                "{reference}

{general}"
            )),
            (Some(reference), None) => Some(reference),
            (None, Some(general)) => Some(general),
            (None, None) => None,
        };
        match canvas_context {
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
    let model_name = args
        .model_name
        .clone()
        .unwrap_or_else(|| "default".to_owned());
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

    let mut canvas_pending_node_id: Option<String> = None;
    let record = repository.create_task(draft).map_err(|e| match e {
        GenerationRepositoryError::Persistence(p) => AgentToolError::Persistence(p),
        other => AgentToolError::ExecutionFailed {
            tool: tool_name.to_owned(),
            reason: other.to_string(),
        },
    })?;

    // 图片生成任务立即在画布挂 pending 占位节点（同步/异步共用）：
    // 记录 taskId 供完成侧定位补全，参考图能匹配到画布节点时立即连「风格参考」，
    // 生成过程中参考图与任务即可视化关联。
    if tool_name == TOOL_IMAGE_GENERATION {
        match crate::application::canvas_memory_rag::attach_generation_task(
            &ctx.workspace_path,
            &ctx.conversation_id,
            &record.id,
            &args.prompt,
            reference_image.as_deref(),
        ) {
            Some(node_id) => {
                canvas_pending_node_id = Some(node_id);
            }
            None => eprintln!(
                "[AgentTool] canvas pending node attach failed: {}",
                record.id
            ),
        }
    }

    // 尝试自动提交到 Provider
    let mut gen_image_url: Option<String> = None;
    let (submit_status, submit_message) = if let Some(submitter) = &executor.generation_submitter {
        let mut snapshot = serde_json::json!({
            "prompt": record.prompt_text,
            "model": args.model_name,
        });
        if let Some(image) = &reference_image {
            snapshot["reference_image_url"] = serde_json::Value::String(image.clone());
        }
        // 视频时长随快照透传：build_unified_request 把整份快照放进
        // UnifiedRequest.parameters，kling/seedance/即梦 adapter 各自按需读取。
        if tool_name != TOOL_IMAGE_GENERATION {
            if let Some(duration) = args.duration_seconds {
                if let Some(number) = serde_json::Number::from_f64(duration) {
                    snapshot["duration"] = serde_json::Value::Number(number);
                }
            }
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
                // 同步路径（即梦代理）直接返回真实结果 URL：remote_job_id 形如
                // "proxy:image:{url}" / "proxy:video:{url}"，统一取 "http" 起始部分。
                // 图片与视频共用提取，前端按扩展名决定 <img>/<video> 渲染。
                gen_image_url = attempt
                    .remote_job_id
                    .as_deref()
                    .and_then(|u| u.find("http").map(|pos| &u[pos..]))
                    .map(|u| u.to_owned());
                let status_msg = if let Some(ref url) = gen_image_url {
                    let media_label = if tool_name == TOOL_IMAGE_GENERATION {
                        "图片"
                    } else {
                        "视频"
                    };
                    format!(
                        "已创建 {tool_name} 任务并提交到 {provider_id}。\
                         {media_label}已生成完成，URL: {url}\
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

    // P3：同步出图路径（代理直接返回图片 URL）——补全 attach 阶段挂的 pending 节点
    //（填 URL/状态；参考连线已在 attach 时建好，此处不再重复匹配）
    let mut canvas_node_id: Option<String> = canvas_pending_node_id.clone();
    if tool_name == TOOL_IMAGE_GENERATION {
        if let Some(url) = &gen_image_url {
            match crate::application::canvas_memory_rag::ensure_generation_image_node(
                &ctx.workspace_path,
                &ctx.conversation_id,
                &record.id,
                url,
                &args.prompt,
                None,
            ) {
                Some((node_id, linked)) => {
                    eprintln!(
                        "[AgentTool] canvas image node ready: task={} node={node_id} reference_linked={linked}",
                        record.id
                    );
                    canvas_node_id = Some(node_id);
                }
                None => eprintln!(
                    "[AgentTool] canvas image node creation failed: {}",
                    record.id
                ),
            }
        }
    }

    let payload = serde_json::json!({
        "taskId": record.id,
        "ok": submit_status != "failed",
        "status": submit_status,
        "providerName": provider_id,
        "modelName": model_name,
        "prompt": record.prompt_text,
        "imageUrl": gen_image_url,
        "canvasNodeId": canvas_node_id,
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

/// 汇总 ProviderRegistry 中已注册的生成 Provider 及其能力。
///
/// 生成能力不只来自 API-Key 凭据表：资源账号（如即梦 session）经
/// ConnectorAdapterBridge 注册进注册表，同样可承担图片/视频生成。
/// 只列凭据表会让 LLM 误判"没有视频凭据"而放弃调用生成工具
/// （19:57 测试复盘：即梦在册但列表为空，video_generation 从未被调用）。
fn generation_providers_payload(registry: &ProviderRegistry) -> Vec<serde_json::Value> {
    registry
        .list_ids()
        .into_iter()
        .filter_map(|id| {
            let adapter = registry.get(&id)?;
            let mut capabilities: Vec<&str> =
                adapter.capabilities().iter().map(|c| c.as_str()).collect();
            capabilities.sort_unstable();
            capabilities.dedup();
            Some(serde_json::json!({
                "provider": adapter.provider_id(),
                "capabilities": capabilities,
            }))
        })
        .collect()
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
    let generation_providers = executor
        .provider_registry
        .as_ref()
        .map(|registry| generation_providers_payload(registry))
        .unwrap_or_default();
    let payload = serde_json::json!({
        "credentials": summary,
        "count": summary.len(),
        "generationProviders": generation_providers,
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
    _executor: &BuiltinToolExecutor,
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
            .map(|(node_id, text, score)| {
                serde_json::json!({ "nodeId": node_id, "text": text, "score": score })
            })
            .collect::<Vec<_>>(),
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

fn execute_canvas_add_note(
    _executor: &BuiltinToolExecutor,
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
    let node_id = crate::application::canvas_memory_rag::add_canvas_note(
        &ctx.workspace_path,
        &ctx.conversation_id,
        args.x,
        args.y,
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
                ctx.current_user_message.as_deref(),
            )
        })
        .unwrap_or(0);
    let (node_count, edge_count) = crate::application::canvas_memory_rag::canvas_stats(
        &ctx.workspace_path,
        &ctx.conversation_id,
    );
    let payload = serde_json::json!({
        "ok": node_id.is_some(),
        "nodeId": node_id,
        "autoConnected": connected,
        "canvasDigest": { "nodes": node_count, "edges": edge_count },
        "nextSuggestions": [
            "用 canvas_connect 把新便签与相关节点连线（建立上下文关联）",
            "继续创建更多便签记录其他结论",
            "节点杂乱时用 canvas_auto_layout 整理"
        ],
        "message": if node_id.is_some() {
            format!("已在画布上创建便签，并自动关联了 {connected} 个相关节点。画布现有 {node_count} 个节点。")
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
    _executor: &BuiltinToolExecutor,
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
    )
    .into_iter()
    .next();
    let target = crate::application::canvas_memory_rag::search_nodes(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &args.target_query,
        1,
    )
    .into_iter()
    .next();
    let (Some((source_id, source_text, _)), Some((target_id, target_text, _))) = (source, target)
    else {
        let payload = serde_json::json!({
            "ok": false,
            "message": "未找到匹配的节点，请检查检索关键词。"
        });
        return Ok(ToolExecutionResult {
            content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
            generation_task_id: None,
        });
    };
    let edge_id = crate::application::canvas_memory_rag::connect_nodes(
        &ctx.workspace_path,
        &ctx.conversation_id,
        &source_id,
        &target_id,
        args.label.as_deref().or(Some("语义相关")),
    );
    let message = if edge_id.is_some() {
        format!("已建立连线：{source_text} → {target_text}")
    } else {
        "两个节点之间已存在连线，未重复创建。".to_owned()
    };
    let payload = serde_json::json!({
        "ok": true,
        "edgeId": edge_id,
        "message": message,
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// canvas_add_image：在画布上创建图片节点并自动关联相关节点。
fn execute_canvas_add_image(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        url: String,
        description: Option<String>,
        x: Option<f64>,
        y: Option<f64>,
    }
    let args: Args = parse_arguments(TOOL_CANVAS_ADD_IMAGE, arguments)?;
    let node_id = crate::application::canvas_memory_rag::add_canvas_image(
        &ctx.workspace_path,
        &ctx.conversation_id,
        args.url.trim(),
        args.description.as_deref(),
        args.x,
        args.y,
        None,
    );
    let connected = node_id
        .as_ref()
        .map(|id| {
            crate::application::canvas_memory_rag::auto_connect_related(
                &ctx.workspace_path,
                &ctx.conversation_id,
                id,
                3,
                ctx.current_user_message.as_deref(),
            )
        })
        .unwrap_or(0);
    let (node_count, edge_count) = crate::application::canvas_memory_rag::canvas_stats(
        &ctx.workspace_path,
        &ctx.conversation_id,
    );
    let payload = serde_json::json!({
        "ok": node_id.is_some(),
        "nodeId": node_id,
        "autoConnected": connected,
        "canvasDigest": { "nodes": node_count, "edges": edge_count },
        "nextSuggestions": [
            "用 canvas_connect 把图片与相关便签连线",
            "需要补充说明时用 canvas_update_node"
        ],
        "message": if node_id.is_some() {
            format!("已在画布上创建图片节点，并自动关联了 {connected} 个相关节点。画布现有 {node_count} 个节点。")
        } else {
            "画布图片节点创建失败。".to_owned()
        }
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// canvas_update_node：更新节点文本 / 颜色。
fn execute_canvas_update_node(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        #[serde(rename = "nodeId")]
        node_id: String,
        text: Option<String>,
        color: Option<String>,
    }
    let args: Args = parse_arguments(TOOL_CANVAS_UPDATE_NODE, arguments)?;
    if args.text.is_none() && args.color.is_none() {
        return Err(AgentToolError::InvalidArguments {
            tool: TOOL_CANVAS_UPDATE_NODE.to_owned(),
            reason: "text 与 color 至少提供一项".to_owned(),
        });
    }
    let updated = crate::application::canvas_memory_rag::update_canvas_node(
        &ctx.workspace_path,
        &ctx.conversation_id,
        args.node_id.trim(),
        args.text.as_deref(),
        args.color.as_deref(),
    );
    let payload = serde_json::json!({
        "ok": updated.is_some(),
        "nodeId": updated,
        "message": if updated.is_some() {
            "节点已更新。".to_owned()
        } else {
            "未找到该节点，请用 canvas_search 重新检索。".to_owned()
        }
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// canvas_auto_layout：按类型分组自动整理画布。
fn execute_canvas_auto_layout(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let _: serde_json::Value = parse_arguments(TOOL_CANVAS_AUTO_LAYOUT, arguments)?;
    let moved = crate::application::canvas_memory_rag::auto_layout_canvas(
        &ctx.workspace_path,
        &ctx.conversation_id,
    );
    let payload = serde_json::json!({
        "ok": true,
        "moved": moved,
        "message": format!("画布整理完成，移动了 {moved} 个节点。"),
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// canvas_export：导出画布 JSON（dataUrl 省略）。
fn execute_canvas_export(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let _: serde_json::Value = parse_arguments(TOOL_CANVAS_EXPORT, arguments)?;
    let exported = crate::application::canvas_memory_rag::export_canvas(
        &ctx.workspace_path,
        &ctx.conversation_id,
    );
    let payload = serde_json::json!({
        "ok": exported.is_some(),
        "canvas": exported,
        "message": if exported.is_some() {
            "画布已导出。".to_owned()
        } else {
            "画布为空或不存在。".to_owned()
        }
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

// ── 插件中心：技能与 MCP（Agent 自助调用） ──

use crate::application::mcp_client;
use crate::application::plugin_service;

/// list_skills：列出已安装技能。
fn execute_list_skills(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    let _: serde_json::Value = parse_arguments(TOOL_LIST_SKILLS, arguments)?;
    let skills = plugin_service::list_skills(&ctx.workspace_path).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_LIST_SKILLS.to_owned(),
            reason: e.to_string(),
        }
    })?;
    let payload = serde_json::json!({
        "count": skills.len(),
        "skills": skills,
        "message": if skills.is_empty() {
            "尚未安装任何技能。".to_owned()
        } else {
            format!("共 {} 个技能，用 use_skill 加载指令。", skills.len())
        }
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// use_skill：返回技能指令正文，要求 LLM 按指令执行。
fn execute_use_skill(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        name: String,
    }
    let args: Args = parse_arguments(TOOL_USE_SKILL, arguments)?;
    let skills = plugin_service::list_skills(&ctx.workspace_path).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_USE_SKILL.to_owned(),
            reason: e.to_string(),
        }
    })?;
    let meta = skills
        .iter()
        .find(|s| s.name == args.name || s.slug == args.name)
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: TOOL_USE_SKILL.to_owned(),
            reason: format!("技能「{}」不存在，用 list_skills 查看可用技能。", args.name),
        })?;
    let body = plugin_service::get_skill_body(&ctx.workspace_path, &meta.slug).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_USE_SKILL.to_owned(),
            reason: e.to_string(),
        }
    })?;
    let content = format!(
        "以下是技能「{}」的指令：

{}

请严格按上述技能指令完成用户任务。",
        meta.name, body
    );
    let payload = serde_json::json!({
        "ok": true,
        "skill": meta.name,
        "instructions": content,
        "message": format!("已加载技能「{}」，请按指令执行。", meta.name)
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// mcp_list_tools：列出 MCP 服务器概览或某服务器的工具清单。
fn execute_mcp_list_tools(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        server: Option<String>,
    }
    let args: Args = parse_arguments(TOOL_MCP_LIST_TOOLS, arguments)?;
    let servers = plugin_service::list_mcp_servers(&ctx.workspace_path).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_MCP_LIST_TOOLS.to_owned(),
            reason: e.to_string(),
        }
    })?;

    let Some(server_name) = args
        .server
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        let enabled: Vec<_> = servers.iter().filter(|s| s.enabled).collect();
        let payload = serde_json::json!({
            "count": enabled.len(),
            "servers": enabled
                .iter()
                .map(|s| serde_json::json!({ "name": s.name, "command": s.command }))
                .collect::<Vec<_>>(),
            "message": if enabled.is_empty() {
                "尚未启用任何 MCP 服务器。".to_owned()
            } else {
                "传 server 名称可查看该服务器提供的工具清单。".to_owned()
            }
        });
        return Ok(ToolExecutionResult {
            content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
            generation_task_id: None,
        });
    };

    let config = servers
        .iter()
        .find(|s| s.enabled && (s.name == server_name || s.id == server_name))
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: TOOL_MCP_LIST_TOOLS.to_owned(),
            reason: format!("MCP 服务器「{server_name}」不存在或未启用。"),
        })?;
    let tools = mcp_client::list_tools(config).map_err(|e| AgentToolError::ExecutionFailed {
        tool: TOOL_MCP_LIST_TOOLS.to_owned(),
        reason: e.to_string(),
    })?;
    let payload = serde_json::json!({
        "server": server_name,
        "count": tools.len(),
        "tools": tools,
        "message": format!("服务器「{server_name}」提供 {} 个工具，用 mcp_call 调用。", tools.len())
    });
    Ok(ToolExecutionResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned()),
        generation_task_id: None,
    })
}

/// mcp_call：调用 MCP 服务器工具。
fn execute_mcp_call(
    _executor: &BuiltinToolExecutor,
    ctx: &ToolContext,
    arguments: &str,
) -> Result<ToolExecutionResult, AgentToolError> {
    #[derive(Debug, Deserialize)]
    struct Args {
        server: String,
        tool: String,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
    }
    let args: Args = parse_arguments(TOOL_MCP_CALL, arguments)?;
    let servers = plugin_service::list_mcp_servers(&ctx.workspace_path).map_err(|e| {
        AgentToolError::ExecutionFailed {
            tool: TOOL_MCP_CALL.to_owned(),
            reason: e.to_string(),
        }
    })?;
    let config = servers
        .iter()
        .find(|s| s.enabled && (s.name == args.server || s.id == args.server))
        .ok_or_else(|| AgentToolError::ExecutionFailed {
            tool: TOOL_MCP_CALL.to_owned(),
            reason: format!("MCP 服务器「{}」不存在或未启用。", args.server),
        })?;
    let content = mcp_client::call_tool(
        config,
        &args.tool,
        args.arguments.unwrap_or(serde_json::json!({})),
    )
    .map_err(|e| AgentToolError::ExecutionFailed {
        tool: TOOL_MCP_CALL.to_owned(),
        reason: e.to_string(),
    })?;
    let payload = serde_json::json!({
        "ok": true,
        "server": args.server,
        "tool": args.tool,
        "content": content,
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
    let _instruction = if let Some(context) = &args.context {
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
    /// 视频生成时长（秒）。仅 video_generation 使用；缺省由 Provider 决定（5）。
    /// 当前主流供应商（kling/seedance）单镜头支持 5/10 秒。
    #[serde(rename = "durationSeconds")]
    duration_seconds: Option<f64>,
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
            current_user_message: None,
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
            current_user_message: None,
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
            current_user_message: None,
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
            current_user_message: None,
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
            current_user_message: None,
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
            current_user_message: None,
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
            current_user_message: None,
        };
        let args = serde_json::json!({"scriptText": ""});
        let error = executor
            .execute(&ctx, TOOL_PARSE_SCRIPT, &args.to_string())
            .unwrap_err();
        assert!(matches!(error, AgentToolError::InvalidArguments { .. }));
    }

    #[test]
    fn lists_all_agent_tools() {
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
        // 画布工具必须完整暴露给 LLM（缺失会导致模型选不到正确工具）
        assert!(names.contains(&TOOL_CANVAS_SEARCH));
        assert!(names.contains(&TOOL_CANVAS_ADD_NOTE));
        assert!(names.contains(&TOOL_CANVAS_CONNECT));
        assert!(names.contains(&TOOL_CANVAS_ADD_IMAGE));
        assert!(names.contains(&TOOL_CANVAS_UPDATE_NODE));
        assert!(names.contains(&TOOL_CANVAS_AUTO_LAYOUT));
        assert!(names.contains(&TOOL_CANVAS_EXPORT));
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
            current_user_message: None,
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

    #[test]
    fn test_generation_providers_payload_lists_capabilities() {
        // 资源账号（如即梦）经注册表暴露生成能力；list_credentials 必须能看到，
        // 否则 LLM 会凭"凭据表为空"误判视频生成不可用。
        let registry = ProviderRegistry::new();
        registry.register(Arc::new(MockImageProvider {
            submit_error: false,
        }));

        let payload = generation_providers_payload(&registry);
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["provider"], "mock-image");
        let caps = payload[0]["capabilities"].as_array().unwrap();
        assert!(caps.iter().any(|c| c == "text_to_image"));
    }

    #[test]
    fn test_generation_providers_payload_empty_registry() {
        let registry = ProviderRegistry::new();
        assert!(generation_providers_payload(&registry).is_empty());
    }
}
