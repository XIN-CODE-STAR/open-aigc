//! 语义领域模型 — Artifact 语义分析结果。
#![allow(dead_code)]
//!
//! Artifact 是资源事实源（宪法第 5 条）。
//! SemanticProfile 是对 Artifact 的语义分析结果（可重建的索引）。
//! Tags、Entities、Caption、OCR text 是 RAG 检索的入口。
//!
//! 数据流：
//! Artifact → Analyzer → SemanticProfile → SemanticIndex → RAG Retrieval

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::common::InferenceSource;

// ──────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("artifact id is required")]
    ArtifactIdRequired,

    #[error("analyzer id is required")]
    AnalyzerIdRequired,

    #[error("tag name exceeds {max} characters")]
    TagNameTooLong { max: usize },

    #[error("entity name exceeds {max} characters")]
    EntityNameTooLong { max: usize },

    #[error("invalid confidence value: {0} (must be 0.0-1.0)")]
    InvalidConfidence(f32),
}

// ──────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────

const TAG_NAME_MAX: usize = 200;
const ENTITY_NAME_MAX: usize = 200;

// ──────────────────────────────────────────────────────────────────
// ArtifactSemanticProfile — 资源语义分析结果
// ──────────────────────────────────────────────────────────────────

/// Artifact 的语义分析结果。
///
/// 是可重建的索引（宪法第 5 条）：
/// - 如果分析器升级，可以重新分析所有 Artifact 而不改变 Artifact 本身
/// - Tags/Entities/Caption/OCR 是 RAG 检索的入口
/// - Embedding 是向量检索的入口
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSemanticProfile {
    /// 关联的 Artifact ID（一对一）。
    pub artifact_id: String,

    /// 图片/视频描述。
    pub caption: Option<String>,
    /// OCR 提取的文本。
    pub ocr_text: Option<String>,

    /// 语义标签。
    pub tags: Vec<SemanticTag>,
    /// 识别的实体（人物、物体、场景）。
    pub entities: Vec<SemanticEntity>,

    /// 向量索引中的 ID（指向 embedding store 中的向量）。
    pub embedding_id: Option<String>,

    /// 分析器标识（如 "gpt-4o" / "local-clip"）。
    pub analyzer: String,
    /// 分析器版本。
    pub analyzer_version: String,
    /// 分析时间。
    pub analyzed_at: String,

    // ── 新增字段：Resource Intelligence Layer ──
    /// 简短描述（1句话摘要）。
    pub description_short: Option<String>,
    /// 详细描述（完整描述）。
    pub description_detailed: Option<String>,
    /// 检测到的物体列表。
    pub objects: Vec<String>,
    /// 场景标签（室内、室外、工作环境等）。
    pub scene: Vec<String>,
    /// 检测到的动作。
    pub actions: Vec<String>,
    /// 抽象概念（AIGC、图像生成、Prompt 等）。
    pub concepts: Vec<String>,
    /// 资源内部关系（如"人物A在物体B旁边"）。
    pub relations: Vec<SemanticRelation>,
    /// 产生此画像的分析任务 ID。
    pub analysis_job_id: Option<String>,
}

impl ArtifactSemanticProfile {
    /// 验证字段。
    pub fn validate(&self) -> Result<(), SemanticError> {
        if self.artifact_id.is_empty() {
            return Err(SemanticError::ArtifactIdRequired);
        }
        if self.analyzer.is_empty() {
            return Err(SemanticError::AnalyzerIdRequired);
        }
        for tag in &self.tags {
            if tag.name.len() > TAG_NAME_MAX {
                return Err(SemanticError::TagNameTooLong { max: TAG_NAME_MAX });
            }
            if !(0.0..=1.0).contains(&tag.confidence) {
                return Err(SemanticError::InvalidConfidence(tag.confidence));
            }
        }
        for entity in &self.entities {
            if entity.name.len() > ENTITY_NAME_MAX {
                return Err(SemanticError::EntityNameTooLong {
                    max: ENTITY_NAME_MAX,
                });
            }
            if !(0.0..=1.0).contains(&entity.confidence) {
                return Err(SemanticError::InvalidConfidence(entity.confidence));
            }
        }
        Ok(())
    }

