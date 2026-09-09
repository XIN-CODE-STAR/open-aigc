//! 资源账号应用服务。
//!
//! 封装 ResourceAccountRepository，提供 CRUD + 健康检查 + session 刷新能力。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    application::error::AppError,
    ports::resource_account_repository::{
        ResourceAccountDraft, ResourceAccountRepository, ResourceAccountUpdate,
    },
    ports::resource_connector::ResourceAccountRecord,
};

pub struct ResourceAccountService {
    repository: Mutex<Box<dyn ResourceAccountRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl ResourceAccountService {
    pub fn new(
        repository: impl ResourceAccountRepository + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    pub fn list(&self) -> Result<Vec<ResourceAccountRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list().map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<ResourceAccountRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get(id).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn list_by_provider(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ResourceAccountRecord>, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.list_by_provider(provider_id).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn create(
        &self,
        draft: ResourceAccountDraft,
        session_secret: String,
    ) -> Result<ResourceAccountRecord, AppError> {
        if session_secret.trim().is_empty() {
            return Err(AppError::CredentialValidation(
                crate::domain::credentials::CredentialValidationError::Required {
                    field: "sessionCookie",
                },
            ));
        }
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create(draft, session_secret).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn update(
        &self,
        id: &str,
        update: ResourceAccountUpdate,
    ) -> Result<ResourceAccountRecord, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update(id, update).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn update_session(&self, id: &str, session_secret: String) -> Result<(), AppError> {
        if session_secret.trim().is_empty() {
            return Err(AppError::CredentialValidation(
                crate::domain::credentials::CredentialValidationError::Required {
                    field: "sessionCookie",
                },
            ));
        }
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_session(id, session_secret).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })?;
        // Session 更新后自动将状态重置为 active，使其可被 Provider 注册使用。
        repo.update_status(id, "active").map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn update_status(&self, id: &str, status: &str) -> Result<(), AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_status(id, status).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.set_enabled(id, enabled).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.delete(id).map_err(|e| {
            AppError::Provider(crate::domain::providers::ProviderError::ConfigInvalid(
                e.to_string(),
            ))
        })
    }
}
