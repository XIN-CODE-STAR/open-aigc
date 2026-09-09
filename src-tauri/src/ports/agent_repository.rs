#![allow(dead_code)]
use thiserror::Error;

use crate::{
    domain::agent::{
        AgentValidationError, ConversationDraft, ConversationRecord, ConversationStatus,
        MessageDraft, MessageRecord, ToolInvocationDraft, ToolInvocationRecord,
        ToolInvocationStatus,
    },
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum AgentRepositoryError {
    #[error("agent conversation {0} does not exist")]
    ConversationNotFound(String),
    #[error("agent message {0} does not exist")]
    MessageNotFound(String),
    #[error("agent tool invocation {0} does not exist")]
    InvocationNotFound(String),
    #[error(transparent)]
    Validation(#[from] AgentValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// Agent 仓储端口：持久化会话、消息、工具调用记录。
/// 实现需保证：
/// - `create_conversation` 插入会话并返回完整记录。
/// - `append_message` 追加消息（按 created_at 升序）。
/// - `create_invocation` / `update_invocation_status` 记录工具调用生命周期。
/// - `list_messages` 返回会话所有消息（含 tool_calls 已反序列化）。
pub trait AgentRepository: Send {
    fn create_conversation(
        &mut self,
        draft: ConversationDraft,
    ) -> Result<ConversationRecord, AgentRepositoryError>;

    fn get_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<Option<ConversationRecord>, AgentRepositoryError>;

    fn list_conversations(
        &mut self,
        workspace_id: &str,
    ) -> Result<Vec<ConversationRecord>, AgentRepositoryError>;

    fn update_conversation_status(
        &mut self,
        conversation_id: &str,
        status: ConversationStatus,
    ) -> Result<ConversationRecord, AgentRepositoryError>;

    fn rename_conversation(
        &mut self,
        conversation_id: &str,
        title: &str,
    ) -> Result<ConversationRecord, AgentRepositoryError>;

    fn delete_conversation(&mut self, conversation_id: &str) -> Result<(), AgentRepositoryError>;

    fn append_message(
        &mut self,
        draft: MessageDraft,
    ) -> Result<MessageRecord, AgentRepositoryError>;

    fn list_messages(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<MessageRecord>, AgentRepositoryError>;

    fn create_invocation(
        &mut self,
        draft: ToolInvocationDraft,
    ) -> Result<ToolInvocationRecord, AgentRepositoryError>;

    fn update_invocation_status(
        &mut self,
        invocation_id: &str,
        status: ToolInvocationStatus,
        result_json: Option<String>,
        error_message: Option<String>,
        generation_task_id: Option<String>,
    ) -> Result<ToolInvocationRecord, AgentRepositoryError>;

    fn list_invocations(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<ToolInvocationRecord>, AgentRepositoryError>;
}
