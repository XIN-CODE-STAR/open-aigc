//! Generation Submit Service：提交生成请求并调用 Provider。
//!
//! 职责：
//! 1. 创建 Attempt 记录
//! 2. 从 ProviderRegistry 解析 Provider
//! 3. 调用 Provider.submit()
//! 4. 更新 remote_job_id 和状态
//! 5. 同步 Provider（如 Grok）直接触发 Pipeline

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::{
    adapters::sqlite::generation_attempt_repository::SqliteGenerationAttemptRepository,
    application::{
        error::AppError, generation_pipeline::GenerationPipeline,
        provider_registry::ProviderRegistry,
    },
    domain::generation::{AttemptStatus, GenerationAttemptDraft, GenerationAttemptRecord},
    ports::{
        generation_attempt_repository::GenerationAttemptRepository,
        unified_provider::{CapabilityKind, UnifiedRequest},
    },
};

/// 生成提交服务。
pub struct GenerationSubmitService {
    attempt_repository: Mutex<Box<dyn GenerationAttemptRepository>>,
    provider_registry: Arc<ProviderRegistry>,
    pipeline: Arc<GenerationPipeline>,
    database_path: PathBuf,
}

impl GenerationSubmitService {
    pub fn new(
        attempt_repository: impl GenerationAttemptRepository + 'static,
        provider_registry: Arc<ProviderRegistry>,
        pipeline: Arc<GenerationPipeline>,
        database_path: PathBuf,
    ) -> Result<Self, AppError> {
        Ok(Self {
            attempt_repository: Mutex::new(Box::new(attempt_repository)),
            provider_registry,
            pipeline,
            database_path,
        })
    }

    /// 提交生成请求：创建 Attempt → 调用 Provider.submit() → 更新 remote_job_id。
    /// `app` 用于向前端推送进度事件（同步 Provider 路径）。
    pub fn submit_and_dispatch(
        &self,
        task_id: &str,
        credential_id: &str,
        capability: &str,
        request_snapshot_json: &str,
        provider_id: &str,
        app: Option<&AppHandle>,
    ) -> Result<GenerationAttemptRecord, AppError> {
        // 1. 从 Registry 解析 Provider + Credential。provider_id 可能来自凭据展示名，因此走别名解析。
        let resolved = self
            .provider_registry
            .resolve_by_alias(provider_id)
            .ok_or_else(|| {
                AppError::new(
                    "provider not found",
                    std::io::Error::other(format!("Provider '{provider_id}' not registered")),
                )
            })?;
        let provider = &resolved.adapter;
        let credential = &resolved.credential;
        let adapter_provider_id = provider.provider_id().to_owned();

        // 2. 创建 Attempt 记录，持久化稳定 adapter id，方便 PollWorker 后续解析。
        let draft = GenerationAttemptDraft {
            task_id: task_id.to_owned(),
            credential_id: credential_id.to_owned(),
            capability: capability.to_owned(),
            request_snapshot_json: request_snapshot_json.to_owned(),
            provider_id: adapter_provider_id,
        };

        let attempt = self.with_repo(|repo| repo.create(draft).map_err(AppError::from))?;

        // 3. 构建 UnifiedRequest
        let request = build_unified_request(capability, request_snapshot_json)?;

        // 4. 调用 Provider.submit()
        let submit_result = match provider.submit(&request, credential) {
            Ok(result) => result,
            Err(error) => {
                let message = error.to_string();
                let _ = self.with_repo(|repo| {
                    repo.update_status(
                        &attempt.id,
                        AttemptStatus::Failed,
                        Some("PROVIDER_SUBMIT_FAILED"),
                        Some(&message),
                    )
                    .map_err(AppError::from)
                });
                return Err(AppError::new("provider submit failed", error));
            }
        };

        // 5. 更新 remote_job_id 和状态
        let mut latest = self.with_repo(|repo| {
            repo.set_remote_job_id(&attempt.id, &submit_result.remote_job_id)
                .map_err(AppError::from)?;
            repo.update_status(&attempt.id, AttemptStatus::Submitted, None, None)
                .map_err(AppError::from)
        })?;

        // 6. 同步 Provider（如 Grok、代理视频生成）直接处理结果
        if let Some(url) = submit_result.immediate_result_url {
            eprintln!(
                "[Submit] Sync provider result for attempt {}: {}",
                attempt.id, url
            );
            // 发送进度事件：生成完成，开始处理
            if let Some(app_handle) = app {
                use tauri::Emitter;
                let _ = app_handle.emit(
                    "generation://progress",
                    serde_json::json!({
                        "attemptId": attempt.id,
                        "taskId": attempt.task_id,
                        "progress": 90,
                        "status": "downloading",
                        "message": "视频生成完成，正在下载..."
                    }),
                );
            }
            let output = self.pipeline.complete_attempt(
                provider.as_ref(),
                credential,
                &attempt.id,
                &attempt.task_id,
                &url,
                &attempt.task_id,
                app,
            )?;
            latest = self.with_repo(|repo| {
                repo.set_result_asset(&attempt.id, &output.asset_id)
                    .map_err(AppError::from)?;
                repo.update_status(&attempt.id, AttemptStatus::Succeeded, None, None)
                    .map_err(AppError::from)
            })?;
            // 更新 task 状态为 succeeded
            let _ = self.update_task_status(&attempt.task_id, "succeeded");
        }

        Ok(latest)
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

    /// 更新 generation_tasks 表的状态。
    fn update_task_status(&self, task_id: &str, status: &str) -> Result<(), AppError> {
        let conn = rusqlite::Connection::open(&self.database_path)
            .map_err(|e| AppError::new("open db for task status update", e))?;
        let now = crate::adapters::sqlite::now_rfc3339()?;
        conn.execute(
            "UPDATE generation_tasks SET status = ?1, updated_at = ?2, completed_at = ?2 WHERE id = ?3 AND status != 'succeeded'",
            rusqlite::params![status, now, task_id],
        )
        .map_err(|e| AppError::new("update task status", e))?;
        Ok(())
    }
}

/// 从 request_snapshot_json 构建 UnifiedRequest。
fn build_unified_request(
    capability: &str,
    snapshot_json: &str,
) -> Result<UnifiedRequest, AppError> {
    let params: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(snapshot_json).unwrap_or_default();

    let cap = match capability {
        "text_to_image" | "text-to-image" | "image_generation" => CapabilityKind::TextToImage,
        "image_to_image" | "image-to-image" => CapabilityKind::ImageToImage,
        "text_to_video" | "text-to-video" | "video_generation" => CapabilityKind::TextToVideo,
        "image_to_video" | "image-to-video" => CapabilityKind::ImageToVideo,
        "text_to_speech" | "text-to-speech" => CapabilityKind::TextToSpeech,
        "voice_clone" | "voice-clone" => CapabilityKind::VoiceClone,
        other => {
            return Err(AppError::new(
                "build unified request",
                format!("unsupported generation capability: {other}"),
            ));
        }
    };

    Ok(UnifiedRequest {
        capability: cap,
        model: params
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_owned(),
        prompt: params
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        negative_prompt: params
            .get("negative_prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        reference_image_path: params
            .get("reference_image_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        reference_image_url: params
            .get("reference_image_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        parameters: params,
    })
}

impl crate::ports::reloadable::Reloadable for GenerationSubmitService {
    fn reload(&self, database_path: &std::path::Path) -> Result<(), AppError> {
        let new_repo = SqliteGenerationAttemptRepository::open(database_path)?;
        let mut repo = self
            .attempt_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repo = Box::new(new_repo);
        Ok(())
    }
}
