#![allow(dead_code)]
//! 生成任务轮询器。
//!
//! 定期检查活跃的 generation_attempts，通过 ProviderRegistry 解析对应 Provider，
//! 调用 Provider.poll() 获取远端状态，成功时触发 GenerationPipeline 完成后处理全链路。
//!
//! 事件推送：
//! - `generation://progress` — 进度更新
//! - `generation://completed` — 生成完成（Pipeline 已执行）
//! - `generation://failed` — 生成失败

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::adapters::sqlite::generation_attempt_repository::SqliteGenerationAttemptRepository;
use crate::application::error::AppError;
use crate::application::generation_pipeline::GenerationPipeline;
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::generation::{AttemptStatus, GenerationAttemptRecord};
use crate::ports::generation_attempt_repository::GenerationAttemptRepository;
use crate::ports::unified_provider::{UnifiedPollResult, UnifiedProviderAdapter};

/// 轮询器配置。
#[derive(Debug, Clone)]
pub struct PollWorkerConfig {
    /// 图片类轮询间隔（秒）。
    pub image_interval_secs: u64,
    /// 视频类轮询间隔（秒）。
    pub video_interval_secs: u64,
    /// 最大连续失败次数（超过标记 timed_out）。
    pub max_consecutive_failures: u32,
}

impl Default for PollWorkerConfig {
    fn default() -> Self {
        Self {
            image_interval_secs: 3,
            video_interval_secs: 8,
            max_consecutive_failures: 60,
        }
    }
}

/// 轮询结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    pub attempt_id: String,
    pub old_status: AttemptStatus,
    pub new_status: AttemptStatus,
    pub progress: u8,
    pub result_url: Option<String>,
    pub error_message: Option<String>,
}

/// 后台轮询器。
///
/// 持有 ProviderRegistry 和 GenerationPipeline，能自主完成：
/// 解析 Provider → 轮询远端 → 成功时触发 Pipeline → 推送事件。
pub struct PollWorker {
    attempt_repository: Arc<Mutex<Box<dyn GenerationAttemptRepository>>>,
    provider_registry: Arc<ProviderRegistry>,
    pipeline: Arc<GenerationPipeline>,
    app_handle: Option<AppHandle>,
    config: PollWorkerConfig,
    running: Arc<Mutex<bool>>,
}

