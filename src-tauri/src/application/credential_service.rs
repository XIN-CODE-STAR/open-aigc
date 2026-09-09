use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::credential_repository::SqliteCredentialRepository,
    application::error::AppError,
    domain::credentials::{CredentialDraft, CredentialRecord},
    ports::credential_repository::CredentialRepository,
};

pub struct CredentialService {
    repository: Mutex<Box<dyn CredentialRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl CredentialService {
    pub fn new(repository: impl CredentialRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn list(&self) -> Result<Vec<CredentialRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list().map_err(AppError::from)
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Result<Option<CredentialRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get(id).map_err(AppError::from)
    }

    pub fn create(
        &self,
        draft: CredentialDraft,
        secret: String,
    ) -> Result<CredentialRecord, AppError> {
        if secret.trim().is_empty() {
            return Err(AppError::CredentialValidation(
                crate::domain::credentials::CredentialValidationError::Required { field: "apiKey" },
            ));
        }
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create(draft, secret).map_err(AppError::from)
    }

    pub fn update(
        &self,
        id: &str,
        draft: CredentialDraft,
        secret: Option<String>,
    ) -> Result<CredentialRecord, AppError> {
        if let Some(ref secret) = secret {
            if secret.trim().is_empty() {
                return Err(AppError::CredentialValidation(
                    crate::domain::credentials::CredentialValidationError::Required {
                        field: "apiKey",
                    },
                ));
            }
        }
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update(id, draft, secret).map_err(AppError::from)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.delete(id).map_err(AppError::from)
    }

    /// 读取密钥明文。仅在 Vision API 等需要调用外部服务时使用。
    pub fn get_secret(&self, credential_key: &str) -> Result<String, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get_secret(credential_key).map_err(AppError::from)
    }
}

impl crate::ports::reloadable::Reloadable for CredentialService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteCredentialRepository::open(database_path)?;
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

    use super::*;
    use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed_service() -> (tempfile::TempDir, CredentialService) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试", "测试教师").unwrap())
            .unwrap();
        drop(workspace);
        let repository = SqliteCredentialRepository::open(&path).unwrap();
        let service = CredentialService::new(repository, path);
        (directory, service)
    }

    fn sample_draft() -> CredentialDraft {
        CredentialDraft::try_new(
            "Seedance".to_owned(),
            "种子舞蹈".to_owned(),
            "https://api.seedance.com".to_owned(),
            "seedance-v2".to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn creates_and_lists_credentials() {
        let (_dir, service) = seed_service();
        let record = service
            .create(sample_draft(), "sk-test-key".to_owned())
            .unwrap();
        assert_eq!(record.provider_name, "Seedance");

        let list = service.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn rejects_empty_api_key() {
        let (_dir, service) = seed_service();
        let error = service.create(sample_draft(), "  ".to_owned()).unwrap_err();
        assert!(matches!(
            error,
            AppError::CredentialValidation(
                crate::domain::credentials::CredentialValidationError::Required { field: "apiKey" }
            )
        ));
    }
}