    /// 获取所有标签名称（用于快速文本匹配）。
    pub fn tag_names(&self) -> Vec<&str> {
        self.tags.iter().map(|t| t.name.as_str()).collect()
    }

    /// 获取所有实体名称。
    pub fn entity_names(&self) -> Vec<&str> {
        self.entities.iter().map(|e| e.name.as_str()).collect()
    }

    /// 是否有可供检索的文本内容。
    pub fn has_searchable_text(&self) -> bool {
        self.caption.is_some()
            || self.ocr_text.is_some()
            || !self.tags.is_empty()
            || !self.entities.is_empty()
    }
}

// ──────────────────────────────────────────────────────────────────
// SemanticTag — 语义标签
// ──────────────────────────────────────────────────────────────────

/// 语义标签 — 对 Artifact 的分类标注。
///
/// 可以是视觉特征（"sunset"）、风格（"cyberpunk"）、
/// 内容类型（"character:Alice"）、场景（"outdoor"）等。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTag {
    /// 标签名称（支持 "namespace:value" 格式，如 "character:Alice"）。
    pub name: String,
    /// 置信度 0.0 ~ 1.0。
    pub confidence: f32,
    /// 标签来源。
    pub source: InferenceSource,
}

// ──────────────────────────────────────────────────────────────────
// SemanticEntity — 识别的实体
// ──────────────────────────────────────────────────────────────────

/// 识别的实体 — 图片/视频中的具体对象。
///
/// 比 SemanticTag 更结构化，包含实体类型和可选的空间位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticEntity {
    /// 实体类型：character / object / location / style / action。
    pub entity_type: String,
    /// 实体名称。
    pub name: String,
    /// 置信度 0.0 ~ 1.0。
    pub confidence: f32,
    /// 可选：在图片中的位置 (x, y, w, h)。
    pub bbox: Option<BBox>,
}

/// 边界框。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// ──────────────────────────────────────────────────────────────────
// SemanticRelation — 资源内部语义关系
// ──────────────────────────────────────────────────────────────────

/// 资源内部的语义关系（如"人物A在物体B旁边"）。
///
/// 用于描述同一资源内不同实体之间的关系。
/// 例如：一张图片中，人物A正在操作电脑B。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRelation {
    /// 关系类型（如 "uses", "near", "contains", "interacts_with"）。
    pub relation_type: String,
    /// 源实体名称。
    pub source_entity: String,
    /// 目标实体名称。
    pub target_entity: String,
    /// 关系置信度 0.0 ~ 1.0。
    pub confidence: f32,
    /// 关系描述（可选）。
    pub description: Option<String>,
}

// ──────────────────────────────────────────────────────────────────
// SemanticProfileDraft — 构造输入
// ──────────────────────────────────────────────────────────────────

/// 构造 SemanticProfile 的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticProfileDraft {
    pub artifact_id: String,
    pub caption: Option<String>,
    pub ocr_text: Option<String>,
    pub tags: Vec<SemanticTag>,
    pub entities: Vec<SemanticEntity>,
    pub embedding_id: Option<String>,
    pub analyzer: String,
    pub analyzer_version: String,

    // ── 新增字段：Resource Intelligence Layer ──
    pub description_short: Option<String>,
    pub description_detailed: Option<String>,
    pub objects: Vec<String>,
    pub scene: Vec<String>,
    pub actions: Vec<String>,
    pub concepts: Vec<String>,
    pub relations: Vec<SemanticRelation>,
    pub analysis_job_id: Option<String>,
}

