#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TITLE_MAX_LENGTH: usize = 200;
const TOOL_NAME_MAX_LENGTH: usize = 80;
const CONTENT_MAX_LENGTH: usize = 16000;
/// 多模态消息（含 base64 图片）的长度上限：10 MB。
const MULTIMODAL_CONTENT_MAX_LENGTH: usize = 10 * 1024 * 1024;
const SYSTEM_PROMPT_MAX_LENGTH: usize = 8000;

/// Agent 会话状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConversationStatus {
    Active,
    Archived,
    Error,
}

impl ConversationStatus {
    pub fn parse(value: &str) -> Result<Self, AgentValidationError> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "error" => Ok(Self::Error),
            _ => Err(AgentValidationError::InvalidChoice { field: "status" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Error => "error",
        }
    }
}

/// Agent 消息角色。完全对齐 OpenAI Chat Completions 协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn parse(value: &str) -> Result<Self, AgentValidationError> {
        match value {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            _ => Err(AgentValidationError::InvalidChoice { field: "role" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// 工具调用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolInvocationStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl ToolInvocationStatus {
    pub fn parse(value: &str) -> Result<Self, AgentValidationError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(AgentValidationError::InvalidChoice {
                field: "invocationStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// 新建会话的草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationDraft {
    pub workspace_id: String,
    pub title: String,
    pub credential_id: String,
    pub system_prompt: Option<String>,
}

impl ConversationDraft {
    pub fn try_new(
        workspace_id: String,
        title: String,
        credential_id: String,
        system_prompt: Option<String>,
    ) -> Result<Self, AgentValidationError> {
        let workspace_id = validate_uuid(workspace_id, "workspaceId")?;
        let title = normalize_required(title, "title", TITLE_MAX_LENGTH)?;
        let credential_id = validate_uuid(credential_id, "credentialId")?;
        let system_prompt =
            normalize_optional(system_prompt, "systemPrompt", SYSTEM_PROMPT_MAX_LENGTH)?;
        Ok(Self {
            workspace_id,
            title,
            credential_id,
            system_prompt,
        })
    }
}

/// 校验并规范化会话标题（用于重命名已有会话），规则与创建会话一致：
/// 去首尾空白、非空、不超过 200 字符、不含控制字符。
pub fn normalize_conversation_title(title: String) -> Result<String, AgentValidationError> {
    normalize_required(title, "title", TITLE_MAX_LENGTH)
}

/// 会话记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub credential_id: String,
    pub system_prompt: Option<String>,
    pub status: ConversationStatus,
    pub execution_mode: ExecutionMode,
    pub loop_state_json: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 消息草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDraft {
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    pub remote_model: Option<String>,
    pub finish_reason: Option<String>,
    pub parent_message_id: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
}

impl MessageDraft {
    pub fn user(conversation_id: String, content: String) -> Result<Self, AgentValidationError> {
        Self::validate_content(&content)?;
        Ok(Self {
            conversation_id: validate_uuid(conversation_id, "conversationId")?,
            role: MessageRole::User,
            content: Some(content),
            tool_calls: Vec::new(),
            tool_call_id: None,
            remote_model: None,
            finish_reason: None,
            parent_message_id: None,
            prompt_tokens: None,
            completion_tokens: None,
        })
    }

    pub fn system(conversation_id: String, content: String) -> Result<Self, AgentValidationError> {
        Self::validate_content(&content)?;
        Ok(Self {
            conversation_id: validate_uuid(conversation_id, "conversationId")?,
            role: MessageRole::System,
            content: Some(content),
            tool_calls: Vec::new(),
            tool_call_id: None,
            remote_model: None,
            finish_reason: None,
            parent_message_id: None,
            prompt_tokens: None,
            completion_tokens: None,
        })
    }

    pub fn assistant(
        conversation_id: String,
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
        remote_model: Option<String>,
        finish_reason: Option<String>,
        prompt_tokens: Option<i64>,
        completion_tokens: Option<i64>,
    ) -> Result<Self, AgentValidationError> {
        if let Some(c) = content.as_deref() {
            Self::validate_content(c)?;
        }
        Ok(Self {
            conversation_id: validate_uuid(conversation_id, "conversationId")?,
            role: MessageRole::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            remote_model,
            finish_reason,
            parent_message_id: None,
            prompt_tokens,
            completion_tokens,
        })
    }

    pub fn tool_message(
        conversation_id: String,
        tool_call_id: String,
        content: String,
    ) -> Result<Self, AgentValidationError> {
        Self::validate_content(&content)?;
        Ok(Self {
            conversation_id: validate_uuid(conversation_id, "conversationId")?,
            role: MessageRole::Tool,
            content: Some(content),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id),
            remote_model: None,
            finish_reason: None,
            parent_message_id: None,
            prompt_tokens: None,
            completion_tokens: None,
        })
    }

    fn validate_content(content: &str) -> Result<(), AgentValidationError> {
        if content.trim().is_empty() {
            return Err(AgentValidationError::Required { field: "content" });
        }
        // 多模态 JSON 消息（包含 images 字段）使用更大的长度限制
        // serde_json BTreeMap 按字母序排列 key，images 在 text 前
        let max_length =
            if content.starts_with(r#"{"images":"#) || content.starts_with(r#"{"text":"#) {
                MULTIMODAL_CONTENT_MAX_LENGTH
            } else {
                CONTENT_MAX_LENGTH
            };
        if content.chars().count() > max_length {
            return Err(AgentValidationError::TooLong {
                field: "content",
                max_length,
            });
        }
        Ok(())
    }
}

/// 消息记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    pub remote_model: Option<String>,
    pub finish_reason: Option<String>,
    pub parent_message_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
}

/// 工具调用草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocationDraft {
    pub message_id: String,
    pub conversation_id: String,
    pub tool_name: String,
    pub arguments_json: String,
}

impl ToolInvocationDraft {
    pub fn try_new(
        message_id: String,
        conversation_id: String,
        tool_name: String,
        arguments_json: String,
    ) -> Result<Self, AgentValidationError> {
        let message_id = validate_uuid(message_id, "messageId")?;
        let conversation_id = validate_uuid(conversation_id, "conversationId")?;
        let tool_name = normalize_required(tool_name, "toolName", TOOL_NAME_MAX_LENGTH)?;
        if arguments_json.trim().is_empty() {
            return Err(AgentValidationError::Required { field: "arguments" });
        }
        serde_json::from_str::<serde_json::Value>(&arguments_json)
            .map_err(|_| AgentValidationError::InvalidChoice { field: "arguments" })?;
        Ok(Self {
            message_id,
            conversation_id,
            tool_name,
            arguments_json,
        })
    }
}

/// 工具调用记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocationRecord {
    pub id: String,
    pub message_id: String,
    pub conversation_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub result_json: Option<String>,
    pub status: ToolInvocationStatus,
    pub error_message: Option<String>,
    pub generation_task_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

// ──────────────────────────────────────────────────────────────────
// OpenAI 兼容 Chat Completions 协议类型（Grok / OpenAI 通用）
// ──────────────────────────────────────────────────────────────────

/// OpenAI 风格的 function tool_call。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    /// 固定为 "function"。
    #[serde(default = "default_tool_call_type")]
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

fn default_tool_call_type() -> String {
    "function".to_owned()
}

impl ToolCall {
    pub fn new(id: String, name: String, arguments: String) -> Self {
        Self {
            id,
            tool_type: "function".to_owned(),
            function: FunctionCall { name, arguments },
        }
    }
}

/// OpenAI 风格的 function call payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    pub name: String,
    /// JSON 字符串形式的参数。
    pub arguments: String,
}

