//! SemanticRetrievalService — RAG 语义检索服务。
//!
//! 独立于 CanvasContextBuilder（空间/图检索），负责语义层面的检索：
//! - 标签匹配（SemanticTag.name 模糊匹配）
//! - OCR 文本匹配（SemanticProfile.ocr_text / caption 全文匹配）
//! - Embedding 相似度入口（通过 embedding_id 查找）
//! - 向量搜索（通过 VectorRepository 进行语义相似度搜索）
//! - 混合搜索（结合关键词搜索和向量搜索）
//!
//! 设计原则：
//! - 宪法第 2 条：Memory/RAG 提供知识，Canvas 保存空间组织
//! - 宪法第 3 条：RAG 只产生检索结果和关系候选，不直接修改 Canvas
//! - 与 CanvasContextBuilder 分离：BFS 不能做 RAG

use std::sync::Arc;

use crate::application::error::AppError;
use crate::domain::semantic::ArtifactSemanticProfile;
use crate::ports::semantic_repository::SemanticRepository;
use crate::ports::vector_repository::{VectorRepository, VectorSearchRequest};

// ──────────────────────────────────────────────────────────────────
// RetrievalResult — 检索结果
// ──────────────────────────────────────────────────────────────────

/// 语义检索结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalResult {
    /// 匹配的语义画像。
    pub profile: ArtifactSemanticProfile,
    /// 匹配得分（0.0 ~ 1.0）。
    pub score: f32,
    /// 匹配原因（供 Agent 理解为什么命中）。
    pub match_reason: String,
    /// 匹配类型（tag, text, vector, hybrid）。
    pub match_type: String,
}

// ──────────────────────────────────────────────────────────────────
// HybridSearchRequest — 混合检索请求
// ──────────────────────────────────────────────────────────────────

/// 混合检索请求。
#[derive(Debug, Clone)]
pub struct HybridSearchRequest {
    /// 查询文本
    pub query: String,
    /// 查询向量（可选，如果提供则进行向量搜索）
    pub embedding: Option<Vec<f32>>,
    /// 返回结果数量限制
    pub limit: usize,
    /// 最小相似度分数（0.0 ~ 1.0）
    pub min_score: Option<f32>,
    /// 是否启用标签搜索
    pub enable_tag_search: bool,
    /// 是否启用文本搜索
    pub enable_text_search: bool,
    /// 是否启用向量搜索
    pub enable_vector_search: bool,
    /// 标签搜索权重（0.0 ~ 1.0）
    pub tag_weight: f32,
    /// 文本搜索权重（0.0 ~ 1.0）
    pub text_weight: f32,
    /// 向量搜索权重（0.0 ~ 1.0）
    pub vector_weight: f32,
}

impl Default for HybridSearchRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            embedding: None,
            limit: 10,
            min_score: Some(0.3),
            enable_tag_search: true,
            enable_text_search: true,
            enable_vector_search: false,
            tag_weight: 0.4,
            text_weight: 0.4,
            vector_weight: 0.2,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// SemanticRetrievalService
// ──────────────────────────────────────────────────────────────────

/// RAG 语义检索服务。
///
/// 从 SemanticRepository 中按标签、文本、embedding 检索相关资源。
/// 检索结果由 ContextOrchestrator 或 RelationDiscoveryService 消费。
pub struct SemanticRetrievalService {
    semantic_repo: Arc<dyn SemanticRepository>,
    vector_repo: Option<Arc<dyn VectorRepository>>,
}

impl SemanticRetrievalService {
    pub fn new(semantic_repo: Arc<dyn SemanticRepository>) -> Self {
        Self {
            semantic_repo,
            vector_repo: None,
        }
    }

    /// Create with vector repository support.
    pub fn with_vector_repo(
        semantic_repo: Arc<dyn SemanticRepository>,
        vector_repo: Arc<dyn VectorRepository>,
    ) -> Self {
        Self {
            semantic_repo,
            vector_repo: Some(vector_repo),
        }
    }

