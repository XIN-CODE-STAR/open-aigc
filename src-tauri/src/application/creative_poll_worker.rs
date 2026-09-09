#![allow(dead_code)]
//! Creative Poll Worker：Creative Runtime 专用轮询器。
//!
//! 接收 Step 3C-1 提交的 PendingSubmission 列表，通过 ProviderRegistry
//! 轮询远端状态，完成后下载到本地工作区。
//!
//! 与现有 poll_worker 的区别：
//! - 不依赖 SQLite（Step 4 接入持久化）
//! - 不发射 Tauri 事件（未接线前端）
//! - 同步阻塞轮询（无后台线程）
//! - 操作 PendingSubmission 结构体（非 GenerationAttemptRecord）
//!
//! 职责链位置：
//! SequentialExecutor（提交）→ CreativePollWorker（轮询 + 下载）→ Artifact 更新

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::application::provider_registry::ProviderRegistry;
use crate::domain::creative_plan::AssetType;
use crate::ports::unified_provider::UnifiedPollResult;

// ─── PendingSubmission ───

/// 待轮询的提交记录（由 Step 3C-1 Facade.submit() 产生）。
#[derive(Debug, Clone)]
pub struct PendingSubmission {
    /// 远端 job ID（Provider.submit 返回的 remote_job_id）。
    pub submission_id: String,
    /// Provider 标识（用于从 ProviderRegistry 解析适配器）。
    pub provider_id: String,
    /// 关联的执行步骤索引。
    pub step_index: u8,
    /// 关联的镜头索引。
    pub shot_index: u8,
    /// 资产类型（用于确定轮询间隔）。
    pub asset_type: AssetType,
    /// Router 选中的模型名。
    pub model: String,
}

// ─── PollOutcome ───

/// 单次轮询结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// 远端仍在处理。
    StillRunning { progress: u8 },
    /// 完成，返回结果 URL。
    Completed { result_url: String },
    /// 失败。
    Failed { reason: String, retryable: bool },
}

// ─── DownloadedFile ───

/// 下载完成的文件信息。
#[derive(Debug, Clone)]
pub struct DownloadedFile {
    /// 本地文件路径。
    pub file_path: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 视频/音频时长（秒）。
    pub duration_secs: Option<f64>,
}

// ─── SubmissionResult ───

/// 单个提交的最终结果（轮询 + 下载后）。
#[derive(Debug, Clone)]
pub enum SubmissionResult {
    /// 成功：下载完成。
    Success {
        submission: PendingSubmission,
        file: DownloadedFile,
    },
    /// 远端生成失败。
    GenerationFailed {
        submission: PendingSubmission,
        reason: String,
    },
    /// 下载失败。
    DownloadFailed {
        submission: PendingSubmission,
        reason: String,
    },
    /// 超时（远端未在限定时间内完成）。
    TimedOut { submission: PendingSubmission },
}

// ─── CreativePollWorker ───

/// Creative Runtime 专用轮询器。
///
/// 通过 ProviderRegistry 直接访问 Provider 的 poll() / download()，
/// 不经过 GenerationFacade（Facade 是 Skill 的边界，PollWorker 是基础设施）。
pub struct CreativePollWorker {
    provider_registry: Arc<ProviderRegistry>,
    workspace_dir: PathBuf,
}

impl CreativePollWorker {
    /// 创建轮询器。
    ///
    /// - `provider_registry`：用于按 provider_id 解析 Provider 适配器
    /// - `workspace_dir`：下载目标目录（通常为 workspace 的 assets 目录）
    pub fn new(provider_registry: Arc<ProviderRegistry>, workspace_dir: PathBuf) -> Self {
        Self {
            provider_registry,
            workspace_dir,
        }
    }