impl PollWorker {
    pub fn new(
        attempt_repository: Arc<Mutex<Box<dyn GenerationAttemptRepository>>>,
        provider_registry: Arc<ProviderRegistry>,
        pipeline: Arc<GenerationPipeline>,
        config: PollWorkerConfig,
    ) -> Self {
        Self {
            attempt_repository,
            provider_registry,
            pipeline,
            app_handle: None,
            config,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 设置 AppHandle（用于事件推送）。在 Tauri setup 阶段调用。
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// 启动后台轮询线程。
    pub fn start(&self) -> Result<(), String> {
        let mut running = self.running.lock().map_err(|e| e.to_string())?;
        if *running {
            return Ok(()); // 已在运行
        }
        *running = true;
        drop(running);

        let repo = Arc::clone(&self.attempt_repository);
        let registry = Arc::clone(&self.provider_registry);
        let pipeline = Arc::clone(&self.pipeline);
        let app = self.app_handle.clone();
        let running = Arc::clone(&self.running);
        let config = self.config.clone();

        thread::spawn(move || {
            // 使用图片间隔作为基础循环间隔（最短，保证响应性）
            let interval = Duration::from_secs(config.image_interval_secs);

            loop {
                {
                    let r = running.lock().unwrap_or_else(|e| e.into_inner());
                    if !*r {
                        break;
                    }
                }

                if let Err(e) = Self::poll_once(&repo, &registry, &pipeline, &app, &config) {
                    eprintln!("[PollWorker] poll_once error: {e}");
                }

                thread::sleep(interval);
            }
        });

        Ok(())
    }

    /// 停止轮询。
    pub fn stop(&self) {
        if let Ok(mut running) = self.running.lock() {
            *running = false;
        }
    }

    /// 检查轮询器是否在运行。
    pub fn is_running(&self) -> bool {
        self.running.lock().map(|r| *r).unwrap_or(false)
    }

    /// 执行一次轮询：检查所有活跃任务，解析 Provider，调用 poll。
    fn poll_once(
        repo: &Arc<Mutex<Box<dyn GenerationAttemptRepository>>>,
        registry: &Arc<ProviderRegistry>,
        pipeline: &Arc<GenerationPipeline>,
        app: &Option<AppHandle>,
        config: &PollWorkerConfig,
    ) -> Result<Vec<PollResult>, String> {
        let active = {
            let mut repo = repo.lock().map_err(|e| e.to_string())?;
            repo.list_active().map_err(|e| e.to_string())?
        };

        let mut results = Vec::new();

        for attempt in active {
            // 只轮询 submitted 和 polling 状态
            if attempt.status != AttemptStatus::Submitted
                && attempt.status != AttemptStatus::Polling
            {
                continue;
            }

            // 检查是否到达轮询间隔（基于 capability）
            if !Self::should_poll_now(&attempt, config) {
                continue;
            }

            // 从 Registry 解析 Provider + Credential
            let resolved = match registry.resolve(&attempt.provider_id) {
                Some(r) => r,
                None => {
                    eprintln!(
                        "[PollWorker] No provider registered: '{}' for attempt {}",
                        attempt.provider_id, attempt.id
                    );
                    continue;
                }
            };
            let provider = &resolved.adapter;
            let credential = &resolved.credential;

            // 获取远端任务 ID
            let remote_job_id = match &attempt.remote_job_id {
                Some(id) if !id.is_empty() => id.clone(),
                _ => continue,
            };

            // 执行轮询
            match provider.poll(&remote_job_id, credential) {
                Ok(poll_result) => {
                    let result = Self::handle_poll_result(
                        repo,
                        pipeline,
                        provider.as_ref(),
                        credential,
                        app,
                        &attempt,
                        &poll_result,
                    );
                    results.push(result);
                }
                Err(e) => {
                    eprintln!("[PollWorker] Poll error for {}: {}", attempt.id, e);
                    // 递增连续失败计数
                    let mut repo_guard = repo.lock().map_err(|e| e.to_string())?;
                    let _ = repo_guard.increment_failures(&attempt.id);

                    // 检查是否超过最大失败次数
                    if attempt.consecutive_failures + 1 >= config.max_consecutive_failures {
                        let _ = repo_guard.update_status(
                            &attempt.id,
                            AttemptStatus::TimedOut,
                            Some("POLL_TIMEOUT"),
                            Some(&format!(
                                "Exceeded max consecutive failures ({})",
                                config.max_consecutive_failures
                            )),
                        );
                        emit_event(
                            app,
                            "generation://failed",
                            &serde_json::json!({
                                "attemptId": attempt.id,
                                "taskId": attempt.task_id,
                                "error": "Poll timeout: too many consecutive failures",
                            }),
                        );
                    }
                }
            }
        }

        Ok(results)
    }

    /// 判断是否应该轮询此 attempt（基于 capability 对应的间隔）。
    fn should_poll_now(attempt: &GenerationAttemptRecord, config: &PollWorkerConfig) -> bool {
        let last_poll = match &attempt.last_poll_at {
            Some(ts) => ts,
            None => return true, // 从未轮询过，立即执行
        };

        let interval_secs = if attempt.capability.contains("video") {
            config.video_interval_secs
        } else {
            config.image_interval_secs
        };

        // 解析 last_poll_at 时间，判断是否已过间隔
        if let Ok(last) =
            time::OffsetDateTime::parse(last_poll, &time::format_description::well_known::Rfc3339)
        {
            let now = time::OffsetDateTime::now_utc();
            let elapsed = (now - last).whole_seconds() as u64;
            elapsed >= interval_secs
        } else {
            true // 解析失败则立即轮询
        }
    }

    /// 处理轮询结果：根据状态分发到 Pipeline 或更新进度。
    fn handle_poll_result(
        repo: &Arc<Mutex<Box<dyn GenerationAttemptRepository>>>,
        pipeline: &Arc<GenerationPipeline>,
        provider: &dyn UnifiedProviderAdapter,
        credential: &crate::domain::credentials::CredentialContext,
        app: &Option<AppHandle>,
        attempt: &GenerationAttemptRecord,
        result: &UnifiedPollResult,
    ) -> PollResult {
        match result.status.as_str() {
            "succeeded" => {
                let pipeline_output = if let Some(result_url) = result.result_url.as_deref() {
                    match pipeline.complete_attempt(
                        provider,
                        credential,
                        &attempt.id,
                        &attempt.task_id,
                        result_url,
                        &attempt.task_id,
                        app.as_ref(),
                    ) {
                        Ok(output) => Some(output),
                        Err(error) => {
                            let message = error.to_string();
                            let mut repo_guard = repo.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = repo_guard.update_status(
                                &attempt.id,
                                AttemptStatus::Failed,
                                Some("PIPELINE_FAILED"),
                                Some(&message),
                            );
                            emit_event(
                                app,
                                "generation://failed",
                                &serde_json::json!({
                                    "attemptId": attempt.id,
                                    "taskId": attempt.task_id,
                                    "error": message,
                                }),
                            );
                            return PollResult {
                                attempt_id: attempt.id.clone(),
                                old_status: attempt.status,
                                new_status: AttemptStatus::Failed,
                                progress: attempt.progress,
                                result_url: result.result_url.clone(),
                                error_message: Some(message),
                            };
                        }
                    }
                } else {
                    None
                };

                // 重置失败计数并更新状态
                {
                    let mut repo_guard = repo.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = repo_guard.reset_failures(&attempt.id);
                    if let Some(output) = pipeline_output.as_ref() {
                        let _ = repo_guard.set_result_asset(&attempt.id, &output.asset_id);
                    }
                    let _ =
                        repo_guard.update_status(&attempt.id, AttemptStatus::Succeeded, None, None);
                }

                emit_event(
                    app,
                    "generation://completed",
                    &serde_json::json!({
                        "attemptId": attempt.id,
                        "taskId": attempt.task_id,
                        "resultUrl": result.result_url,
                        "assetId": pipeline_output.as_ref().map(|output| output.asset_id.as_str()),
                        "versionId": pipeline_output.as_ref().map(|output| output.version_id.as_str()),
                        "reviewId": pipeline_output.as_ref().and_then(|output| output.review_id.as_deref()),
                    }),
                );

                PollResult {
                    attempt_id: attempt.id.clone(),
                    old_status: attempt.status,
                    new_status: AttemptStatus::Succeeded,
                    progress: 100,
                    result_url: result.result_url.clone(),
                    error_message: None,
                }
            }
            "failed" => {
                let mut repo_guard = repo.lock().unwrap_or_else(|e| e.into_inner());
                let _ = repo_guard.update_status(
                    &attempt.id,
                    AttemptStatus::Failed,
                    None,
                    result.error_message.as_deref(),
                );

                emit_event(
                    app,
                    "generation://failed",
                    &serde_json::json!({
                        "attemptId": attempt.id,
                        "taskId": attempt.task_id,
                        "error": result.error_message,
                    }),
                );

                PollResult {
                    attempt_id: attempt.id.clone(),
                    old_status: attempt.status,
                    new_status: AttemptStatus::Failed,
                    progress: attempt.progress,
                    result_url: None,
                    error_message: result.error_message.clone(),
                }
            }
            _ => {
                // processing / pending：更新进度和状态
                let new_status = if attempt.status == AttemptStatus::Submitted {
                    AttemptStatus::Polling
                } else {
                    attempt.status
                };

                {
                    let mut repo_guard = repo.lock().unwrap_or_else(|e| e.into_inner());
                    if result.progress != attempt.progress {
                        let _ = repo_guard.update_progress(&attempt.id, result.progress);
                    }
                    if new_status != attempt.status {
                        let _ = repo_guard.update_status(&attempt.id, new_status, None, None);
                    }
                    let _ = repo_guard.reset_failures(&attempt.id);
                }

                emit_event(
                    app,
                    "generation://progress",
                    &serde_json::json!({
                        "attemptId": attempt.id,
                        "taskId": attempt.task_id,
                        "progress": result.progress,
                    }),
                );

                PollResult {
                    attempt_id: attempt.id.clone(),
                    old_status: attempt.status,
                    new_status,
                    progress: result.progress,
                    result_url: None,
                    error_message: None,
                }
            }
        }
    }
}

impl crate::ports::reloadable::Reloadable for PollWorker {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteGenerationAttemptRepository::open(database_path)?;
        let mut repository = self
            .attempt_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repository = Box::new(new_repository);
        Ok(())
    }
}

impl Drop for PollWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 推送 Tauri 事件（静默失败）。
fn emit_event(app: &Option<AppHandle>, event: &str, payload: &serde_json::Value) {
    if let Some(handle) = app {
        let _ = handle.emit(event, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::generation::{AttemptStatus, GenerationAttemptDraft};
    use crate::domain::providers::ProviderError;
    use crate::ports::generation_attempt_repository::{
        AttemptRepositoryError, GenerationAttemptRepository,
    };
    use crate::ports::unified_provider::{
        CapabilityKind, UnifiedDownloadResult, UnifiedHealthStatus, UnifiedProviderAdapter,
        UnifiedRequest, UnifiedSubmitResult,
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock Provider：可控制 poll 返回结果。
    struct MockPollProvider {
        poll_count: AtomicU32,
        succeed_after: u32,
    }

    impl MockPollProvider {
        fn new(succeed_after: u32) -> Self {
            Self {
                poll_count: AtomicU32::new(0),
                succeed_after,
            }
        }
    }

    impl UnifiedProviderAdapter for MockPollProvider {
        fn provider_id(&self) -> &str {
            "mock-poll"
        }
        fn capabilities(&self) -> Vec<CapabilityKind> {
            vec![CapabilityKind::TextToImage]
        }
        fn submit(
            &self,
            _req: &UnifiedRequest,
            _credential: &crate::domain::credentials::CredentialContext,
        ) -> Result<UnifiedSubmitResult, ProviderError> {
            Ok(UnifiedSubmitResult {
                remote_job_id: "job-001".into(),
                initial_status: "pending".into(),
                immediate_result_url: None,
                estimated_duration_secs: Some(5),
            })
        }
        fn poll(
            &self,
            _id: &str,
            _credential: &crate::domain::credentials::CredentialContext,
        ) -> Result<UnifiedPollResult, ProviderError> {
            let count = self.poll_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count >= self.succeed_after {
                Ok(UnifiedPollResult {
                    status: "succeeded".into(),
                    progress: 100,
                    result_url: Some("https://example.com/result.png".into()),
                    error_message: None,
                    retryable: false,
                })
            } else {
                Ok(UnifiedPollResult {
                    status: "processing".into(),
                    progress: (count * 30).min(90) as u8,
                    result_url: None,
                    error_message: None,
                    retryable: false,
                })
            }
        }
        fn download(
            &self,
            _url: &str,
            _dir: &Path,
            _credential: &crate::domain::credentials::CredentialContext,
        ) -> Result<UnifiedDownloadResult, ProviderError> {
            std::fs::create_dir_all(_dir).map_err(|e| ProviderError::Network(e.to_string()))?;
            let file_path = _dir.join("mock-result.png");
            std::fs::write(&file_path, b"mock image")
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            Ok(UnifiedDownloadResult {
                file_path: file_path.to_string_lossy().to_string(),
                mime_type: "image/png".into(),
                file_size: 10,
                duration_secs: None,
                width: Some(1024),
                height: Some(1024),
            })
        }
        fn health_check(
            &self,
            _credential: &crate::domain::credentials::CredentialContext,
        ) -> Result<UnifiedHealthStatus, ProviderError> {
            Ok(UnifiedHealthStatus {
                available: true,
                message: "ok".into(),
                quota_remaining: None,
            })
        }
    }

    /// 内存 Attempt Repository 用于测试。
    struct InMemoryAttemptRepo {
        attempts: Vec<GenerationAttemptRecord>,
    }

    impl InMemoryAttemptRepo {
        fn new() -> Self {
            Self {
                attempts: Vec::new(),
            }
        }
    }

    impl GenerationAttemptRepository for InMemoryAttemptRepo {
        fn create(
            &mut self,
            draft: GenerationAttemptDraft,
        ) -> Result<GenerationAttemptRecord, AttemptRepositoryError> {
            let record = GenerationAttemptRecord {
                id: format!("attempt-{}", self.attempts.len() + 1),
                task_id: draft.task_id,
                credential_id: draft.credential_id,
                capability: draft.capability,
                request_snapshot_json: draft.request_snapshot_json,
                remote_job_id: None,
                status: AttemptStatus::Pending,
                progress: 0,
                error_code: None,
                error_message: None,
                result_asset_id: None,
                provider_id: draft.provider_id,
                consecutive_failures: 0,
                last_poll_at: None,
                started_at: None,
                finished_at: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            };
            self.attempts.push(record.clone());
            Ok(record)
        }
        fn get(
            &mut self,
            id: &str,
        ) -> Result<Option<GenerationAttemptRecord>, AttemptRepositoryError> {
            Ok(self.attempts.iter().find(|a| a.id == id).cloned())
        }
        fn list_by_task(
            &mut self,
            task_id: &str,
        ) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError> {
            Ok(self
                .attempts
                .iter()
                .filter(|a| a.task_id == task_id)
                .cloned()
                .collect())
        }
        fn update_status(
            &mut self,
            id: &str,
            status: AttemptStatus,
            error_code: Option<&str>,
            error_message: Option<&str>,
        ) -> Result<GenerationAttemptRecord, AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.status = status;
                a.error_code = error_code.map(|s| s.to_owned());
                a.error_message = error_message.map(|s| s.to_owned());
                return Ok(a.clone());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn set_remote_job_id(
            &mut self,
            id: &str,
            remote_job_id: &str,
        ) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.remote_job_id = Some(remote_job_id.to_owned());
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn update_progress(
            &mut self,
            id: &str,
            progress: u8,
        ) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.progress = progress;
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn set_result_asset(
            &mut self,
            id: &str,
            asset_id: &str,
        ) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.result_asset_id = Some(asset_id.to_owned());
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn list_active(&mut self) -> Result<Vec<GenerationAttemptRecord>, AttemptRepositoryError> {
            Ok(self
                .attempts
                .iter()
                .filter(|a| {
                    a.status == AttemptStatus::Submitted || a.status == AttemptStatus::Polling
                })
                .cloned()
                .collect())
        }
        fn increment_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.consecutive_failures += 1;
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn reset_failures(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.consecutive_failures = 0;
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn set_provider_id(
            &mut self,
            id: &str,
            provider_id: &str,
        ) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.provider_id = provider_id.to_owned();
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
        fn update_last_poll(&mut self, id: &str) -> Result<(), AttemptRepositoryError> {
            if let Some(a) = self.attempts.iter_mut().find(|a| a.id == id) {
                a.last_poll_at = Some("2026-01-01T00:00:01Z".into());
                return Ok(());
            }
            Err(AttemptRepositoryError::NotFound(id.into()))
        }
    }

    /// 内存 ReviewRepository 用于测试。
    struct InMemoryReviewRepo;

    impl InMemoryReviewRepo {
        fn new() -> Self {
            Self
        }
    }

    impl crate::ports::review_repository::ReviewRepository for InMemoryReviewRepo {
        fn insert_report(
            &mut self,
            _: &crate::domain::review::ReviewReportRecord,
        ) -> Result<
            crate::domain::review::ReviewReportRecord,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            todo!("mock")
        }
        fn get_report(
            &mut self,
            _: &str,
        ) -> Result<
            Option<crate::domain::review::ReviewReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(None)
        }
        fn list_reports_by_project(
            &mut self,
            _: &str,
            _: Option<crate::domain::review::ReviewDecision>,
            _: i64,
        ) -> Result<
            Vec<crate::domain::review::ReviewReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn list_reports_by_shot(
            &mut self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::review::ReviewReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn list_reports_by_asset(
            &mut self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::review::ReviewReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn list_reports_by_attempt(
            &mut self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::review::ReviewReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn insert_dimensions(
            &mut self,
            _: &[crate::domain::review::ReviewDimensionRecord],
        ) -> Result<(), crate::ports::review_repository::ReviewRepositoryError> {
            Ok(())
        }
        fn list_dimensions_by_report(
            &mut self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::review::ReviewDimensionRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn insert_asset_version(
            &mut self,
            version: &crate::domain::review::AssetVersionRecord,
        ) -> Result<
            crate::domain::review::AssetVersionRecord,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(version.clone())
        }
        fn list_asset_versions(
            &mut self,
            _: &str,
            _: i64,
        ) -> Result<
            Vec<crate::domain::review::AssetVersionRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn update_asset_version_status(
            &mut self,
            _: &str,
            _: crate::domain::review::AssetVersionStatus,
            _: Option<&str>,
        ) -> Result<
            crate::domain::review::AssetVersionRecord,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            todo!("mock")
        }
        fn upsert_asset_license(
            &mut self,
            _: &crate::domain::review::AssetLicenseRecord,
        ) -> Result<
            crate::domain::review::AssetLicenseRecord,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            todo!("mock")
        }
        fn get_asset_license(
            &mut self,
            _: &str,
        ) -> Result<
            Option<crate::domain::review::AssetLicenseRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(None)
        }
        fn list_pending_licenses(
            &mut self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::review::AssetLicenseRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
        fn insert_content_guard_report(
            &mut self,
            _: &crate::domain::review::ContentGuardReportRecord,
        ) -> Result<
            crate::domain::review::ContentGuardReportRecord,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            todo!("mock")
        }
        fn get_latest_guard_report(
            &mut self,
            _: &str,
            _: &str,
        ) -> Result<
            Option<crate::domain::review::ContentGuardReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(None)
        }
        fn list_guard_reports_by_project(
            &mut self,
            _: &str,
            _: i64,
        ) -> Result<
            Vec<crate::domain::review::ContentGuardReportRecord>,
            crate::ports::review_repository::ReviewRepositoryError,
        > {
            Ok(vec![])
        }
    }

    struct InMemoryAssetRepo {
        assets: Vec<crate::domain::assets::AssetRecord>,
    }

    impl InMemoryAssetRepo {
        fn new() -> Self {
            Self { assets: Vec::new() }
        }
    }

    impl crate::ports::asset_repository::AssetRepository for InMemoryAssetRepo {
        fn list(
            &mut self,
            _: &crate::domain::assets::AssetFilter,
        ) -> Result<
            Vec<crate::domain::assets::AssetRecord>,
            crate::ports::persistence::PersistenceError,
        > {
            Ok(self.assets.clone())
        }

        fn get(
            &mut self,
            asset_id: &str,
        ) -> Result<
            Option<crate::domain::assets::AssetRecord>,
            crate::ports::persistence::PersistenceError,
        > {
            Ok(self
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
                .cloned())
        }

        fn import(
            &mut self,
            source_paths: Vec<String>,
            namespace: &str,
            _: crate::ports::asset_repository::ImportOptions,
        ) -> Result<
            crate::domain::assets::AssetImportSummary,
            crate::ports::asset_repository::AssetRepositoryError,
        > {
            let mut imported = Vec::new();
            let mut failures = Vec::new();

            for source_path in source_paths {
                let path = std::path::PathBuf::from(&source_path);
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        let file_name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("mock-result.png")
                            .to_owned();
                        let asset = crate::domain::assets::AssetRecord {
                            id: uuid::Uuid::new_v4().to_string(),
                            storage_namespace: crate::domain::assets::StorageNamespace::parse(
                                namespace,
                            )
                            .unwrap_or(crate::domain::assets::StorageNamespace::Generation),
                            asset_kind: crate::domain::assets::AssetKind::infer(
                                Some("image/png"),
                                &file_name,
                            ),
                            display_name: file_name.clone(),
                            relative_path: format!("assets/{file_name}"),
                            size_bytes: metadata.len() as i64,
                            sha256: None,
                            mime_type: Some("image/png".to_owned()),
                            integrity_status: crate::domain::assets::IntegrityStatus::Valid,
                            metadata_json: "{}".to_owned(),
                            origin_device_id: None,
                            revision: 1,
                            created_at: "2026-01-01T00:00:00Z".into(),
                            updated_at: "2026-01-01T00:00:00Z".into(),
                        };
                        self.assets.push(asset.clone());
                        imported
                            .push(crate::domain::assets::AssetImportOutcome { asset, source_path });
                    }
                    Err(error) => failures.push(crate::domain::assets::AssetImportFailure {
                        source_path,
                        reason: error.to_string(),
                    }),
                }
            }

            Ok(crate::domain::assets::AssetImportSummary {
                imported,
                skipped: Vec::new(),
                failures,
            })
        }

        fn reverify(
            &mut self,
        ) -> Result<
            crate::domain::assets::AssetReverificationSummary,
            crate::ports::asset_repository::AssetRepositoryError,
        > {
            Ok(crate::domain::assets::AssetReverificationSummary::empty())
        }

        fn delete(
            &mut self,
            asset_id: &str,
        ) -> Result<(), crate::ports::asset_repository::AssetRepositoryError> {
            self.assets.retain(|asset| asset.id != asset_id);
            Ok(())
        }
    }

