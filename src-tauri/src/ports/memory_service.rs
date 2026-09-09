//! 记忆服务端口。
#![allow(dead_code)]
//!
//! 定义长期记忆的存储和检索接口。
//! 实现可以是 EverOS HTTP 客户端，也可以是其他记忆后端。

use crate::application::error::AppError;
use crate::domain::agent::MessageRecord;

/// 记忆检索结果。
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// 结果来源类型（episode / fact / profile）。
    pub source_type: String,
    /// 摘要或事实内容。
    pub content: String,
    /// 相关性评分（0.0-1.0）。
    pub score: f64,
    /// 来源时间（ISO 8601）。
    pub timestamp: Option<String>,
}

/// 记忆服务端口。
pub trait MemoryServicePort: Send + Sync {
    /// 存储对话消息到长期记忆。
    fn remember_conversation(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        messages: &[MessageRecord],
    ) -> Result<(), AppError>;

    /// 检索与查询相关的记忆（RAG）。
    fn recall(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, AppError>;

    /// 检查记忆服务是否可用。
    fn is_available(&self) -> bool;

    /// 启动记忆服务进程。
    fn start(&self) -> Result<(), AppError>;

    /// 停止记忆服务进程。
    fn stop(&self) -> Result<(), AppError>;
}