/// 工具定义（OpenAI tools 数组项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    pub fn function(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            tool_type: "function".to_owned(),
            function: FunctionDefinition {
                name: name.to_owned(),
                description: description.to_owned(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Chat 请求（对 LLM 端口）。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    /// 是否启用流式输出。本实现先做非流式，流式由调用方在外层模拟。
    pub stream: bool,
    /// 最大推理轮数（防失控）。
    pub max_iterations: u8,
}

impl ChatRequest {
    pub fn new(model: String, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Self {
        Self {
            model,
            messages,
            tools,
            stream: false,
            max_iterations: 10,
        }
    }
}

/// 对话消息（OpenAI 风格）。
///
/// `content` 使用 `serde_json::Value` 以支持两种格式：
/// - 纯文本：`"Hello"` → 序列化为 JSON string
/// - 多模态：`[{"type":"text","text":"..."}, {"type":"image_url",...}]` → JSON array
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<serde_json::Value>,
    /// assistant 消息才有。
    /// 注意：OpenAI 协议要求蛇形命名 tool_calls，显式 rename 覆盖结构体级 camelCase。
    #[serde(default, rename = "tool_calls", skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// tool 消息才有。
    #[serde(
        default,
        rename = "tool_call_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: String) -> Self {
        Self {
            role: "system".to_owned(),
            content: Some(serde_json::Value::String(content)),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(content: String) -> Self {
        Self {
            role: "user".to_owned(),
            content: Some(serde_json::Value::String(content)),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// 构造包含图片的多模态用户消息。
    /// `text` 为用户文字，`image_data_urls` 为 `data:image/...;base64,...` 格式的图片。
    pub fn user_with_images(text: String, image_data_urls: Vec<String>) -> Self {
        let mut parts = vec![serde_json::json!({
            "type": "text",
            "text": text
        })];
        for url in image_data_urls {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url }
            }));
        }
        Self {
            role: "user".to_owned(),
            content: Some(serde_json::Value::Array(parts)),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: content.map(serde_json::Value::String),
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: String, content: String) -> Self {
        Self {
            role: "tool".to_owned(),
            content: Some(serde_json::Value::String(content)),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id),
        }
    }

    /// 获取纯文本内容（多模态时提取 text 部分）。
    pub fn text_content(&self) -> Option<&str> {
        match &self.content {
            Some(serde_json::Value::String(s)) => Some(s.as_str()),
            Some(serde_json::Value::Array(parts)) => {
                // 提取第一个 type=text 的 text 字段
                parts.iter().find_map(|p| {
                    if p.get("type")?.as_str()? == "text" {
                        p.get("text")?.as_str()
                    } else {
                        None
                    }
                })
            }
            _ => None,
        }
    }

    /// 估算消息内容的字符数（用于 token 预算控制）。
    pub fn content_chars(&self) -> usize {
        match &self.content {
            Some(serde_json::Value::String(s)) => s.len(),
            Some(serde_json::Value::Array(parts)) => parts
                .iter()
                .map(|p| {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0)
                        + p.get("image_url")
                            .and_then(|u| u.get("url"))
                            .and_then(|u| u.as_str())
                            .map(|s| s.len())
                            .unwrap_or(0)
                })
                .sum(),
            Some(v) => v.to_string().len(),
            None => 0,
        }
    }
}

