//! Generation Pipeline：端到端生成后处理流水线。
//!
//! 串联"下载→入库→版本→评价→安全检查→事件推送"完整后处理链路。
//! PollWorker 轮询到 succeeded 后调用此 Pipeline。
//!
//! 流程：
//!   Provider 返回 result_url
//!   → 下载文件
//!   → 计算 SHA-256
//!   → 导入资产库
//!   → 创建 AssetVersion
//!   → AI Critic 五层评价（可选，需 Vision LLM）
//!   → Content Guard 安全检查
//!   → 更新 Attempt = succeeded
//!   → 推送 generation://completed 事件

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};

use crate::{
    application::error::AppError,
    domain::{
        assets::{AssetRecord, StorageNamespace},
        credentials::CredentialContext,
        review::{
            AssetVersionRecord, AssetVersionStatus, ReviewDecision, ReviewReportRecord,
            ReviewerType,
        },
    },
    ports::{
        asset_repository::{AssetRepository, ImportOptions},
        review_repository::ReviewRepository,
        unified_provider::{UnifiedDownloadResult, UnifiedProviderAdapter},
    },
};

/// Pipeline 阶段事件。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStageEvent {
    pub attempt_id: String,
    pub stage: String,
    pub detail: Option<String>,
}

/// Pipeline 输出。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineOutput {
    /// 导入的资产 ID。
    pub asset_id: String,
    /// 资产版本 ID。
    pub version_id: String,
    /// 评价报告 ID（如果执行了评价）。
    pub review_id: Option<String>,
    /// 内容安全检查状态。
    pub guard_status: String,
    /// 综合评分（如果有评价）。
    pub overall_score: Option<f64>,
    /// 质量是否通过阈值（无评价时默认 true）。
    pub quality_passed: bool,
}

/// Pipeline 配置。
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// 质量阈值（0-100），低于此分数标记为 quality_passed=false。
    pub quality_threshold: f64,
    /// 是否启用 AI Critic 评价。
    pub enable_critic: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            quality_threshold: 60.0,
            enable_critic: true,
        }
    }
}

/// Generation Pipeline 服务。
pub struct GenerationPipeline {
    /// 下载目录。
    download_dir: PathBuf,
    /// 资产仓库（用于复用受管文件目录、manifest、去重和完整性规则）。
    asset_repository: Mutex<Box<dyn AssetRepository>>,
    /// 评价仓库（用于持久化评价报告和资产版本）。
    review_repository: Mutex<Box<dyn ReviewRepository>>,
    /// Vision 适配器（用于 AI Critic 评价）。
    vision_adapter: Mutex<Option<Box<dyn crate::ports::vision_adapter::VisionAdapter>>>,
    /// Vision API 凭据（api_key, base_url, model）。
    vision_credential: Option<(String, String, String)>,
    /// 配置。
    config: PipelineConfig,
}

impl GenerationPipeline {
    pub fn new(
        download_dir: PathBuf,
        review_repository: impl ReviewRepository + 'static,
        asset_repository: impl AssetRepository + 'static,
    ) -> Self {
        Self {
            download_dir,
            asset_repository: Mutex::new(Box::new(asset_repository)),
            review_repository: Mutex::new(Box::new(review_repository)),
            vision_adapter: Mutex::new(None),
            vision_credential: None,
            config: PipelineConfig::default(),
        }
    }

    /// 设置 Vision 适配器（启用 AI Critic）。
    pub fn with_vision(
        self,
        adapter: impl crate::ports::vision_adapter::VisionAdapter + 'static,
        api_key: String,
        base_url: String,
        model: String,
    ) -> Self {
        *self.vision_adapter.lock().unwrap() = Some(Box::new(adapter));
        Self {
            vision_credential: Some((api_key, base_url, model)),
            ..self
        }
    }

    /// 设置配置。
    pub fn with_config(mut self, config: PipelineConfig) -> Self {
        self.config = config;
        self
    }

