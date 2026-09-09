use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::backup_repository::SqliteBackupRepository,
    application::error::AppError,
    domain::backup::{BackupDraft, BackupSummary, RestorePreview, RestoreSummary},
    ports::backup_repository::BackupRepository,
};

pub struct BackupService {
    repository: Mutex<Box<dyn BackupRepository>>,
    workspace_directory: PathBuf,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl BackupService {
    pub fn new(
        repository: impl BackupRepository + 'static,
        workspace_directory: PathBuf,
        database_path: PathBuf,
    ) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            workspace_directory,
            database_path,
        }
    }

    /// 创建一份完整工作区备份。`archive_path` 由前端通过文件保存对话框选择。
    #[allow(clippy::too_many_arguments)]
    pub fn create_backup(
        &self,
        archive_path: String,
        workspace_name: String,
        teacher_name: String,
        schema_version: i32,
        asset_count: i64,
        total_bytes: i64,
        note: Option<String>,
    ) -> Result<BackupSummary, AppError> {
        validate_archive_path(&archive_path)?;
        let draft = BackupDraft::try_new(
            workspace_name,
            teacher_name,
            schema_version,
            asset_count,
            total_bytes,
            note,
        )?;
        let path = PathBuf::from(&archive_path);
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository
            .create_backup(&path, draft)
            .map_err(AppError::from)
    }

    /// 读取备份归档的预览信息。不修改任何本地状态。
    pub fn preview_restore(&self, archive_path: String) -> Result<RestorePreview, AppError> {
        validate_archive_path(&archive_path)?;
        let path = PathBuf::from(&archive_path);
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.preview_restore(&path).map_err(AppError::from)
    }

    /// 从备份归档恢复工作区。会自动先创建安全备份。
    pub fn restore_backup(&self, archive_path: String) -> Result<RestoreSummary, AppError> {
        validate_archive_path(&archive_path)?;
        let path = PathBuf::from(&archive_path);
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.restore_backup(&path).map_err(AppError::from)
    }

    #[allow(dead_code)]
    pub fn workspace_directory(&self) -> &PathBuf {
        &self.workspace_directory
    }
}

/// 简单校验前端传入的归档路径：不能为空、不能含控制字符。
/// 进一步路径安全（受管目录等）由 adapter 在操作时检查。
fn validate_archive_path(path: &str) -> Result<(), AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::BackupValidation(
            crate::domain::backup::BackupValidationError::Required {
                field: "archivePath",
            },
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::BackupValidation(
            crate::domain::backup::BackupValidationError::ControlCharacters {
                field: "archivePath",
            },
        ));
    }
    Ok(())
}

impl crate::ports::reloadable::Reloadable for BackupService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteBackupRepository::open(database_path)?;
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repository = Box::new(new_repository);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::BackupService;
    use crate::adapters::sqlite::backup_repository::SqliteBackupRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::application::error::AppError;
    use crate::domain::backup::BackupValidationError;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed_service() -> (tempfile::TempDir, BackupService) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("春季课程", "王老师").unwrap())
            .unwrap();
        drop(workspace);
        let repository = SqliteBackupRepository::open(&path).unwrap();
        let service = BackupService::new(repository, directory.path().to_path_buf(), path);
        (directory, service)
    }

    #[test]
    fn rejects_empty_archive_path() {
        let (_directory, service) = seed_service();
        let error = service
            .create_backup(
                "   ".to_owned(),
                "春季课程".to_owned(),
                "王老师".to_owned(),
                6,
                0,
                0,
                None,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::BackupValidation(BackupValidationError::Required {
                field: "archivePath"
            })
        ));
    }

    #[test]
    fn creates_backup_and_previews_restore() {
        let (directory, service) = seed_service();
        let archive_path = directory.path().join("backup.zip");
        let summary = service
            .create_backup(
                archive_path.to_string_lossy().into_owned(),
                "春季课程".to_owned(),
                "王老师".to_owned(),
                8,
                0,
                0,
                Some("期末备份".to_owned()),
            )
            .unwrap();
        assert!(archive_path.exists());
        assert_eq!(summary.archive_path, archive_path.to_string_lossy());

        let preview = service
            .preview_restore(archive_path.to_string_lossy().into_owned())
            .unwrap();
        assert_eq!(preview.manifest.workspace_name, "春季课程");
        assert_eq!(preview.manifest.note.as_deref(), Some("期末备份"));
    }

    #[test]
    fn restores_from_backup_and_clears_local_files() {
        let (directory, service) = seed_service();
        let assets_dir = directory.path().join("managed-files").join("assets");
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(assets_dir.join("a.png"), b"alpha").unwrap();

        let archive_path = directory.path().join("backup.zip");
        let schema_version = crate::adapters::sqlite::database::current_schema_version(
            &rusqlite::Connection::open(directory.path().join("workspace.sqlite3")).unwrap(),
        )
        .unwrap();
        service
            .create_backup(
                archive_path.to_string_lossy().into_owned(),
                "春季课程".to_owned(),
                "王老师".to_owned(),
                schema_version,
                0,
                0,
                None,
            )
            .unwrap();

        // 修改本地状态。
        fs::remove_file(assets_dir.join("a.png")).unwrap();
        fs::write(assets_dir.join("c.png"), b"gamma").unwrap();

        let summary = service
            .restore_backup(archive_path.to_string_lossy().into_owned())
            .unwrap();
        assert!(summary.requires_restart);
        assert!(assets_dir.join("a.png").exists());
        assert!(!assets_dir.join("c.png").exists());
    }
}
