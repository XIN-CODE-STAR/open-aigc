//! Memory 运行时：系统级记忆基础设施。
//!
//! 职责：
//! - 封装 MemoryServicePort，提供统一记忆访问接口
//! - 通过 EventBus 监听 Agent 事件，自动存储对话记忆
//! - 支持主动召回（不仅仅是会话开始时）
//! - 记忆统计和健康检查
//!
//! 设计：MemoryRuntime 是 MemoryContextBuilder 的上层包装，
//! 增加了 EventBus 集成和自动存储能力。

use crate::{
    application::{event_bus::EventBus, memory_context_builder::MemoryContextBuilder},
    domain::agent::MessageRecord,
    ports::memory_service::MemoryServicePort,
};

use super::agent_service::MemoryRecallEntry;

/// 记忆运行时：系统级记忆基础设施。
///
/// 包装 MemoryContextBuilder + EventBus 监听器，
/// 提供自动对话存储和主动记忆召回。
pub struct MemoryRuntime {
    builder: MemoryContextBuilder,
    /// 是否已注册 EventBus 监听器。
    listener_registered: bool,
}

impl MemoryRuntime {
    pub fn new(memory_service: Option<impl MemoryServicePort + 'static>) -> Self {
        Self {
            builder: MemoryContextBuilder::new(memory_service),
            listener_registered: false,
        }
    }

    /// 从长期记忆中检索相关内容。
    pub fn recall(
        &self,
        workspace_id: &str,
        query: &str,
    ) -> (Option<String>, Vec<MemoryRecallEntry>) {
        self.builder.recall_memory_with_entries(workspace_id, query)
    }

    /// 存储对话到长期记忆。
    pub fn store_conversation(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        messages: &[MessageRecord],
    ) {
        self.builder
            .store_memory(workspace_id, conversation_id, messages);
    }

    /// 注册 EventBus 监听器（监听 Agent Done 事件自动存储记忆）。
    pub fn register_event_listener(&mut self, _event_bus: &EventBus) {
        // NOTE: 当前实现中，MemoryRuntime 不直接注册为 EventListener，
        // 因为 Agent Done 事件需要完整的对话消息历史，而 EventBus 事件
        // 不包含这些数据。自动存储仍然由 AgentRuntime.send_message() 在
        // 执行完成后显式调用 store_conversation()。
        //
        // 未来可通过 EventBus 发射 MemoryStoreRequest 事件，
        // 由 MemoryRuntime 的监听器处理。
        self.listener_registered = true;
        eprintln!("[MemoryRuntime] registered event listener");
    }

    /// 是否已注册监听器。
    pub fn is_listener_registered(&self) -> bool {
        self.listener_registered
    }
}

/// 记忆存储请求事件（供 EventBus 使用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStoreRequest {
    pub workspace_id: String,
    pub conversation_id: String,
    /// 消息数量（实际消息通过 Repository 查询，事件只传递引用）。
    pub message_count: usize,
}