    /// 完成 Attempt 的全链路后处理。
    ///
    /// 参数：
    /// - `provider`: 用于下载的 Provider 适配器
    /// - `attempt_id`: 生成尝试 ID
    /// - `task_id`: 生成任务 ID
    /// - `result_url`: Provider 返回的结果 URL
    /// - `project_id`: 项目 ID（用于评价报告）
    /// - `app`: 可选 AppHandle，用于向前端推送阶段事件
    ///
    /// 返回 PipelineOutput，包含资产 ID、版本 ID、评价报告 ID 等。
    #[allow(clippy::too_many_arguments)]
    pub fn complete_attempt(
        &self,
        provider: &dyn UnifiedProviderAdapter,
        credential: &CredentialContext,
        attempt_id: &str,
        task_id: &str,
        result_url: &str,
        project_id: &str,
        app: Option<&AppHandle>,
    ) -> Result<PipelineOutput, AppError> {
        // ── Stage 1: 下载 ──────────────────────────────
        Self::emit_stage(app, attempt_id, "downloading", None);
        let download = provider
            .download(result_url, &self.download_dir, credential)
            .map_err(|e| AppError::new("download result", e))?;

        // ── Stage 2: SHA-256 ───────────────────────────
        Self::emit_stage(app, attempt_id, "hashing", None);
        let hash = compute_sha256(&download.file_path)?;

        // ── Stage 3: 导入受管资产 manifest ─────────────
        Self::emit_stage(app, attempt_id, "importing", None);
        let asset = self.import_downloaded_asset(&download)?;
        let asset_id = asset.id.clone();

        // ── Stage 4: 资产版本 ──────────────────────────
        Self::emit_stage(app, attempt_id, "versioning", None);
        let version_id = uuid::Uuid::new_v4().to_string();
        let now = crate::adapters::sqlite::now_rfc3339()?;

        let asset_version = AssetVersionRecord {
            id: version_id.clone(),
            asset_id: asset_id.clone(),
            version: 1,
            storage_key: asset.relative_path.clone(),
            mime_type: asset
                .mime_type
                .clone()
                .unwrap_or(download.mime_type.clone()),
            size_bytes: asset.size_bytes,
            hash: Some(hash),
            width: download.width.map(|w| w as i64),
            height: download.height.map(|h| h as i64),
            duration_seconds: download.duration_secs,
            source_type: "generated".to_owned(),
            source_task_id: Some(task_id.to_owned()),
            source_attempt_id: Some(attempt_id.to_owned()),
            status: AssetVersionStatus::Reviewing,
            status_reason: None,
            review_id: None,
            created_by: "pipeline".to_owned(),
            created_at: now.clone(),
            updated_at: now,
        };

        // 保存资产版本
        self.with_review_repo(|repo| {
            repo.insert_asset_version(&asset_version)
                .map_err(AppError::from)
        })?;

        // ── Stage 5: AI Critic 评价（可选）────────────
        // 视频文件跳过 Critic 评价（视觉模型无法处理视频）
        let is_video = download.mime_type.starts_with("video/");
        Self::emit_stage(app, attempt_id, "evaluating", None);
        let (review_id, overall_score, quality_passed) = if is_video {
            eprintln!(
                "[Pipeline] Skipping Critic evaluation for video asset (mime={})",
                download.mime_type
            );
            Self::emit_stage(app, attempt_id, "evaluating-skipped-video", None);
            (None, None, true)
        } else if self.config.enable_critic {
            match self.run_critic_evaluation(&download.file_path, project_id, attempt_id) {
                Ok((rid, score)) => {
                    let passed = score >= self.config.quality_threshold;
                    (rid, Some(score), passed)
                }
                Err(e) => {
                    // 评价失败不阻塞流程，记录日志后继续
                    eprintln!("[Pipeline] Critic evaluation failed (non-blocking): {e}");
                    (None, None, true)
                }
            }
        } else {
            (None, None, true)
        };

        // ── Stage 6: Content Guard 检查 ─────────────
        Self::emit_stage(app, attempt_id, "guarding", None);
        let guard_result = run_content_guard_check(project_id, attempt_id, &download.mime_type);

        // ── Stage 7: 完成 ──────────────────────────────
        Self::emit_stage(app, attempt_id, "completed", None);
        Ok(PipelineOutput {
            asset_id,
            version_id,
            review_id,
            guard_status: guard_result.0,
            overall_score,
            quality_passed,
        })
    }

    /// 向前端推送 Pipeline 阶段事件。
    fn emit_stage(app: Option<&AppHandle>, attempt_id: &str, stage: &str, detail: Option<String>) {
        if let Some(handle) = app {
            let event = PipelineStageEvent {
                attempt_id: attempt_id.to_owned(),
                stage: stage.to_owned(),
                detail,
            };
            let _ = handle.emit("generation://stage", &event);
        }
    }

