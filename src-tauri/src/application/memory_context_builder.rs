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

    /// 画布结论沉淀：把 Agent 写入画布的便签内容写入长期记忆。
    /// 长期记忆未启用（EverOS 未配置）时为无操作。
    pub fn store_canvas_facts(&self, workspace_id: &str, facts: &[String]) {
        if facts.is_empty() {
            return;
        }
        if let Some(memory) = &self.memory_service {
            if memory.is_available() {
                let _ = memory.remember_facts(workspace_id, facts);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::AppError;
    use crate::domain::agent::MessageRecord;
    use crate::ports::memory_service::MemoryResult;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct SpyMemoryService {
        stored_facts: Mutex<Vec<Vec<String>>>,
    }

    impl MemoryServicePort for SpyMemoryService {
        fn remember_conversation(
            &self,
            _workspace_id: &str,
            _conversation_id: &str,
            _messages: &[MessageRecord],
        ) -> Result<(), AppError> {
            Ok(())
        }

        fn remember_facts(&self, _workspace_id: &str, facts: &[String]) -> Result<(), AppError> {
            self.stored_facts.lock().unwrap().push(facts.to_vec());
            Ok(())
        }

        fn recall(
            &self,
            _workspace_id: &str,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryResult>, AppError> {
            Ok(Vec::new())
        }

        fn is_available(&self) -> bool {
            true
        }

        fn start(&self) -> Result<(), AppError> {
            Ok(())
        }

        fn stop(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    impl MemoryServicePort for Arc<SpyMemoryService> {
        fn remember_conversation(
            &self,
            workspace_id: &str,
            conversation_id: &str,
            messages: &[MessageRecord],
        ) -> Result<(), AppError> {
            (**self).remember_conversation(workspace_id, conversation_id, messages)
        }

        fn remember_facts(&self, workspace_id: &str, facts: &[String]) -> Result<(), AppError> {
            (**self).remember_facts(workspace_id, facts)
        }

        fn recall(
            &self,
            workspace_id: &str,
            query: &str,
            limit: usize,
        ) -> Result<Vec<MemoryResult>, AppError> {
            (**self).recall(workspace_id, query, limit)
        }

        fn is_available(&self) -> bool {
            (**self).is_available()
        }

        fn start(&self) -> Result<(), AppError> {
            (**self).start()
        }

        fn stop(&self) -> Result<(), AppError> {
            (**self).stop()
        }
    }

    #[test]
    fn store_canvas_facts_forwards_and_skips_empty() {
        let spy = Arc::new(SpyMemoryService::default());
        let builder = MemoryContextBuilder::new(Some(Arc::clone(&spy)));

        builder.store_canvas_facts(
            "ws-1",
            &[
                "[画布结论] 用户偏好暗色调".to_owned(),
                "[画布结论] 主角是狐狸".to_owned(),
            ],
        );
        builder.store_canvas_facts("ws-1", &[]); // 空列表不应触发

        let stored = spy.stored_facts.lock().unwrap();
        assert_eq!(stored.len(), 1, "空列表不应转发");
        assert_eq!(stored[0].len(), 2);
    }
}