impl SemanticProfileDraft {
    pub fn validate(&self) -> Result<(), SemanticError> {
        if self.artifact_id.is_empty() {
            return Err(SemanticError::ArtifactIdRequired);
        }
        if self.analyzer.is_empty() {
            return Err(SemanticError::AnalyzerIdRequired);
        }
        for tag in &self.tags {
            if tag.name.len() > TAG_NAME_MAX {
                return Err(SemanticError::TagNameTooLong { max: TAG_NAME_MAX });
            }
            if !(0.0..=1.0).contains(&tag.confidence) {
                return Err(SemanticError::InvalidConfidence(tag.confidence));
            }
        }
        for entity in &self.entities {
            if entity.name.len() > ENTITY_NAME_MAX {
                return Err(SemanticError::EntityNameTooLong {
                    max: ENTITY_NAME_MAX,
                });
            }
            if !(0.0..=1.0).contains(&entity.confidence) {
                return Err(SemanticError::InvalidConfidence(entity.confidence));
            }
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile() -> ArtifactSemanticProfile {
        ArtifactSemanticProfile {
            artifact_id: "art-1".to_owned(),
            caption: Some("A girl with white hair standing in the rain".to_owned()),
            ocr_text: None,
            tags: vec![
                SemanticTag {
                    name: "character:girl".to_owned(),
                    confidence: 0.95,
                    source: InferenceSource::Llm,
                },
                SemanticTag {
                    name: "weather:rain".to_owned(),
                    confidence: 0.92,
                    source: InferenceSource::Llm,
                },
            ],
            entities: vec![SemanticEntity {
                entity_type: "character".to_owned(),
                name: "Girl".to_owned(),
                confidence: 0.98,
                bbox: Some(BBox {
                    x: 100.0,
                    y: 50.0,
                    width: 200.0,
                    height: 400.0,
                }),
            }],
            embedding_id: Some("emb-art-1".to_owned()),
            analyzer: "gpt-4o".to_owned(),
            analyzer_version: "2024-08-06".to_owned(),
            analyzed_at: "2026-08-17T00:00:00Z".to_owned(),
            // 新增字段
            description_short: Some("A girl standing in the rain".to_owned()),
            description_detailed: Some(
                "A girl with white hair is standing outside in the rain, looking contemplative."
                    .to_owned(),
            ),
            objects: vec!["girl".to_owned(), "rain".to_owned()],
            scene: vec!["outdoor".to_owned(), "rainy".to_owned()],
            actions: vec!["standing".to_owned()],
            concepts: vec!["loneliness".to_owned(), "weather".to_owned()],
            relations: vec![SemanticRelation {
                relation_type: "subject_in".to_owned(),
                source_entity: "Girl".to_owned(),
                target_entity: "rain".to_owned(),
                confidence: 0.95,
                description: Some("Girl is standing in the rain".to_owned()),
            }],
            analysis_job_id: Some("job-123".to_owned()),
        }
    }

    #[test]
    fn valid_profile() {
        let profile = make_profile();
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn profile_tag_names() {
        let profile = make_profile();
        let names = profile.tag_names();
        assert_eq!(names, vec!["character:girl", "weather:rain"]);
    }

    #[test]
    fn profile_entity_names() {
        let profile = make_profile();
        let names = profile.entity_names();
        assert_eq!(names, vec!["Girl"]);
    }

    #[test]
    fn profile_has_searchable_text() {
        let mut profile = make_profile();
        assert!(profile.has_searchable_text());

        profile.caption = None;
        assert!(profile.has_searchable_text()); // tags still present

        profile.tags.clear();
        profile.entities.clear();
        assert!(!profile.has_searchable_text());

        profile.ocr_text = Some("text".to_owned());
        assert!(profile.has_searchable_text());
    }

    #[test]
    fn missing_artifact_id() {
        let mut profile = make_profile();
        profile.artifact_id = String::new();
        assert!(matches!(
            profile.validate(),
            Err(SemanticError::ArtifactIdRequired)
        ));
    }

    #[test]
    fn invalid_confidence() {
        let mut profile = make_profile();
        profile.tags.push(SemanticTag {
            name: "test".to_owned(),
            confidence: 1.5,
            source: InferenceSource::Heuristic,
        });
        assert!(matches!(
            profile.validate(),
            Err(SemanticError::InvalidConfidence(1.5))
        ));
    }

    #[test]
    fn tag_name_too_long() {
        let mut profile = make_profile();
        profile.tags.push(SemanticTag {
            name: "x".repeat(TAG_NAME_MAX + 1),
            confidence: 0.9,
            source: InferenceSource::Heuristic,
        });
        assert!(matches!(
            profile.validate(),
            Err(SemanticError::TagNameTooLong { .. })
        ));
    }

    #[test]
    fn serialization_roundtrip() {
        let profile = make_profile();
        let json = serde_json::to_string(&profile).unwrap();
        let back: ArtifactSemanticProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.artifact_id, "art-1");
        assert_eq!(back.tags.len(), 2);
        assert_eq!(back.entities.len(), 1);
        assert_eq!(back.entities[0].name, "Girl");
    }
}