/// Chat 响应（对 LLM 端口）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    /// LLM 返回的内容（finish_reason="stop" 时有值）。
    pub content: Option<String>,
    /// LLM 决定的工具调用（finish_reason="tool_calls" 时有值）。
    pub tool_calls: Vec<ToolCall>,
    /// 完成原因：stop / tool_calls / length / content_filter。
    pub finish_reason: String,
    /// 模型名称（来自 LLM 响应）。
    pub model: String,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
}

/// Agent 错误类型。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} is invalid")]
    InvalidChoice { field: &'static str },
    #[error("{field} is not a valid uuid")]
    InvalidUuid { field: &'static str },
}

impl AgentValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::InvalidChoice { field }
            | Self::InvalidUuid { field } => Some(field),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", label(field)),
            Self::TooLong { field, max_length } => {
                format!("{}不能超过 {max_length} 个字符。", label(field))
            }
            Self::InvalidChoice { field } => format!("{}无效。", label(field)),
            Self::InvalidUuid { field } => format!("{}格式无效。", label(field)),
        }
    }
}

fn label(field: &str) -> &'static str {
    match field {
        "workspaceId" => "工作空间标识",
        "title" => "会话标题",
        "credentialId" => "凭据标识",
        "systemPrompt" => "系统提示",
        "conversationId" => "会话标识",
        "messageId" => "消息标识",
        "content" => "消息内容",
        "role" => "消息角色",
        "toolName" => "工具名称",
        "arguments" => "工具参数",
        "invocationStatus" => "调用状态",
        "status" => "状态",
        _ => "字段",
    }
}