    /// 获取下载目录。
    pub fn download_dir(&self) -> &PathBuf {
        &self.download_dir
    }

    fn with_review_repo<T>(
        &self,
        op: impl FnOnce(&mut dyn ReviewRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .review_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        op(repo.as_mut())
    }

    fn with_asset_repo<T>(
        &self,
        op: impl FnOnce(&mut dyn AssetRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .asset_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        op(repo.as_mut())
    }

    fn import_downloaded_asset(
        &self,
        download: &UnifiedDownloadResult,
    ) -> Result<AssetRecord, AppError> {
        self.with_asset_repo(|repo| {
            let summary = repo
                .import(
                    vec![download.file_path.clone()],
                    StorageNamespace::Generation.as_str(),
                    ImportOptions {
                        max_bytes: 2 * 1024 * 1024 * 1024,
                    },
                )
                .map_err(AppError::from)?;

            if let Some(imported) = summary.imported.into_iter().next() {
                return Ok(imported.asset);
            }

            if let Some(skipped) = summary.skipped.into_iter().next() {
                if let Some(asset_id) = skipped.existing_asset_id {
                    return repo.get(&asset_id)?.ok_or_else(|| {
                        AppError::new(
                            "import downloaded asset",
                            format!("duplicate asset {asset_id} was not found"),
                        )
                    });
                }
            }

            let reason = summary
                .failures
                .first()
                .map(|failure| failure.reason.clone())
                .unwrap_or_else(|| "asset import returned no result".to_owned());
            Err(AppError::new("import downloaded asset", reason))
        })
    }

    /// 执行 AI Critic 评价。
    ///
    /// 读取本地文件 → base64 编码 → 调用 Vision API → 解析评分 → 持久化报告。
    /// 返回 (review_id, overall_score)。
    fn run_critic_evaluation(
        &self,
        file_path: &str,
        project_id: &str,
        attempt_id: &str,
    ) -> Result<(Option<String>, f64), AppError> {
        let (api_key, base_url, model) = self.vision_credential.as_ref().ok_or_else(|| {
            AppError::new("critic evaluation", "Vision credential not configured")
        })?;

        // 读取文件并编码
        let image_bytes =
            std::fs::read(file_path).map_err(|e| AppError::new("read image for critic", e))?;

        let mime = if file_path.ends_with(".png") {
            "image/png"
        } else if file_path.ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        };
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
        let data_uri = format!("data:{mime};base64,{b64}");

        // 构建 Critic 评价提示词
        let system_prompt = CRITIC_SYSTEM_PROMPT;
        let user_prompt = "请对这张 AI 生成的图片进行多维审美评价。";

        // 调用 Vision API（锁定适配器）
        let result = {
            let guard = self
                .vision_adapter
                .lock()
                .map_err(|_| AppError::StateUnavailable)?;
            let adapter = guard.as_ref().ok_or_else(|| {
                AppError::new("critic evaluation", "Vision adapter not configured")
            })?;
            adapter
                .evaluate_image(
                    &data_uri,
                    system_prompt,
                    user_prompt,
                    model,
                    api_key,
                    if base_url.is_empty() {
                        None
                    } else {
                        Some(base_url.as_str())
                    },
                )
                .map_err(|e| AppError::new("vision critic evaluation", e))?
        };

        // 持久化评价报告
        let review_id = uuid::Uuid::new_v4().to_string();
        let now = crate::adapters::sqlite::now_rfc3339()?;

        let report = ReviewReportRecord {
            id: review_id.clone(),
            project_id: project_id.to_owned(),
            run_id: None,
            shot_id: None,
            asset_id: None,
            generation_attempt_id: Some(attempt_id.to_owned()),
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: None,
            reviewer_provider: Some(model.clone()),
            requirement_scores_json: "{}".to_owned(),
            visual_scores_json: "{}".to_owned(),
            content_scores_json: "{}".to_owned(),
            commercial_scores_json: "{}".to_owned(),
            technical_scores_json: "{}".to_owned(),
            overall_score: result.overall_score,
            weighted_score: None,
            issues_json: "[]".to_owned(),
            decision: parse_decision(&result.decision),
            confidence: Some(result.confidence),
            source_task_id: None,
            review_version: 1,
            created_at: now,
        };

        self.with_review_repo(|repo| repo.insert_report(&report).map_err(AppError::from))?;

        Ok((Some(review_id), result.overall_score))
    }
}

impl crate::ports::reloadable::Reloadable for GenerationPipeline {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        use crate::adapters::sqlite::{
            asset_repository::SqliteAssetRepository, review_repository::SqliteReviewRepository,
        };

