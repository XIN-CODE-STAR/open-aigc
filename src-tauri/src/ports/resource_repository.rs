use thiserror::Error;

use crate::{
    domain::resources::{AssociationDraft, AssociationFilter, AssociationRecord},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum ResourceRepositoryError {
    /// 关联的 asset_id 在 manifest 中不存在或已软删。
    #[error("asset {0} does not exist")]
    AssetNotFound(String),
    /// 同一 asset + context + role 已存在有效关联。
    #[error("association already exists for asset {asset_id}")]
    Duplicate { asset_id: String },
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// 资源关联持久化端口。adapter 实现需保证：
/// - `list` 与 `get` 不返回 `deleted_at` 非空的软删记录。
/// - `create` 在 asset_manifest 中找不到 asset_id 时返回 AssetNotFound。
/// - `create` 命中唯一索引冲突时返回 Duplicate。
pub trait ResourceRepository: Send {
    fn list(
        &mut self,
        filter: &AssociationFilter,
    ) -> Result<Vec<AssociationRecord>, ResourceRepositoryError>;

    fn get(
        &mut self,
        association_id: &str,
    ) -> Result<Option<AssociationRecord>, ResourceRepositoryError>;

    fn create(
        &mut self,
        draft: AssociationDraft,
    ) -> Result<AssociationRecord, ResourceRepositoryError>;

    fn delete(&mut self, association_id: &str) -> Result<(), ResourceRepositoryError>;
}
