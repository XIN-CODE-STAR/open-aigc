#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    application::error::AppError,
    domain::generation::{
        AttemptStatus, GenerationAttemptDraft, GenerationAttemptRecord, GenerationErrorKind,
    },
    ports::generation_attempt_repository::GenerationAttemptRepository,
};

/// 生成任务队列服务：管理每次 Provider 调用的生命周期。
///
/// 流程：submit → submitted → polling → downloading → succeeded/failed
/// 支持：重试、取消、超时检测、远端对账。
pub struct GenerationQueueService {
    attempt_repository: Mutex<Box<dyn GenerationAttemptRepository>>,
    database_path: PathBuf,
}

impl GenerationQueueService {
    pub fn new(
        attempt_repository: impl GenerationAttemptRepository + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            attempt_repository: Mutex::new(Box::new(attempt_repository)),
            database_path,
        }
    }

    /// 创建一个新的生成尝试（状态：pending）。
    pub fn submit_attempt(
        &self,
        task_id: &str,
        credential_id: &str,
        capability: &str,
        request_snapshot_json: &str,
        provider_id: &str,
    ) -> Result<GenerationAttemptRecord, AppError> {
        let draft = GenerationAttemptDraft {
            task_id: task_id.to_owned(),
            credential_id: credential_id.to_owned(),
            capability: capability.to_owned(),
            request_snapshot_json: request_snapshot_json.to_owned(),
            provider_id: provider_id.to_owned(),
        };
        self.with_repo(|repo| repo.create(draft).map_err(AppError::from))
    }

    /// 标记尝试为 submitted（已提交到 Provider）。
    pub fn mark_submitted(&self, id: &str) -> Result<GenerationAttemptRecord, AppError> {
        self.with_repo(|repo| {
            repo.update_status(id, AttemptStatus::Submitted, None, None)
                .map_err(AppError::from)
        })
    }

    /// 设置远端任务 ID。
    pub fn set_remote_job(&self, id: &str, remote_job_id: &str) -> Result<(), AppError> {
        self.with_repo(|repo| {
            repo.set_remote_job_id(id, remote_job_id)
                .map_err(AppError::from)
        })
    }

    /// 更新进度。
    pub fn update_progress(&self, id: &str, progress: u8) -> Result<(), AppError> {
        self.with_repo(|repo| repo.update_progress(id, progress).map_err(AppError::from))
    }

    /// 标记为 polling 状态。
    pub fn mark_polling(&self, id: &str) -> Result<GenerationAttemptRecord, AppError> {
        self.with_repo(|repo| {
            repo.update_status(id, AttemptStatus::Polling, None, None)
                .map_err(AppError::from)
        })
    }

    /// 标记成功并关联资产。
    pub fn mark_succeeded(
        &self,
        id: &str,
        asset_id: &str,
    ) -> Result<GenerationAttemptRecord, AppError> {
        self.with_repo(|repo| {
            repo.set_result_asset(id, asset_id)?;
            repo.update_status(id, AttemptStatus::Succeeded, None, None)
                .map_err(AppError::from)
        })
    }

    /// 标记失败。
    pub fn mark_failed(
        &self,
        id: &str,
        error_kind: GenerationErrorKind,
        message: &str,
    ) -> Result<GenerationAttemptRecord, AppError> {
        let error_code = format!("{:?}", error_kind);
        self.with_repo(|repo| {
            repo.update_status(id, AttemptStatus::Failed, Some(&error_code), Some(message))
                .map_err(AppError::from)
        })
    }

    /// 取消尝试。
    pub fn cancel(&self, id: &str) -> Result<GenerationAttemptRecord, AppError> {
        self.with_repo(|repo| {
            repo.update_status(id, AttemptStatus::Cancelled, None, None)
                .map_err(AppError::from)
        })
    }

    /// 重试一个失败的尝试（创建新的尝试，继承原始请求）。
    pub fn retry(&self, failed_id: &str) -> Result<GenerationAttemptRecord, AppError> {
        let failed = self.with_repo(|repo| repo.get(failed_id).map_err(AppError::from))?;
        let failed = failed.ok_or_else(|| {
            AppError::GenerationValidation(
                crate::domain::generation::GenerationValidationError::Required {
                    field: "attemptId",
                },
            )
        })?;

        // 创建新的尝试，继承原始 task_id, credential_id, capability, request_snapshot, provider_id
        let draft = GenerationAttemptDraft {
            task_id: failed.task_id,
            credential_id: failed.credential_id,
            capability: failed.capability,
            request_snapshot_json: failed.request_snapshot_json,
            provider_id: failed.provider_id,
        };
        self.with_repo(|repo| repo.create(draft).map_err(AppError::from))
    }

    /// 获取尝试记录。
    pub fn get_attempt(&self, id: &str) -> Result<Option<GenerationAttemptRecord>, AppError> {
        self.with_repo(|repo| repo.get(id).map_err(AppError::from))
    }

    /// 列出某个任务的所有尝试。
    pub fn list_by_task(&self, task_id: &str) -> Result<Vec<GenerationAttemptRecord>, AppError> {
        self.with_repo(|repo| repo.list_by_task(task_id).map_err(AppError::from))
    }

    /// 列出所有活跃尝试（用于轮询和对账）。
    pub fn list_active(&self) -> Result<Vec<GenerationAttemptRecord>, AppError> {
        self.with_repo(|repo| repo.list_active().map_err(AppError::from))
    }

    fn with_repo<T>(
        &self,
        op: impl FnOnce(&mut dyn GenerationAttemptRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .attempt_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        op(repo.as_mut())
    }
}
