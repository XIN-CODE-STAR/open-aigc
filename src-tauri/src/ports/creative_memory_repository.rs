use thiserror::Error;

use crate::{
    domain::creative_memory::{
        CreativeMemoryDraft, CreativeMemoryEventRecord, CreativeMemoryRecord, MemoryEventType,
        MemoryFilter, MemoryStatus,
    },
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum CreativeMemoryRepositoryError {
    #[error("creative memory {0} does not exist")]
    NotFound(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub trait CreativeMemoryRepository: Send {
    /// 创建新的记忆记录。
    fn insert(
        &mut self,
        draft: &CreativeMemoryDraft,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError>;

    /// 按 ID 读取记忆。
    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<CreativeMemoryRecord>, CreativeMemoryRepositoryError>;

    /// 按筛选条件列出记忆。
    fn list(
        &mut self,
        filter: &MemoryFilter,
    ) -> Result<Vec<CreativeMemoryRecord>, CreativeMemoryRepositoryError>;

    /// 更新记忆状态。
    fn update_status(
        &mut self,
        id: &str,
        status: MemoryStatus,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError>;

    /// 更新记忆内容（用于用户编辑偏好）。
    fn update_content(
        &mut self,
        id: &str,
        content_json: &str,
        summary: &str,
    ) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError>;

    /// 确认记忆（增加确认次数和置信度）。
    fn confirm(&mut self, id: &str) -> Result<CreativeMemoryRecord, CreativeMemoryRepositoryError>;

    /// 删除记忆（软删除，标记为 archived）。
    fn archive(&mut self, id: &str) -> Result<(), CreativeMemoryRepositoryError>;

    /// 插入记忆事件。
    fn insert_event(
        &mut self,
        memory_id: &str,
        event_type: MemoryEventType,
        event_detail: Option<&str>,
        created_by: &str,
    ) -> Result<CreativeMemoryEventRecord, CreativeMemoryRepositoryError>;

    /// 列出某条记忆的事件历史。
    fn list_events(
        &mut self,
        memory_id: &str,
    ) -> Result<Vec<CreativeMemoryEventRecord>, CreativeMemoryRepositoryError>;
}
