use thiserror::Error;

use crate::{
    domain::generation::{
        GenerationFilter, GenerationResultRecord, GenerationStatus, GenerationTaskDraft,
        GenerationTaskRecord,
    },
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum GenerationRepositoryError {
    /// 指定的生成任务不存在。
    #[error("generation task {0} does not exist")]
    TaskNotFound(String),
    /// 关联的 asset_id 在 manifest 中不存在或已软删。
    #[error("asset {0} does not exist")]
    AssetNotFound(String),
    /// 同一 task + asset 的结果已存在。
    #[error("generation result already exists for task {task_id} and asset {asset_id}")]
    DuplicateResult { task_id: String, asset_id: String },
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 生成历史持久化端口。adapter 实现需保证：
/// - `create_task` 插入 pending 状态的任务并返回完整记录。
/// - `update_status` 在任务不存在时返回 TaskNotFound。
/// - `add_result` 在 asset_manifest 中找不到 asset_id 时返回 AssetNotFound；
///   命中唯一索引冲突时返回 DuplicateResult。
/// - `list_results` 返回该任务的所有生成结果，按 created_at 升序。
pub trait GenerationRepository: Send {
    fn create_task(
        &mut self,
        draft: GenerationTaskDraft,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError>;

    fn update_status(
        &mut self,
        task_id: &str,
        status: GenerationStatus,
        error_message: Option<String>,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError>;

    fn get_task(
        &mut self,
        task_id: &str,
    ) -> Result<Option<GenerationTaskRecord>, GenerationRepositoryError>;

    fn list_tasks(
        &mut self,
        filter: &GenerationFilter,
    ) -> Result<Vec<GenerationTaskRecord>, GenerationRepositoryError>;

    fn add_result(
        &mut self,
        task_id: &str,
        asset_id: &str,
    ) -> Result<GenerationResultRecord, GenerationRepositoryError>;

    fn list_results(
        &mut self,
        task_id: &str,
    ) -> Result<Vec<GenerationResultRecord>, GenerationRepositoryError>;

    /// 更新任务进度（0-100）。任务不存在时返回 TaskNotFound。
    #[allow(dead_code)]
    fn update_progress(
        &mut self,
        task_id: &str,
        progress: u8,
    ) -> Result<GenerationTaskRecord, GenerationRepositoryError>;
}