fn validate_uuid(value: String, field: &'static str) -> Result<String, AgentValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AgentValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(AgentValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, AgentValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AgentValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(AgentValidationError::TooLong { field, max_length });
    }
    // 允许常见空白控制字符（\n \r \t），拒绝其他控制字符。
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(AgentValidationError::InvalidChoice { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_optional(
    value: Option<String>,
    field: &'static str,
    max_length: usize,
) -> Result<Option<String>, AgentValidationError> {
    match value {
        None => Ok(None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > max_length {
                return Err(AgentValidationError::TooLong { field, max_length });
            }
            if trimmed
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
            {
                return Err(AgentValidationError::InvalidChoice { field });
            }
            Ok(Some(trimmed.to_owned()))
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Plan-and-Execute 架构类型
// ──────────────────────────────────────────────────────────────────

/// 迭代限制常量。
pub const PLAN_MAX_ITERATIONS: u8 = 3;
pub const EXECUTE_MAX_ITERATIONS: u8 = 15;
pub const TOTAL_MAX_ITERATIONS: u8 = 18;

/// Agent 执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    /// 先规划后执行（新默认）。
    PlanAndExecute,
    /// 传统 ReAct 循环（向后兼容）。
    /// 显式 rename：kebab-case 会把它序列化成 "re-act"，与前端契约 "react" 不符。
    #[serde(rename = "react")]
    ReAct,
}

impl ExecutionMode {
    pub fn parse(value: &str) -> Result<Self, AgentValidationError> {
        match value {
            "plan-and-execute" => Ok(Self::PlanAndExecute),
            "react" => Ok(Self::ReAct),
            _ => Err(AgentValidationError::InvalidChoice {
                field: "executionMode",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanAndExecute => "plan-and-execute",
            Self::ReAct => "react",
        }
    }
}

/// 计划步骤状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanStepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl PlanStepStatus {
    pub fn parse(value: &str) -> Result<Self, AgentValidationError> {
        match value {
            "pending" => Ok(Self::Pending),
            "in-progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(AgentValidationError::InvalidChoice {
                field: "planStepStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in-progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// 计划步骤类型（用于 SkillRegistry 分发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepKind {
    /// 通用任务（LLM 规划产出的默认类型）。
    #[default]
    Task,
    /// 图片生成。
    ImageGeneration,
    /// 视频生成。
    VideoGeneration,
    /// 音频生成。
    AudioGeneration,
    /// 多素材合成（ffmpeg 等）。
    Composite,
    /// AI 评价/审核。
    Critic,
    /// 风格设定。
    StyleSetup,
}

/// 计划中的单个步骤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub index: u8,
    pub description: String,
    pub status: PlanStepStatus,
    /// 步骤类型（SkillRegistry 据此分发到对应 Skill）。
    #[serde(default)]
    pub kind: PlanStepKind,
}

/// 完整计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
}

/// 计划持久化记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub id: String,
    pub conversation_id: String,
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStepStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建计划的草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDraft {
    pub conversation_id: String,
    pub goal: String,
    pub steps: Vec<PlanStep>,
}

impl PlanDraft {
    pub fn try_new(
        conversation_id: String,
        goal: String,
        steps: Vec<PlanStep>,
    ) -> Result<Self, AgentValidationError> {
        let conversation_id = validate_uuid(conversation_id, "conversationId")?;
        let goal = normalize_required(goal, "goal", 2000)?;
        if steps.is_empty() {
            return Err(AgentValidationError::Required { field: "steps" });
        }
        Ok(Self {
            conversation_id,
            goal,
            steps,
        })
    }
}

/// 记忆配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryConfig {
    /// 滑动窗口最大消息数。
    pub max_context_messages: usize,
    /// 是否始终包含系统提示词。
    pub include_system_prompt: bool,
    /// 是否启用长期记忆（Phase 3）。
    pub enable_long_term_memory: bool,
}

impl Default for AgentMemoryConfig {
    fn default() -> Self {
        Self {
            max_context_messages: 20,
            include_system_prompt: true,
            enable_long_term_memory: false,
        }
    }
}

/// Agent 循环状态（用于中断/恢复，如 ask_user_question）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopState {
    /// 当前执行阶段。
    pub phase: String,
    /// 当前计划步骤索引。
    pub current_step_index: u8,
    /// 已用迭代次数。
    pub iterations_used: u8,
    /// 等待回答的工具调用 ID。
    pub pending_tool_call_id: Option<String>,
    /// 等待回答的问题。
    pub pending_question: Option<String>,
}

/// 从 LLM 响应文本中解析编号列表为计划步骤。
pub fn parse_plan_from_text(text: &str) -> Vec<PlanStep> {
    let mut steps = Vec::new();
    let mut index: u8 = 1;

    for line in text.lines() {
        let trimmed = line.trim();
        // 匹配 "1. xxx", "1、xxx", "- xxx", "• xxx" 等格式
        let desc = if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
            // "1. xxx" or "1、xxx"
            rest.trim_start_matches(['.', '、', ' ']).trim()
        } else if let Some(rest) = trimmed.strip_prefix('-') {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix('•') {
            rest.trim()
        } else {
            continue;
        };

        if !desc.is_empty() && desc.len() <= 500 {
            steps.push(PlanStep {
                index,
                description: desc.to_owned(),
                status: PlanStepStatus::Pending,
                kind: PlanStepKind::Task,
            });
            index += 1;
        }
    }

    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn parses_known_statuses() {
        assert_eq!(
            ConversationStatus::parse("active").unwrap(),
            ConversationStatus::Active
        );
        assert_eq!(
            MessageRole::parse("assistant").unwrap(),
            MessageRole::Assistant
        );
        assert_eq!(
            ToolInvocationStatus::parse("succeeded").unwrap(),
            ToolInvocationStatus::Succeeded
        );
        assert!(MessageRole::parse("unknown").is_err());
    }

    #[test]
    fn accepts_valid_conversation_draft() {
        let draft = ConversationDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "测试会话".to_owned(),
            SAMPLE_UUID.to_owned(),
            Some("你是 AIGC Studio 的 Agent".to_owned()),
        )
        .unwrap();
        assert_eq!(draft.title, "测试会话");
        assert_eq!(draft.workspace_id, SAMPLE_UUID);
    }

    #[test]
    fn rejects_empty_title() {
        let error = ConversationDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "  ".to_owned(),
            SAMPLE_UUID.to_owned(),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AgentValidationError::Required { field: "title" }
        ));
    }

    #[test]
    fn builds_user_message() {
        let msg = MessageDraft::user(SAMPLE_UUID.to_owned(), "画一张猫".to_owned()).unwrap();
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content.as_deref(), Some("画一张猫"));
    }

    #[test]
    fn builds_tool_message() {
        let msg = MessageDraft::tool_message(
            SAMPLE_UUID.to_owned(),
            "call_1".to_owned(),
            r#"{"status":"ok"}"#.to_owned(),
        )
        .unwrap();
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn builds_tool_call() {
        let call = ToolCall::new(
            "call_1".to_owned(),
            "image_generation".to_owned(),
            r#"{"prompt":"猫"}"#.to_owned(),
        );
        assert_eq!(call.function.name, "image_generation");
        assert_eq!(call.tool_type, "function");
    }

    #[test]
    fn rejects_too_long_system_prompt() {
        let long_prompt = "a".repeat(8001);
        let error = ConversationDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "标题".to_owned(),
            SAMPLE_UUID.to_owned(),
            Some(long_prompt),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AgentValidationError::TooLong {
                field: "systemPrompt",
                max_length: 8000
            }
        ));
    }

    #[test]
    fn rejects_invalid_arguments_json() {
        let error = ToolInvocationDraft::try_new(
            SAMPLE_UUID.to_owned(),
            SAMPLE_UUID.to_owned(),
            "image_generation".to_owned(),
            "not json".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AgentValidationError::InvalidChoice { field: "arguments" }
        ));
    }

    #[test]
    fn parses_numbered_plan_steps() {
        let text = "1. 生成猫咪图片\n2. 调整风格为日系动漫\n3. 保存到资产库";
        let steps = parse_plan_from_text(text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].description, "生成猫咪图片");
        assert_eq!(steps[1].index, 2);
        assert_eq!(steps[2].status, PlanStepStatus::Pending);
    }

    #[test]
    fn parses_dash_plan_steps() {
        let text = "- 第一步\n- 第二步\n• 第三步";
        let steps = parse_plan_from_text(text);
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn plan_draft_rejects_empty_steps() {
        let error = PlanDraft::try_new(SAMPLE_UUID.to_owned(), "生成图片".to_owned(), Vec::new())
            .unwrap_err();
        assert!(matches!(
            error,
            AgentValidationError::Required { field: "steps" }
        ));
    }

    #[test]
    fn execution_mode_roundtrip() {
        assert_eq!(
            ExecutionMode::parse("plan-and-execute").unwrap(),
            ExecutionMode::PlanAndExecute
        );
        assert_eq!(ExecutionMode::parse("react").unwrap(), ExecutionMode::ReAct);
        assert_eq!(ExecutionMode::PlanAndExecute.as_str(), "plan-and-execute");
    }

    #[test]
    fn execution_mode_serde_matches_frontend_contract() {
        // 前端 executionModeSchema 仅接受 "plan-and-execute" | "react"。
        assert_eq!(
            serde_json::to_string(&ExecutionMode::PlanAndExecute).unwrap(),
            "\"plan-and-execute\""
        );
        assert_eq!(
            serde_json::to_string(&ExecutionMode::ReAct).unwrap(),
            "\"react\""
        );
    }

    #[test]
    fn default_memory_config() {
        let config = AgentMemoryConfig::default();
        assert_eq!(config.max_context_messages, 20);
        assert!(config.include_system_prompt);
        assert!(!config.enable_long_term_memory);
    }
}