    /// 按标签名称检索语义画像。
    ///
    /// 用于 Agent 查询"哪些资源包含角色 Alice"、"哪些资源是 cyberpunk 风格"。
    pub fn search_by_tag(
        &self,
        tag_query: &str,
        limit: usize,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        let profiles = self.semantic_repo.find_profiles_by_tag(tag_query, limit)?;

        Ok(profiles
            .into_iter()
            .map(|profile| {
                let matching_tags: Vec<&str> = profile
                    .tags
                    .iter()
                    .filter(|t| t.name.contains(tag_query))
                    .map(|t| t.name.as_str())
                    .collect();

                let avg_confidence = if matching_tags.is_empty() {
                    0.5
                } else {
                    let sum: f32 = profile
                        .tags
                        .iter()
                        .filter(|t| t.name.contains(tag_query))
                        .map(|t| t.confidence)
                        .sum();
                    sum / matching_tags.len() as f32
                };

                RetrievalResult {
                    score: avg_confidence,
                    match_reason: format!("标签匹配: {}", matching_tags.join(", ")),
                    match_type: "tag".to_owned(),
                    profile,
                }
            })
            .collect())
    }

    /// 按文本内容检索（OCR + caption 全文搜索）。
    ///
    /// 用于 Agent 查询"哪些资源包含某个文本片段"、"哪些资源描述了雨天场景"。
    pub fn search_by_text(
        &self,
        text_query: &str,
        limit: usize,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        let profiles = self.semantic_repo.find_profiles_by_ocr(text_query, limit)?;

        Ok(profiles
            .into_iter()
            .map(|profile| {
                let mut match_reason = String::new();
                if let Some(ref caption) = profile.caption {
                    if caption.contains(text_query) {
                        match_reason.push_str("caption 匹配");
                    }
                }
                if let Some(ref ocr) = profile.ocr_text {
                    if ocr.contains(text_query) {
                        if !match_reason.is_empty() {
                            match_reason.push_str(", ");
                        }
                        match_reason.push_str("OCR 匹配");
                    }
                }
                if match_reason.is_empty() {
                    match_reason = "文本匹配".to_owned();
                }

                // Simple relevance score based on text match position
                let score = 0.8; // Could be improved with TF-IDF or BM25

                RetrievalResult {
                    score,
                    match_reason,
                    match_type: "text".to_owned(),
                    profile,
                }
            })
            .collect())
    }

    /// 按向量相似度检索。
    ///
    /// 用于语义相似度搜索，需要提供查询向量。
    pub fn search_by_vector(
        &self,
        embedding: &[f32],
        limit: usize,
        min_score: Option<f32>,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        let vector_repo = self
            .vector_repo
            .as_ref()
            .ok_or_else(|| AppError::Workflow("vector repository not configured".to_owned()))?;

        let search_request = VectorSearchRequest {
            embedding: embedding.to_vec(),
            limit,
            min_score,
            asset_ids: None,
            content_type: None,
        };

        let vector_results = vector_repo.search(&search_request)?;

        let mut results = Vec::new();
        for vector_result in vector_results {
            // Get the full profile from the semantic repository
            if let Some(profile) = self
                .semantic_repo
                .get_profile(&vector_result.entry.metadata.asset_id)?
            {
                results.push(RetrievalResult {
                    profile,
                    score: vector_result.score,
                    match_reason: format!(
                        "向量相似度: {:.2} (内容: {})",
                        vector_result.score, vector_result.entry.metadata.content_type
                    ),
                    match_type: "vector".to_owned(),
                });
            }
        }

        Ok(results)
    }

    /// 混合检索：结合标签、文本和向量搜索。
    ///
    /// 使用加权融合策略，综合多种检索方式的结果。
    pub fn hybrid_search(
        &self,
        request: &HybridSearchRequest,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        let mut all_results: Vec<RetrievalResult> = Vec::new();

        // 标签搜索
        if request.enable_tag_search && !request.query.is_empty() {
            let tag_results = self.search_by_tag(&request.query, request.limit * 2)?;
            for mut result in tag_results {
                result.score *= request.tag_weight;
                all_results.push(result);
            }
        }

        // 文本搜索
        if request.enable_text_search && !request.query.is_empty() {
            let text_results = self.search_by_text(&request.query, request.limit * 2)?;
            for mut result in text_results {
                result.score *= request.text_weight;
                all_results.push(result);
            }
        }

        // 向量搜索
        if request.enable_vector_search {
            if let Some(embedding) = &request.embedding {
                let vector_results =
                    self.search_by_vector(embedding, request.limit * 2, request.min_score)?;
                for mut result in vector_results {
                    result.score *= request.vector_weight;
                    all_results.push(result);
                }
            }
        }

        // 合并去重，保留最高分
        let mut merged: std::collections::HashMap<String, RetrievalResult> =
            std::collections::HashMap::new();

        for result in all_results {
            let key = result.profile.artifact_id.clone();
            match merged.get_mut(&key) {
                Some(existing) => {
                    // 累加分数（如果来自不同检索方式）
                    if result.match_type != existing.match_type {
                        existing.score = (existing.score + result.score).min(1.0);
                        existing.match_reason =
                            format!("{}, {}", existing.match_reason, result.match_reason);
                        existing.match_type = "hybrid".to_owned();
                    } else if result.score > existing.score {
                        existing.score = result.score;
                        existing.match_reason = result.match_reason;
                    }
                }
                None => {
                    merged.insert(key, result);
                }
            }
        }

        let mut results: Vec<RetrievalResult> = merged.into_values().collect();

        // 过滤低于最小分数的结果
        if let Some(min_score) = request.min_score {
            results.retain(|r| r.score >= min_score);
        }

        // 按分数降序排序
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(request.limit);

        Ok(results)
    }

