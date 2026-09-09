//! SemanticPipelineService — 图片语义分析管线。
//!
//! 编排 Image Perception Pipeline 的核心服务：
//! 图片 → CaptioningPort → SemanticProfileDraft → SemanticRepository
//!
//! 数据流：
//! 1. 接收图片 data URL + asset_id
//! 2. 调用 CaptioningPort 生成描述
//! 3. 将 CaptionResult 转换为 ArtifactSemanticProfile
//! 4. 存入 SemanticRepository（SQLite）
//! 5. 返回语义画像，供 RAG 检索和关系发现使用
//!
//! 设计原则：
//! - 可插拔：支持多个 CaptioningPort（DashScope、Florence-2），按优先级选择
//! - 可重建：SemanticProfile 是可重建的索引（宪法第 5 条），重新分析会覆盖旧数据
//! - 幂等：对同一 artifact_id 多次分析等效于 upsert

use std::sync::Arc;

use crate::application::error::AppError;
use crate::domain::semantic::ArtifactSemanticProfile;
use crate::ports::captioning_port::{CaptionRequest, CaptioningPort};
use crate::ports::semantic_repository::SemanticRepository;

// ──────────────────────────────────────────────────────────────────
// PipelineResult
// ──────────────────────────────────────────────────────────────────

/// 单个资源的分析结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    /// 语义画像。
    pub profile: ArtifactSemanticProfile,
    /// 使用的适配器 ID。
    pub adapter_id: String,
    /// 处理耗时（毫秒）。
    pub elapsed_ms: u64,
}

// ──────────────────────────────────────────────────────────────────
// SemanticPipelineService
// ──────────────────────────────────────────────────────────────────

/// 图片语义分析管线服务。
///
/// 编排 captioning → storage 流程。
/// 支持多个 captioning 适配器，按注册顺序优先选择可用的。
pub struct SemanticPipelineService {
    captioners: Vec<Arc<dyn CaptioningPort>>,
    semantic_repo: Arc<dyn SemanticRepository>,
}

impl SemanticPipelineService {
    pub fn new(
        captioners: Vec<Arc<dyn CaptioningPort>>,
        semantic_repo: Arc<dyn SemanticRepository>,
    ) -> Self {
        Self {
            captioners,
            semantic_repo,
        }
    }

