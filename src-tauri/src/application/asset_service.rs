use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::asset_repository::SqliteAssetRepository,
    application::{asset_opener, error::AppError},
    domain::assets::{AssetFilter, AssetImportSummary, AssetRecord, AssetReverificationSummary},
    ports::asset_repository::{AssetRepository, ImportOptions},
};

pub struct AssetService {
    repository: Mutex<Box<dyn AssetRepository>>,
    workspace_directory: PathBuf,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl AssetService {
    pub fn new(
        repository: impl AssetRepository + 'static,
        workspace_directory: PathBuf,
        database_path: PathBuf,
    ) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            workspace_directory,
            database_path,
        }
    }

    pub fn list(
        &self,
        search: Option<String>,
        storage_namespace: Option<String>,
        asset_kind: Option<String>,
        integrity_status: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<AssetRecord>, AppError> {
        let filter = AssetFilter::try_new(
            search,
            storage_namespace,
            asset_kind,
            integrity_status,
            limit,
        )?;
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.list(&filter).map_err(AppError::from)
    }

    pub fn get(&self, asset_id: &str) -> Result<Option<AssetRecord>, AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.get(asset_id).map_err(AppError::from)
    }

    pub fn import(
        &self,
        source_paths: Vec<String>,
        namespace: String,
    ) -> Result<AssetImportSummary, AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository
            .import(source_paths, &namespace, ImportOptions::default())
            .map_err(AppError::from)
    }

    pub fn reverify(&self) -> Result<AssetReverificationSummary, AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.reverify().map_err(AppError::from)
    }

    /// 软删资产，同时级联软删关联的资源关联记录。
    pub fn delete(&self, asset_id: &str) -> Result<(), AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repository.delete(asset_id).map_err(AppError::from)
    }

    pub fn open_file(&self, app: &tauri::AppHandle, asset_id: &str) -> Result<(), AppError> {
        let record = self.get_required(asset_id)?;
        asset_opener::open_asset_file(app, &self.workspace_directory, &record)
            .map_err(AppError::from)
    }

    pub fn open_containing_folder(
        &self,
        app: &tauri::AppHandle,
        asset_id: &str,
    ) -> Result<(), AppError> {
        let record = self.get_required(asset_id)?;
        asset_opener::open_asset_containing_folder(app, &self.workspace_directory, &record)
            .map_err(AppError::from)
    }

    fn get_required(&self, asset_id: &str) -> Result<AssetRecord, AppError> {
        self.get(asset_id)?.ok_or_else(|| {
            AppError::from(asset_opener::AssetOpenError::NotFound(asset_id.to_owned()))
        })
    }
}

impl crate::ports::reloadable::Reloadable for AssetService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteAssetRepository::open(database_path)?;
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
    use tempfile::tempdir;

    use super::AssetService;
    use crate::adapters::sqlite::asset_repository::SqliteAssetRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::application::error::AppError;
    use crate::domain::assets::AssetValidationError;

    fn seed_service() -> (tempfile::TempDir, AssetService) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        // 先用 workspace 仓库跑迁移。
        let workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        drop(workspace);
        let repository = SqliteAssetRepository::open(&path).unwrap();
        let service = AssetService::new(repository, directory.path().to_path_buf(), path);
        (directory, service)
    }

    #[test]
    fn list_returns_empty_when_manifest_is_empty() {
        let (_directory, service) = seed_service();
        let records = service.list(None, None, None, None, None).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn list_rejects_invalid_limit() {
        let (_directory, service) = seed_service();
        let error = service.list(None, None, None, None, Some(-1)).unwrap_err();
        assert!(matches!(
            error,
            AppError::AssetValidation(AssetValidationError::InvalidLimit)
        ));
    }

    #[test]
    fn list_rejects_unknown_namespace_filter() {
        let (_directory, service) = seed_service();
        let error = service
            .list(None, Some("not-a-namespace".to_owned()), None, None, None)
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::AssetValidation(AssetValidationError::InvalidChoice {
                field: "storageNamespace"
            })
        ));
    }

    #[test]
    fn import_returns_empty_summary_for_empty_source_list() {
        let (_directory, service) = seed_service();
        let summary = service.import(Vec::new(), "workspace".to_owned()).unwrap();
        assert!(summary.is_empty());
    }

    #[test]
    fn import_marks_missing_source_file_as_failure() {
        let (_directory, service) = seed_service();
        let summary = service
            .import(
                vec!["this/path/does/not/exist.png".to_owned()],
                "workspace".to_owned(),
            )
            .unwrap();
        assert_eq!(summary.failures.len(), 1);
        assert!(summary.imported.is_empty());
        assert!(summary.skipped.is_empty());
    }
}