    /// 按 artifact_id 获取语义画像（直接查找）。
    pub fn get_profile(
        &self,
        artifact_id: &str,
    ) -> Result<Option<ArtifactSemanticProfile>, AppError> {
        self.semantic_repo.get_profile(artifact_id)
    }

    /// 综合检索：同时按标签和文本搜索，去重合并。
    ///
    /// ContextOrchestrator 使用此方法进行统一的 RAG 检索。
    pub fn unified_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RetrievalResult>, AppError> {
        let request = HybridSearchRequest {
            query: query.to_owned(),
            limit,
            enable_tag_search: true,
            enable_text_search: true,
            enable_vector_search: false,
            ..Default::default()
        };
        self.hybrid_search(&request)
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::semantic_repository::SqliteSemanticRepository;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::{SemanticEntity, SemanticTag};
    use rusqlite::Connection;

    fn setup_service() -> SemanticRetrievalService {
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
        SemanticRetrievalService::new(repo)
    }

    fn seed_profile(svc: &SemanticRetrievalService, id: &str, caption: &str, tags: Vec<&str>) {
        let profile = ArtifactSemanticProfile {
            artifact_id: id.to_owned(),
            caption: Some(caption.to_owned()),
            ocr_text: None,
            tags: tags
                .into_iter()
                .map(|t| SemanticTag {
                    name: t.to_owned(),
                    confidence: 0.9,
                    source: InferenceSource::Llm,
                })
                .collect(),
            entities: vec![SemanticEntity {
                entity_type: "character".to_owned(),
                name: "Alice".to_owned(),
                confidence: 0.95,
                bbox: None,
            }],
            embedding_id: None,
            analyzer: "test".to_owned(),
            analyzer_version: "v1".to_owned(),
            analyzed_at: "2026-08-17T00:00:00Z".to_owned(),
            // 新增字段
            description_short: None,
            description_detailed: None,
            objects: Vec::new(),
            scene: Vec::new(),
            actions: Vec::new(),
            concepts: Vec::new(),
            relations: Vec::new(),
            analysis_job_id: None,
        };
        svc.semantic_repo.upsert_profile(&profile).unwrap();
    }

    #[test]
    fn search_by_tag_finds_match() {
        let svc = setup_service();
        seed_profile(
            &svc,
            "art-1",
            "A girl in rain",
            vec!["character:Alice", "weather:rain"],
        );

        let results = svc.search_by_tag("Alice", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].profile.artifact_id, "art-1");
        assert!(results[0].match_reason.contains("Alice"));
    }

    #[test]
    fn search_by_text_finds_caption() {
        let svc = setup_service();
        seed_profile(
            &svc,
            "art-1",
            "A cyberpunk city at night",
            vec!["style:cyberpunk"],
        );

        let results = svc.search_by_text("cyberpunk", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].match_reason.contains("caption"));
    }

    #[test]
    fn unified_search_merges() {
        let svc = setup_service();
        seed_profile(
            &svc,
            "art-1",
            "Alice in cyberpunk city",
            vec!["character:Alice", "style:cyberpunk"],
        );
        seed_profile(&svc, "art-2", "Rain scene", vec!["weather:rain"]);

        let results = svc.unified_search("Alice", 10).unwrap();
        assert_eq!(results.len(), 1); // Only art-1 matches "Alice"
        assert_eq!(results[0].profile.artifact_id, "art-1");
    }

    #[test]
    fn empty_search_returns_empty() {
        let svc = setup_service();
        let results = svc.search_by_tag("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }
}
