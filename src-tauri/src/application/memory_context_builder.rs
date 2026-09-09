//! 记忆上下文构建器：从 AgentService 提取的记忆召回/存储逻辑。
//!
//! 职责：
//! - 从长期记忆中检索相关内容（recall_memory_with_entries）
//! - 对话完成后存储到长期记忆（store_memory）

use crate::ports::memory_service::MemoryServicePort;

use super::agent_service::MemoryRecallEntry;

/// 记忆上下文构建器：负责记忆召回与存储。
///
/// 从 AgentService.recall_memory_with_entries() + store_memory() 提取。
pub(crate) struct MemoryContextBuilder {
    memory_service: Option<Box<dyn MemoryServicePort>>,
}

impl MemoryContextBuilder {
    pub fn new(memory_service: Option<impl MemoryServicePort + 'static>) -> Self {
        Self {
            memory_service: memory_service.map(|m| Box::new(m) as Box<dyn MemoryServicePort>),
        }
    }

    /// 从长期记忆中检索相关内容，返回格式化文本和原始条目。
    pub fn recall_memory_with_entries(
        &self,
        workspace_id: &str,
        query: &str,
    ) -> (Option<String>, Vec<MemoryRecallEntry>) {
        let memory = match self.memory_service.as_ref() {
            Some(m) if m.is_available() => m,
            _ => return (None, Vec::new()),
        };
        let results = match memory.recall(workspace_id, query, 5) {
            Ok(r) if !r.is_empty() => r,
            _ => return (None, Vec::new()),
        };

        let entries: Vec<MemoryRecallEntry> = results
            .iter()
            .map(|r| MemoryRecallEntry {
                source_type: r.source_type.clone(),
                content: r.content.clone(),
                score: r.score,
            })
            .collect();

        let lines: Vec<String> = results
            .iter()
            .map(|r| {
                let prefix = match r.source_type.as_str() {
                    "episode" => "来自对话",
                    "fact" => "已知事实",
                    "profile" => "学生画像",
                    _ => "记忆",
                };
                format!("- ({prefix}) {}", r.content)
            })
            .collect();

        (Some(lines.join("\n")), entries)
    }

    /// 对话完成后存储到长期记忆。
    pub fn store_memory(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        messages: &[crate::domain::agent::MessageRecord],
    ) {
        if let Some(memory) = &self.memory_service {
            if memory.is_available() {
                let _ = memory.remember_conversation(workspace_id, conversation_id, messages);
            }
        }
    }
}