        let new_review_repository = SqliteReviewRepository::open(database_path)?;
        let new_asset_repository = SqliteAssetRepository::open(database_path)?;

        {
            let mut review_repository = self
                .review_repository
                .lock()
                .map_err(|_| AppError::StateUnavailable)?;
            *review_repository = Box::new(new_review_repository);
        }

        {
            let mut asset_repository = self
                .asset_repository
                .lock()
                .map_err(|_| AppError::StateUnavailable)?;
            *asset_repository = Box::new(new_asset_repository);
        }

        Ok(())
    }
}

/// Critic 评价系统提示词。
const CRITIC_SYSTEM_PROMPT: &str = "\
你是一位专业的 AI 生成内容审美评审员。请从以下维度评价这张图片：

1. 视觉质量（构图、色彩、光影、细节）
2. 技术质量（清晰度、无伪影、无畸变）
3. 内容表达（主题明确、情感传达）
4. 创意水平（独特性、艺术性）

请以 JSON 格式输出：
```json
{
  \"overallScore\": 0-100,
  \"decision\": \"accept/revise/regenerate\",
  \"confidence\": 0-1,
  \"strengths\": [\"优点1\", \"优点2\"],
  \"issues\": [\"问题1\", \"问题2\"],
  \"suggestions\": [\"改进建议1\"]
}
```
只输出 JSON。";

/// 解析决策字符串为枚举。
fn parse_decision(s: &str) -> ReviewDecision {
    match s.to_lowercase().as_str() {
        "needs_review" | "needs-review" | "review" => ReviewDecision::NeedsReview,
        "accept" | "approve" | "approved" => ReviewDecision::Accept,
        "revise" | "revision" => ReviewDecision::Revise,
        "regenerate" | "reject" | "rejected" => ReviewDecision::Regenerate,
        _ => ReviewDecision::NeedsReview,
    }
}

// ─────────────────────────────────────────────────────
// 工具函数
// ─────────────────────────────────────────────────────

/// 计算文件的 SHA-256 哈希值。
fn compute_sha256(file_path: &str) -> Result<String, AppError> {
    use sha2::{Digest, Sha256};

    let mut file =
        std::fs::File::open(file_path).map_err(|e| AppError::new("open file for hashing", e))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| AppError::new("read file for hashing", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// 运行内容安全检查（本地规则引擎）。
fn run_content_guard_check(
    _project_id: &str,
    _attempt_id: &str,
    mime_type: &str,
) -> (String, String) {
    // 基于 MIME 类型的基本检查
    let allowed_types = [
        "image/png",
        "image/jpeg",
        "image/webp",
        "video/mp4",
        "video/webm",
    ];

    if allowed_types.contains(&mime_type) {
        ("passed".to_owned(), "low".to_owned())
    } else {
        ("needs_review".to_owned(), "medium".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_output_serialization() {
        let output = PipelineOutput {
            asset_id: "asset-1".into(),
            version_id: "v1".into(),
            review_id: Some("review-1".into()),
            guard_status: "passed".into(),
            overall_score: Some(85.0),
            quality_passed: true,
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("assetId"));
        assert!(json.contains("85"));
        assert!(json.contains("qualityPassed"));
    }

    #[test]
    fn compute_sha256_works() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let hash = compute_sha256(&file_path.to_string_lossy()).unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 is 64 hex chars
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn content_guard_check_allows_image_types() {
        let (status, risk) = run_content_guard_check("proj-1", "att-1", "image/png");
        assert_eq!(status, "passed");
        assert_eq!(risk, "low");
    }

    #[test]
    fn content_guard_check_flags_unknown_types() {
        let (status, risk) = run_content_guard_check("proj-1", "att-1", "application/x-unknown");
        assert_eq!(status, "needs_review");
        assert_eq!(risk, "medium");
    }
}