    fn setup_worker(
        succeed_after: u32,
    ) -> (PollWorker, Arc<Mutex<Box<dyn GenerationAttemptRepository>>>) {
        let repo: Arc<Mutex<Box<dyn GenerationAttemptRepository>>> =
            Arc::new(Mutex::new(Box::new(InMemoryAttemptRepo::new())));
        let registry = Arc::new(ProviderRegistry::new());
        registry.register(Arc::new(MockPollProvider::new(succeed_after)));

        let temp_dir = tempfile::tempdir().unwrap();
        let download_dir = temp_dir.path().join("downloads");
        std::fs::create_dir_all(&download_dir).unwrap();

        let pipeline = Arc::new(GenerationPipeline::new(
            download_dir,
            InMemoryReviewRepo::new(),
            InMemoryAssetRepo::new(),
        ));

        let worker = PollWorker::new(
            Arc::clone(&repo),
            registry,
            pipeline,
            PollWorkerConfig {
                image_interval_secs: 1,
                video_interval_secs: 2,
                max_consecutive_failures: 5,
            },
        );
        (worker, repo)
    }

    #[test]
    fn poll_once_resolves_provider_and_polls() {
        let (worker, repo) = setup_worker(1); // 第一次 poll 就成功

        // 创建一个 submitted 状态的 attempt
        {
            let mut r = repo.lock().unwrap_or_else(|e| e.into_inner());
            let draft = GenerationAttemptDraft {
                task_id: "task-1".into(),
                credential_id: "cred-1".into(),
                capability: "text_to_image".into(),
                request_snapshot_json: "{}".into(),
                provider_id: "mock-poll".into(),
            };
            let record = r.create(draft).unwrap();
            r.set_remote_job_id(&record.id, "job-001").unwrap();
            r.update_status(&record.id, AttemptStatus::Submitted, None, None)
                .unwrap();
        }

        // 执行一次轮询
        let registry = Arc::clone(&worker.provider_registry);
        let pipeline = Arc::clone(&worker.pipeline);
        let config = worker.config.clone();
        let results = PollWorker::poll_once(&repo, &registry, &pipeline, &None, &config).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].new_status, AttemptStatus::Succeeded);
        assert_eq!(results[0].progress, 100);

        // 验证 DB 状态已更新
        let mut r = repo.lock().unwrap_or_else(|e| e.into_inner());
        let attempt = r.get("attempt-1").unwrap().unwrap();
        assert_eq!(attempt.status, AttemptStatus::Succeeded);
    }

    #[test]
    fn poll_once_skips_unknown_provider() {
        let (worker, repo) = setup_worker(1);

        // 创建一个 provider_id 不存在的 attempt
        {
            let mut r = repo.lock().unwrap_or_else(|e| e.into_inner());
            let draft = GenerationAttemptDraft {
                task_id: "task-1".into(),
                credential_id: "cred-1".into(),
                capability: "text_to_image".into(),
                request_snapshot_json: "{}".into(),
                provider_id: "nonexistent-provider".into(),
            };
            let record = r.create(draft).unwrap();
            r.set_remote_job_id(&record.id, "job-001").unwrap();
            r.update_status(&record.id, AttemptStatus::Submitted, None, None)
                .unwrap();
        }

        let registry = Arc::clone(&worker.provider_registry);
        let pipeline = Arc::clone(&worker.pipeline);
        let config = worker.config.clone();
        let results = PollWorker::poll_once(&repo, &registry, &pipeline, &None, &config).unwrap();

        // 应该跳过，无结果
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn poll_once_handles_processing_status() {
        let (worker, repo) = setup_worker(3); // 第 3 次才成功

        {
            let mut r = repo.lock().unwrap_or_else(|e| e.into_inner());
            let draft = GenerationAttemptDraft {
                task_id: "task-1".into(),
                credential_id: "cred-1".into(),
                capability: "text_to_image".into(),
                request_snapshot_json: "{}".into(),
                provider_id: "mock-poll".into(),
            };
            let record = r.create(draft).unwrap();
            r.set_remote_job_id(&record.id, "job-001").unwrap();
            r.update_status(&record.id, AttemptStatus::Submitted, None, None)
                .unwrap();
        }

        let registry = Arc::clone(&worker.provider_registry);
        let pipeline = Arc::clone(&worker.pipeline);
        let config = worker.config.clone();

        // 第一次轮询：processing
        let results = PollWorker::poll_once(&repo, &registry, &pipeline, &None, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].new_status, AttemptStatus::Polling);
        assert_eq!(results[0].progress, 30);

        // 第二次轮询：仍然 processing
        let results = PollWorker::poll_once(&repo, &registry, &pipeline, &None, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].progress, 60);

        // 第三次轮询：succeeded
        let results = PollWorker::poll_once(&repo, &registry, &pipeline, &None, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].new_status, AttemptStatus::Succeeded);
    }

    #[test]
    fn should_poll_now_respects_interval() {
        let config = PollWorkerConfig {
            image_interval_secs: 3,
            video_interval_secs: 8,
            max_consecutive_failures: 60,
        };

        // 从未轮询过 → 应该轮询
        let attempt = GenerationAttemptRecord {
            id: "a1".into(),
            task_id: "t1".into(),
            credential_id: "c1".into(),
            capability: "text_to_image".into(),
            request_snapshot_json: "{}".into(),
            remote_job_id: Some("job-1".into()),
            status: AttemptStatus::Submitted,
            progress: 0,
            error_code: None,
            error_message: None,
            result_asset_id: None,
            provider_id: "mock".into(),
            consecutive_failures: 0,
            last_poll_at: None,
            started_at: None,
            finished_at: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        assert!(PollWorker::should_poll_now(&attempt, &config));

        // 刚刚轮询过（未来时间）→ 不应该轮询
        let mut recent = attempt.clone();
        recent.last_poll_at = Some(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
        );
        assert!(!PollWorker::should_poll_now(&recent, &config));
    }
}