    /// 获取可用的 captioning 适配器。
    ///
    /// 如果指定了 preferred_adapter，优先使用；否则使用第一个就绪的适配器。
    fn get_captioner(
        &self,
        preferred_adapter: Option<&str>,
    ) -> Result<&Arc<dyn CaptioningPort>, AppError> {
        if let Some(preferred) = preferred_adapter {
            if let Some(c) = self
                .captioners
                .iter()
                .find(|c| c.adapter_id() == preferred && c.is_ready())
            {
                return Ok(c);
            }
        }

        self.captioners
            .iter()
            .find(|c| c.is_ready())
            .ok_or_else(|| {
                AppError::Workflow(format!(
                    "no captioning adapter available (tried: {})",
                    self.captioners
                        .iter()
                        .map(|c| c.adapter_id())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }

    /// 分析单个资源。
    ///
    /// 调用 captioning 适配器生成描述，存入 SemanticRepository。
    pub fn analyze_asset(
        &self,
        asset_id: &str,
        image_data_url: &str,
        preferred_adapter: Option<&str>,
    ) -> Result<AnalysisResult, AppError> {
        let captioner = self.get_captioner(preferred_adapter)?;
        let adapter_id = captioner.adapter_id().to_owned();

        eprintln!(
            "[SemanticPipeline] analyzing asset={} with adapter={}",
            asset_id, adapter_id
        );

        let t0 = std::time::Instant::now();

        let request = CaptionRequest {
            image_url: image_data_url.to_owned(),
            instruction: None,
            asset_id: Some(asset_id.to_owned()),
        };

        let result = captioner
            .caption_image(&request)
            .map_err(|e| AppError::Workflow(format!("captioning failed: {e}")))?;

        let elapsed = t0.elapsed().as_millis() as u64;

        // 转换为 ArtifactSemanticProfile
        let profile = ArtifactSemanticProfile {
            artifact_id: asset_id.to_owned(),
            caption: Some(result.caption),
            ocr_text: result.ocr_text,
            tags: result.tags,
            entities: result.entities,
            embedding_id: None, // Phase 2+：由 embedding 服务生成
            analyzer: adapter_id.clone(),
            analyzer_version: "1.0".to_owned(),
            analyzed_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
            // 新增字段：Resource Intelligence Layer
            description_short: None,
            description_detailed: None,
            objects: Vec::new(),
            scene: Vec::new(),
            actions: Vec::new(),
            concepts: Vec::new(),
            relations: Vec::new(),
            analysis_job_id: None,
        };

        // 验证并存储
        profile
            .validate()
            .map_err(|e| AppError::Workflow(format!("invalid semantic profile: {e}")))?;

        self.semantic_repo.upsert_profile(&profile)?;

        eprintln!(
            "[SemanticPipeline] asset={} analyzed in {}ms, caption_len={}, tags={}, entities={}",
            asset_id,
            elapsed,
            profile.caption.as_ref().map(|c| c.len()).unwrap_or(0),
            profile.tags.len(),
            profile.entities.len(),
        );

        Ok(AnalysisResult {
            profile,
            adapter_id,
            elapsed_ms: elapsed,
        })
    }

    /// 批量分析资源。
    pub fn analyze_batch(
        &self,
        items: &[(String, String)], // (asset_id, image_data_url)
        preferred_adapter: Option<&str>,
        on_progress: Option<&dyn Fn(usize, usize)>,
    ) -> Result<Vec<AnalysisResult>, AppError> {
        let mut results = Vec::with_capacity(items.len());

        for (i, (asset_id, image_url)) in items.iter().enumerate() {
            if let Some(cb) = on_progress {
                cb(i, items.len());
            }

            match self.analyze_asset(asset_id, image_url, preferred_adapter) {
                Ok(result) => results.push(result),
                Err(e) => {
                    eprintln!(
                        "[SemanticPipeline] failed to analyze asset={}: {e}",
                        asset_id
                    );
                    // 继续处理其他资源，不因单个失败而中断
                }
            }
        }

        Ok(results)
    }

    /// 获取已分析的语义画像。
    pub fn get_profile(&self, asset_id: &str) -> Result<Option<ArtifactSemanticProfile>, AppError> {
        self.semantic_repo.get_profile(asset_id)
    }

    /// 获取所有可用适配器信息。
    pub fn available_adapters(&self) -> Vec<crate::ports::captioning_port::CaptionerInfo> {
        self.captioners.iter().map(|c| c.info()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::semantic_repository::SqliteSemanticRepository;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::{SemanticEntity, SemanticTag};
    use crate::ports::captioning_port::{
        CaptionError, CaptionRequest, CaptionResult, CaptioningPort,
    };
    use rusqlite::Connection;

    /// Mock captioner for testing.
    struct MockCaptioner {
        ready: bool,
    }

    impl MockCaptioner {
        fn new() -> Self {
            Self { ready: true }
        }
    }

    impl CaptioningPort for MockCaptioner {
        fn adapter_id(&self) -> &str {
            "mock"
        }

        fn supported_models(&self) -> Vec<&str> {
            vec!["mock-model"]
        }

        fn is_ready(&self) -> bool {
            self.ready
        }

        fn caption_image(&self, _request: &CaptionRequest) -> Result<CaptionResult, CaptionError> {
            Ok(CaptionResult {
                caption: "A test image with a cat sitting on a desk".to_owned(),
                tags: vec![
                    SemanticTag {
                        name: "cat".to_owned(),
                        confidence: 0.95,
                        source: InferenceSource::Llm,
                    },
                    SemanticTag {
                        name: "desk".to_owned(),
                        confidence: 0.90,
                        source: InferenceSource::Llm,
                    },
                ],
                entities: vec![SemanticEntity {
                    entity_type: "object".to_owned(),
                    name: "cat".to_owned(),
                    confidence: 0.95,
                    bbox: None,
                }],
                ocr_text: None,
                raw_response: "{}".to_owned(),
                tokens_used: Some(100),
            })
        }
    }

    fn setup_service() -> SemanticPipelineService {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifact_semantic_profiles (
                artifact_id TEXT PRIMARY KEY,
                caption TEXT,
                ocr_text TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                entities_json TEXT NOT NULL DEFAULT '[]',
                embedding_id TEXT,
                analyzer TEXT NOT NULL,
                analyzer_version TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                description_short TEXT,
                description_detailed TEXT,
                objects_json TEXT NOT NULL DEFAULT '[]',
                scene_json TEXT NOT NULL DEFAULT '[]',
                actions_json TEXT NOT NULL DEFAULT '[]',
                concepts_json TEXT NOT NULL DEFAULT '[]',
                relations_json TEXT NOT NULL DEFAULT '[]',
                analysis_job_id TEXT
            );",
        )
        .unwrap();
        let repo = Arc::new(SqliteSemanticRepository::open(conn));
        let captioner = Arc::new(MockCaptioner::new());
        SemanticPipelineService::new(vec![captioner], repo)
    }

    #[test]
    fn analyze_asset_stores_profile() {
        let svc = setup_service();
        let result = svc
            .analyze_asset("asset-1", "data:image/png;base64,abc123", None)
            .unwrap();

        assert_eq!(result.profile.artifact_id, "asset-1");
        assert!(result.profile.caption.is_some());
        assert_eq!(result.profile.tags.len(), 2);
        assert_eq!(result.adapter_id, "mock");

        // Verify stored in repo
        let stored = svc.get_profile("asset-1").unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().artifact_id, "asset-1");
    }

    #[test]
    fn analyze_batch_processes_all() {
        let svc = setup_service();
        let items = vec![
            ("a1".to_owned(), "data:image/png;base64,a".to_owned()),
            ("a2".to_owned(), "data:image/png;base64,b".to_owned()),
        ];
        let results = svc.analyze_batch(&items, None, None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn no_adapter_returns_error() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifact_semantic_profiles (
                artifact_id TEXT PRIMARY KEY,
                caption TEXT, ocr_text TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                entities_json TEXT NOT NULL DEFAULT '[]',
                embedding_id TEXT,
                analyzer TEXT NOT NULL,
                analyzer_version TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                description_short TEXT,
                description_detailed TEXT,
                objects_json TEXT NOT NULL DEFAULT '[]',
                scene_json TEXT NOT NULL DEFAULT '[]',
                actions_json TEXT NOT NULL DEFAULT '[]',
                concepts_json TEXT NOT NULL DEFAULT '[]',
                relations_json TEXT NOT NULL DEFAULT '[]',
                analysis_job_id TEXT
            );",
        )
        .unwrap();
        let repo = Arc::new(SqliteSemanticRepository::open(conn));
        let svc = SemanticPipelineService::new(vec![], repo);

        let err = svc
            .analyze_asset("asset-1", "data:image/png;base64,abc", None)
            .unwrap_err();
        assert!(err.to_string().contains("no captioning adapter"));
    }
}