    /// 单次轮询：解析 Provider → 调用 poll() → 映射为 PollOutcome。
    pub fn poll_once(&self, provider_id: &str, submission_id: &str) -> PollOutcome {
        let resolved = match self.provider_registry.resolve(provider_id) {
            Some(r) => r,
            None => {
                return PollOutcome::Failed {
                    reason: format!("Provider '{provider_id}' not found in registry"),
                    retryable: false,
                };
            }
        };

        match resolved.adapter.poll(submission_id, &resolved.credential) {
            Ok(result) => map_poll_result(&result),
            Err(e) => PollOutcome::Failed {
                reason: format!("Provider poll error: {e}"),
                retryable: true,
            },
        }
    }

    /// 下载：解析 Provider → 调用 download() → 返回 DownloadedFile。
    pub fn download(&self, provider_id: &str, result_url: &str) -> Result<DownloadedFile, String> {
        let resolved = self
            .provider_registry
            .resolve(provider_id)
            .ok_or_else(|| format!("Provider '{provider_id}' not found in registry"))?;

        let target_dir = self.workspace_dir.as_path();

        match resolved
            .adapter
            .download(result_url, target_dir, &resolved.credential)
        {
            Ok(result) => Ok(DownloadedFile {
                file_path: result.file_path,
                mime_type: result.mime_type,
                file_size: result.file_size,
                duration_secs: result.duration_secs,
            }),
            Err(e) => Err(format!("Provider download error: {e}")),
        }
    }

    /// 阻塞轮询直到所有提交完成或超时。
    ///
    /// - `pending`：待轮询的提交列表
    /// - `timeout_secs`：全局超时（秒）
    /// - `image_interval_secs`：图片类轮询间隔（秒）
    /// - `video_interval_secs`：视频类轮询间隔（秒）
    ///
    /// 返回每个提交的最终结果（顺序与 pending 一致）。
    pub fn wait_and_download(
        &self,
        pending: Vec<PendingSubmission>,
        timeout_secs: u64,
        image_interval_secs: u64,
        video_interval_secs: u64,
    ) -> Vec<SubmissionResult> {
        if pending.is_empty() {
            return Vec::new();
        }

        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        // 跟踪每个提交的状态
        let mut states: Vec<Option<PollState>> = vec![None; pending.len()];

        // 轮询循环：每轮检查所有未完成的提交
        loop {
            if start.elapsed() >= timeout {
                eprintln!("[PollWorker] Global timeout after {timeout_secs}s");
                break;
            }

            let mut all_done = true;

            for (i, submission) in pending.iter().enumerate() {
                if states[i].is_some() {
                    continue; // 已完成或已失败
                }

                all_done = false;

                let outcome = self.poll_once(&submission.provider_id, &submission.submission_id);

                match outcome {
                    PollOutcome::StillRunning { progress } => {
                        eprintln!(
                            "[PollWorker] {} still running ({progress}%)",
                            &submission.submission_id
                        );
                    }
                    PollOutcome::Completed { result_url } => {
                        eprintln!(
                            "[PollWorker] {} completed, downloading...",
                            &submission.submission_id
                        );
                        match self.download(&submission.provider_id, &result_url) {
                            Ok(file) => {
                                eprintln!(
                                    "[PollWorker] {} downloaded: {}",
                                    &submission.submission_id, file.file_path
                                );
                                states[i] = Some(PollState::Downloaded(file));
                            }
                            Err(reason) => {
                                eprintln!(
                                    "[PollWorker] {} download failed: {reason}",
                                    &submission.submission_id
                                );
                                states[i] = Some(PollState::DownloadFailed(reason));
                            }
                        }
                    }
                    PollOutcome::Failed { reason, .. } => {
                        eprintln!(
                            "[PollWorker] {} failed: {reason}",
                            &submission.submission_id
                        );
                        states[i] = Some(PollState::GenerationFailed(reason));
                    }
                }
            }

            if all_done {
                break;
            }

            // 计算本轮最小等待间隔（视频类用较长间隔）
            let min_interval = pending
                .iter()
                .enumerate()
                .filter(|(i, _)| states[*i].is_none())
                .map(|(_, s)| match s.asset_type {
                    AssetType::Video | AssetType::Audio => video_interval_secs,
                    _ => image_interval_secs,
                })
                .min()
                .unwrap_or(image_interval_secs);

            thread::sleep(Duration::from_secs(min_interval));
        }

        // 组装结果
        pending
            .into_iter()
            .zip(states.into_iter())
            .map(|(submission, state)| match state {
                Some(PollState::Downloaded(file)) => SubmissionResult::Success { submission, file },
                Some(PollState::GenerationFailed(reason)) => {
                    SubmissionResult::GenerationFailed { submission, reason }
                }
                Some(PollState::DownloadFailed(reason)) => {
                    SubmissionResult::DownloadFailed { submission, reason }
                }
                None => SubmissionResult::TimedOut { submission },
            })
            .collect()
    }
}

