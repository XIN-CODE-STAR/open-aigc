use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};

use crate::{
    adapters::sqlite::credential_repository::SqliteCredentialRepository,
    application::error::AppError,
    domain::providers::{GenerationRequest, ProviderPollResult},
    ports::{
        credential_repository::{CredentialRepository, CredentialRepositoryError},
        provider_adapter::ProviderAdapter,
    },
};

/// Provider 服务负责调用 AI 供应商 API 并通过 Tauri event 推送进度。
pub struct ProviderService {
    credential_repository: Mutex<Box<dyn CredentialRepository>>,
    #[allow(dead_code)]
    provider_adapter: Mutex<Box<dyn ProviderAdapter>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl ProviderService {
    pub fn new(
        credential_repository: impl CredentialRepository + 'static,
        provider_adapter: impl ProviderAdapter + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            credential_repository: Mutex::new(Box::new(credential_repository)),
            provider_adapter: Mutex::new(Box::new(provider_adapter)),
            database_path,
        }
    }

    /// 提交生成请求到 Provider，返回远程任务 ID。
    #[allow(dead_code)]
    pub fn submit_generation(
        &self,
        credential_id: &str,
        prompt: &str,
        parameters: serde_json::Value,
    ) -> Result<String, AppError> {
        let credential = self.get_credential(credential_id)?;
        let api_key = self.get_secret(&credential.credential_key)?;
        let request = GenerationRequest {
            prompt: prompt.to_owned(),
            model: credential.model_name.clone(),
            parameters,
            reference_assets: Vec::new(),
        };
        let mut adapter = self
            .provider_adapter
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let result = adapter
            .submit(
                &credential.base_url,
                &api_key,
                &credential.model_name,
                &request,
            )
            .map_err(AppError::from)?;
        Ok(result.remote_task_id)
    }

    /// 轮询远程任务进度并通过 Tauri event 推送给前端。
    /// 在前端通过 `listen('generation://progress', ...)` 接收。
    #[allow(dead_code)]
    pub fn poll_and_emit(
        &self,
        app: &AppHandle,
        credential_id: &str,
        task_id: &str,
        remote_task_id: &str,
    ) -> Result<ProviderPollResult, AppError> {
        let credential = self.get_credential(credential_id)?;
        let api_key = self.get_secret(&credential.credential_key)?;
        let mut adapter = self
            .provider_adapter
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        let poll_result = adapter
            .poll(&credential.base_url, &api_key, remote_task_id)
            .map_err(AppError::from)?;

        // 通过 Tauri event 推送进度。
        let payload = serde_json::json!({
            "taskId": task_id,
            "status": poll_result.status,
            "progress": poll_result.progress,
            "resultUrl": poll_result.result_url,
            "errorMessage": poll_result.error_message,
        });
        let _ = app.emit("generation://progress", payload);

        Ok(poll_result)
    }

    /// 下载结果文件到本地临时路径。
    #[allow(dead_code)]
    pub fn download_result(
        &self,
        credential_id: &str,
        result_url: &str,
        local_path: &Path,
    ) -> Result<(), AppError> {
        let credential = self.get_credential(credential_id)?;
        let api_key = self.get_secret(&credential.credential_key)?;
        let mut adapter = self
            .provider_adapter
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        adapter
            .download_result(&credential.base_url, &api_key, result_url, local_path)
            .map_err(AppError::from)
    }

    #[allow(dead_code)]
    fn get_credential(
        &self,
        credential_id: &str,
    ) -> Result<crate::domain::credentials::CredentialRecord, AppError> {
        let mut repo = self
            .credential_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get(credential_id)?.ok_or_else(|| {
            AppError::from(CredentialRepositoryError::NotFound(
                credential_id.to_owned(),
            ))
        })
    }

    #[allow(dead_code)]
    fn get_secret(&self, credential_key: &str) -> Result<String, AppError> {
        let mut repo = self
            .credential_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.get_secret(credential_key).map_err(AppError::from)
    }
}

impl crate::ports::reloadable::Reloadable for ProviderService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteCredentialRepository::open(database_path)?;
        let mut repository = self
            .credential_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repository = Box::new(new_repository);
        Ok(())
    }
}
