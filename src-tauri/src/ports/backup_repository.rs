use std::path::Path;

use thiserror::Error;

use crate::{
    domain::backup::{BackupDraft, BackupSummary, RestorePreview, RestoreSummary},
    ports::persistence::PersistenceError,
};

#[derive(Debug, Error)]
pub enum BackupRepositoryError {
    /// 备份归档路径不可写或不可读。
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    /// 备份归档本身损坏（zip 解压失败、manifest 缺失、SQLite 快照校验失败等）。
    #[error("backup archive is invalid: {0}")]
    InvalidArchive(String),
    /// 备份的 schema 版本与当前应用不兼容。
    #[error("backup schema version {backup_version} is incompatible with current version {current_version}")]
    IncompatibleSchema {
        backup_version: i32,
        current_version: i32,
    },
}

/// 备份与恢复端口。adapter 实现需保证：
/// - `create_backup` 原子写入：先写临时文件再 rename，避免半成品归档。
/// - `preview_restore` 只读不写，安全调用多次。
/// - `restore_backup` 在恢复前自动创建安全备份；恢复失败时尝试回滚到安全备份。
/// - `restore_backup` 完成后调用方应触发应用重启以让所有服务重新加载数据库。
pub trait BackupRepository: Send {
    /// 创建一份完整工作区备份到指定的归档路径。
    /// 归档内容：SQLite 快照、managed-files 目录、manifest.json。
    fn create_backup(
        &mut self,
        archive_path: &Path,
        draft: BackupDraft,
    ) -> Result<BackupSummary, BackupRepositoryError>;

    /// 读取备份归档的预览信息，不修改任何本地状态。
    fn preview_restore(
        &mut self,
        archive_path: &Path,
    ) -> Result<RestorePreview, BackupRepositoryError>;

    /// 从备份归档恢复：先创建安全备份，再替换 SQLite 与 managed-files。
    /// 成功后调用方应提示用户重启应用。
    fn restore_backup(
        &mut self,
        archive_path: &Path,
    ) -> Result<RestoreSummary, BackupRepositoryError>;
}