// ─── Internal ───

/// 内部状态追踪（区分下载成功 / 生成失败 / 下载失败）。
#[derive(Clone)]
enum PollState {
    Downloaded(DownloadedFile),
    GenerationFailed(String),
    DownloadFailed(String),
}

/// 将 UnifiedPollResult 映射为 PollOutcome。
fn map_poll_result(result: &UnifiedPollResult) -> PollOutcome {
    match result.status.as_str() {
        "succeeded" => match &result.result_url {
            Some(url) if !url.is_empty() => PollOutcome::Completed {
                result_url: url.clone(),
            },
            _ => PollOutcome::Failed {
                reason: "Completed but no result_url".to_owned(),
                retryable: false,
            },
        },
        "failed" => PollOutcome::Failed {
            reason: result
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown error".to_owned()),
            retryable: result.retryable,
        },
        "pending" | "processing" => PollOutcome::StillRunning {
            progress: result.progress,
        },
        _other => PollOutcome::StillRunning { progress: 0 },
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_outcome_variants() {
        let running = PollOutcome::StillRunning { progress: 42 };
        assert_eq!(running, PollOutcome::StillRunning { progress: 42 });

        let completed = PollOutcome::Completed {
            result_url: "https://example.com/result.png".to_owned(),
        };
        assert!(matches!(completed, PollOutcome::Completed { .. }));

        let failed = PollOutcome::Failed {
            reason: "timeout".to_owned(),
            retryable: true,
        };
        assert!(matches!(
            failed,
            PollOutcome::Failed {
                retryable: true,
                ..
            }
        ));
    }

    #[test]
    fn test_map_poll_result_succeeded() {
        let result = UnifiedPollResult {
            status: "succeeded".to_owned(),
            progress: 100,
            result_url: Some("https://cdn.example.com/out.mp4".to_owned()),
            error_message: None,
            retryable: false,
        };
        assert!(matches!(
            map_poll_result(&result),
            PollOutcome::Completed { .. }
        ));
    }

    #[test]
    fn test_map_poll_result_failed() {
        let result = UnifiedPollResult {
            status: "failed".to_owned(),
            progress: 0,
            result_url: None,
            error_message: Some("quota exceeded".to_owned()),
            retryable: false,
        };
        assert!(matches!(
            map_poll_result(&result),
            PollOutcome::Failed {
                retryable: false,
                ..
            }
        ));
    }

    #[test]
    fn test_map_poll_result_processing() {
        let result = UnifiedPollResult {
            status: "processing".to_owned(),
            progress: 65,
            result_url: None,
            error_message: None,
            retryable: false,
        };
        assert_eq!(
            map_poll_result(&result),
            PollOutcome::StillRunning { progress: 65 }
        );
    }

    #[test]
    fn test_map_poll_result_succeeded_no_url() {
        let result = UnifiedPollResult {
            status: "succeeded".to_owned(),
            progress: 100,
            result_url: None,
            error_message: None,
            retryable: false,
        };
        assert!(matches!(
            map_poll_result(&result),
            PollOutcome::Failed { .. }
        ));
    }

    #[test]
    fn test_wait_and_download_empty() {
        let registry = Arc::new(ProviderRegistry::new());
        let worker = CreativePollWorker::new(registry, PathBuf::from("/tmp"));
        let results = worker.wait_and_download(Vec::new(), 10, 1, 2);
        assert!(results.is_empty());
    }
}
