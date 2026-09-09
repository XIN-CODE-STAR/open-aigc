use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::resource_repository::SqliteResourceRepository,
    application::error::AppError,
    domain::resources::{
        AssociationDraft, AssociationFilter, AssociationRecord, ResourceValidationError,
    },
    ports::resource_repository::ResourceRepository,
};

pub struct ResourceService {
    repository: Mutex<Box<dyn ResourceRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl ResourceService {
    pub fn new(repository: impl ResourceRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    pub fn list(
        &self,
        asset_id: Option<String>,
        context_kind: Option<String>,
        context_ref: Option<String>,
        role: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<AssociationRecord>, AppError> {
        let filter = AssociationFilter::try_new(asset_id, context_kind, context_ref, role, limit)?;
        self.with_repository(|repository| repository.list(&filter).map_err(Into::into))
    }

    pub fn get(&self, association_id: &str) -> Result<Option<AssociationRecord>, AppError> {
        validate_id(association_id, "associationId")?;
        self.with_repository(|repository| repository.get(association_id).map_err(Into::into))
    }

    pub fn create(
        &self,
        asset_id: String,
        context_kind: String,
        context_ref: String,
        role: String,
        notes: Option<String>,
    ) -> Result<AssociationRecord, AppError> {
        let draft = AssociationDraft::try_new(asset_id, context_kind, context_ref, role, notes)?;
        self.with_repository(|repository| repository.create(draft).map_err(Into::into))
    }

    pub fn delete(&self, association_id: &str) -> Result<(), AppError> {
        validate_id(association_id, "associationId")?;
        self.with_repository(|repository| repository.delete(association_id).map_err(Into::into))
    }

    fn with_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn ResourceRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }
}

fn validate_id(value: &str, field: &'static str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::ResourceValidation(
            ResourceValidationError::Required { field },
        ));
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(AppError::ResourceValidation(
            ResourceValidationError::InvalidUuid { field },
        ));
    }
    Ok(())
}

impl crate::ports::reloadable::Reloadable for ResourceService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteResourceRepository::open(database_path)?;
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

    use super::ResourceService;
    use crate::adapters::sqlite::asset_repository::SqliteAssetRepository;
    use crate::adapters::sqlite::resource_repository::SqliteResourceRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::application::error::AppError;
    use crate::domain::resources::ResourceValidationError;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::asset_repository::{AssetRepository, ImportOptions};
    use crate::ports::resource_repository::ResourceRepositoryError;
    use crate::ports::workspace_repository::WorkspaceRepository;

    const UNKNOWN_UUID: &str = "99999999-9999-4999-9999-999999999999";

    fn seed() -> (tempfile::TempDir, ResourceService, String) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
            .unwrap();
        drop(workspace);

        // 通过公开的 import API 创建一条真实资产，避免触碰 sqlite 内部 helper。
        let source_file = directory.path().join("source.png");
        fs::write(&source_file, b"fake-png-content").unwrap();
        let mut asset_repository = SqliteAssetRepository::open(&path).unwrap();
        let summary = asset_repository
            .import(
                vec![source_file.to_string_lossy().into_owned()],
                "workspace",
                ImportOptions::default(),
            )
            .unwrap();
        assert_eq!(summary.imported.len(), 1);
        let asset_id = summary.imported[0].asset.id.clone();
        drop(asset_repository);

        let resource_repository = SqliteResourceRepository::open(&path).unwrap();
        let service = ResourceService::new(resource_repository, path);
        (directory, service, asset_id)
    }

    #[test]
    fn creates_and_lists_association() {
        let (_directory, service, asset_id) = seed();
        let created = service
            .create(
                asset_id.clone(),
                "teaching-resource".to_owned(),
                "classroom:abc".to_owned(),
                "source".to_owned(),
                Some("教案原图".to_owned()),
            )
            .unwrap();

        let records = service
            .list(Some(asset_id), None, None, None, None)
            .unwrap();
        assert_eq!(records, vec![created.clone()]);

        let fetched = service.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched, created);
    }

    #[test]
    fn rejects_create_for_missing_asset() {
        let (_directory, service, _asset_id) = seed();
        let error = service
            .create(
                UNKNOWN_UUID.to_owned(),
                "teaching-resource".to_owned(),
                String::new(),
                "source".to_owned(),
                None,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::ResourceRepository(ResourceRepositoryError::AssetNotFound(_))
        ));
    }

    #[test]
    fn rejects_invalid_association_id_on_get() {
        let (_directory, service, _asset_id) = seed();
        let error = service.get("not-a-uuid").unwrap_err();
        assert!(matches!(
            error,
            AppError::ResourceValidation(ResourceValidationError::InvalidUuid {
                field: "associationId"
            })
        ));
    }

    #[test]
    fn deletes_association() {
        let (_directory, service, asset_id) = seed();
        let created = service
            .create(
                asset_id,
                "teaching-resource".to_owned(),
                String::new(),
                "reference".to_owned(),
                None,
            )
            .unwrap();
        service.delete(&created.id).unwrap();
        assert!(service.get(&created.id).unwrap().is_none());
    }
}
