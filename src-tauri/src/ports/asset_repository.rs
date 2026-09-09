use thiserror::Error;

use crate::{
    domain::assets::{AssetFilter, AssetImportSummary, AssetRecord, AssetReverificationSummary},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum AssetRepositoryError {
    /// staging 写入本身不可用（磁盘满、权限问题），整个批次无法继续。
    #[error(transparent)]
    Staging(#[from] StagingError),
    /// 导入流程中 manifest 插入或文件移动失败。
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    /// 复检过程中受管目录不可用（无法读取文件计算哈希）。
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
}

#[derive(Debug, Error)]
pub enum StagingError {
    #[error("staging directory is unavailable: {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("managed assets directory is unavailable: {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ImportOptions {
    pub max_bytes: i64,
}

impl Default for ImportOptions {
    fn default() -> Self {
        // 教学场景下单个资产 512 MiB 是足够保守的上限；视频生成结果可后续单独配置。
        Self {
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

/// 资产持久化与导入的端口。adapter 端实现需要保证：
/// - `import` 中单条文件的失败不会污染同批其他文件，且 staging 残留会被清理。
/// - `list` 与 `get` 不返回 `deleted_at` 非空的软删资产。
/// - `import` 返回的 summary 同时包含成功、跳过和失败，单条失败不作为 Err 抛出。
/// - `import` 内部会读取 device 表确定 origin_device_id，调用方无需注入。
/// - `reverify` 扫描所有未软删的 manifest 记录，更新 integrity_status。
/// - `delete` 软删资产时级联软删所有关联的 resource_association 记录。
pub trait AssetRepository: Send {
    fn list(&mut self, filter: &AssetFilter) -> Result<Vec<AssetRecord>, PersistenceError>;

    fn get(&mut self, asset_id: &str) -> Result<Option<AssetRecord>, PersistenceError>;

    /// 按顺序处理每条源文件。只有当 staging 写入本身不可用（如磁盘满、权限问题）时才返回 Err。
    fn import(
        &mut self,
        source_paths: Vec<String>,
        namespace: &str,
        options: ImportOptions,
    ) -> Result<AssetImportSummary, AssetRepositoryError>;

    /// 扫描所有未软删的 manifest 记录，按受管文件实际存在性和哈希更新 integrity_status。
    /// `quarantined` 状态的资产保持不变，由人工或后续流程处理。
    fn reverify(&mut self) -> Result<AssetReverificationSummary, AssetRepositoryError>;

    /// 软删资产，同时级联软删关联的 resource_association 记录。
    fn delete(&mut self, asset_id: &str) -> Result<(), AssetRepositoryError>;
}
