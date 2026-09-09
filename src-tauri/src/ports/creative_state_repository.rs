use crate::domain::creative_state::{CreativeStateDraft, CreativeStateRecord};
use crate::ports::persistence::PersistenceError;

#[derive(Debug, thiserror::Error)]
pub enum CreativeStateRepositoryError {
    #[error("creative state {0} not found")]
    NotFound(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 创作状态仓储端口。
pub trait CreativeStateRepository: Send {
    /// 创建创作状态。
    fn create(
        &mut self,
        draft: CreativeStateDraft,
    ) -> Result<CreativeStateRecord, CreativeStateRepositoryError>;
    /// 按 ID 获取。
    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<CreativeStateRecord>, CreativeStateRepositoryError>;
    /// 按工作区获取（一个工作区一个创作状态）。
    fn get_by_workspace(
        &mut self,
        workspace_id: &str,
    ) -> Result<Option<CreativeStateRecord>, CreativeStateRepositoryError>;
    /// 更新整体状态（JSON 列全量替换）。
    fn update(&mut self, record: &CreativeStateRecord) -> Result<(), CreativeStateRepositoryError>;
    /// 删除。
    fn delete(&mut self, id: &str) -> Result<(), CreativeStateRepositoryError>;
}
