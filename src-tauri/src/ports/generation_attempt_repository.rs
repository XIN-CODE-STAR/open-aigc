use crate::domain::generation::{AttemptStatus, GenerationAttemptDraft, GenerationAttemptRecord};
use crate::ports::persistence::PersistenceError;

#[derive(Debug, thiserror::Error)]
pub enum AttemptRepositoryError {
    #[error("generation attempt {0} not found")]
    NotFound(String),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 生成尝试仓储端口。
pub trait GenerationAttemptRepository: Send {
    fn create(
        &mut self,
        draft: GenerationAttemptDraft,
    ) -> Result<GenerationAttemptRecord, AttemptRepositoryError>;
    fn get(&mut self, id: &str) -> Result<Option<GenerationAttemptRecord>, AttemptRepositoryError>;
    fn list_by_task(
        &mut self,
        task_id: &str,
    ) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError>;
    fn update_status(
        &mut self,
        id: &str,
        status: AttemptStatus,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<GenerationAttemptRecord, AttemptRepositoryError>;
    fn set_remote_job_id(
        &mut self,
        id: &str,
        remote_job_id: &str,
    ) -> Result<(), AttemptRepositoryError>;
    fn update_progress(&mut self, id: &str, progress: u8) -> Result<(), AttemptRepositoryError>;
    fn set_result_asset(&mut self, id: &str, asset_id: &str) -> Result<(), AttemptRepositoryError>;
    fn list_active(&mut self) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError>;
    /// 递增连续失败计数并更新 last_poll_at。
    fn increment_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError>;
    /// 重置连续失败计数（轮询成功时调用）。
    fn reset_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError>;
    /// 设置 Provider 标识。
    fn set_provider_id(
        &mut self,
        id: &str,
        provider_id: &str,
    ) -> Result<(), AttemptRepositoryError>;
    /// 更新最后轮询时间。
    fn update_last_poll(&mut self, id: &str) -> Result<(), AttemptRepositoryError>;
}
