use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

use crate::domain::agent::ToolDefinition;
use crate::domain::sandbox::SandboxPolicy;

/// 工具执行上下文：每次工具调用时由 AgentService 注入。
pub struct ToolContext {
    pub workspace_id: String,
    /// 工作空间目录路径，用于下载生成结果到本地。
    pub workspace_path: PathBuf,
    /// 用户选择的项目输出目录（可选）。生成结果同时保存到此目录。
    pub output_directory: Option<PathBuf>,
    /// 当前对话 ID，用于查询画布上下文。
    pub conversation_id: String,
    /// 沙箱策略：限制工具可访问的路径、网络、命令。
    pub sandbox: Option<Arc<SandboxPolicy>>,
    /// 对话中最近一张用户上传的图片（data URL）。存在时图片生成默认走图生图。
    pub latest_user_image: Option<String>,
}

/// 工具执行结果。
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// 回传给 LLM 的内容（JSON 字符串，作为 role=tool 消息的 content）。
    pub content: String,
    /// 若工具触发了 generation_task，记录关联的任务 ID。
    pub generation_task_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum AgentToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments for tool {tool}: {reason}")]
    InvalidArguments { tool: String, reason: String },
    #[error("tool {tool} execution failed: {reason}")]
    ExecutionFailed { tool: String, reason: String },
    #[error("persistence error: {0}")]
    Persistence(#[from] crate::ports::persistence::PersistenceError),
}

/// Agent 工具执行器端口：每个工具调用都通过此 trait 执行。
///
/// 实现需保证：
/// - `list_tools` 返回稳定的工具定义集合（与 LLM 工具循环中提供的 tools 数组保持一致）。
/// - `execute` 解析参数、调用对应业务逻辑、返回 LLM 可消费的 JSON 文本。
/// - 执行失败时返回 ExecutionFailed，由 AgentService 写入 error_message 后继续循环。
pub trait AgentToolExecutor: Send {
    fn list_tools(&self) -> Vec<ToolDefinition>;
    fn execute(
        &mut self,
        ctx: &ToolContext,
        tool_name: &str,
        arguments: &str,
    ) -> Result<ToolExecutionResult, AgentToolError>;
}
