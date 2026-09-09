//! AI Critic Agent 领域模型。
//!
//! 定义多维评价体系的类型、校验规则和序列化结构。
//! 按照 2026-07-20-creative-agent-critical-capability-decisions.md §4 设计。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────
// 常量
// ─────────────────────────────────────────────────────

const INTENT_TEXT_MAX_LENGTH: usize = 2_000;
const PLAN_SUMMARY_MAX_LENGTH: usize = 500;
const ISSUE_MESSAGE_MAX_LENGTH: usize = 1_000;
const SUGGESTED_FIX_MAX_LENGTH: usize = 1_000;
const DIMENSION_REASONING_MAX_LENGTH: usize = 2_000;
const REVIEWER_NAME_MAX_LENGTH: usize = 80;
const ASSET_HASH_LENGTH: usize = 64;

// ═══════════════════════════════════════════════════
// 评价类型枚举
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewerType {
    Auto,
    Manual,
    Hybrid,
}

impl ReviewerType {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            "hybrid" => Ok(Self::Hybrid),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "reviewerType",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    NeedsReview,
    Accept,
    AcceptWithSuggestions,
    Revise,
    Regenerate,
    Block,
}

impl ReviewDecision {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "needs_review" => Ok(Self::NeedsReview),
            "accept" => Ok(Self::Accept),
            "accept_with_suggestions" => Ok(Self::AcceptWithSuggestions),
            "revise" => Ok(Self::Revise),
            "regenerate" => Ok(Self::Regenerate),
            "block" => Ok(Self::Block),
            _ => Err(ReviewValidationError::InvalidChoice { field: "decision" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsReview => "needs_review",
            Self::Accept => "accept",
            Self::AcceptWithSuggestions => "accept_with_suggestions",
            Self::Revise => "revise",
            Self::Regenerate => "regenerate",
            Self::Block => "block",
        }
    }

    /// 从总分推导决策。
    pub fn from_overall_score(score: f64) -> Self {
        if score >= 90.0 {
            Self::Accept
        } else if score >= 80.0 {
            Self::AcceptWithSuggestions
        } else if score >= 70.0 {
            Self::Revise
        } else if score >= 60.0 {
            Self::Regenerate
        } else {
            Self::Block
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewDimensionLayer {
    Requirement,
    Visual,
    Content,
    Commercial,
    Technical,
}

impl ReviewDimensionLayer {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "requirement" => Ok(Self::Requirement),
            "visual" => Ok(Self::Visual),
            "content" => Ok(Self::Content),
            "commercial" => Ok(Self::Commercial),
            "technical" => Ok(Self::Technical),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "dimensionLayer",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Visual => "visual",
            Self::Content => "content",
            Self::Commercial => "commercial",
            Self::Technical => "technical",
        }
    }
}

// ═══════════════════════════════════════════════════
// 评价问题
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl IssueSeverity {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            _ => Err(ReviewValidationError::InvalidChoice { field: "severity" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// 评价中发现的单个问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewIssue {
    pub dimension: String,
    pub severity: IssueSeverity,
    pub message: String,
    pub suggested_fix: String,
}

impl ReviewIssue {
    pub fn try_new(
        dimension: String,
        severity: String,
        message: String,
        suggested_fix: String,
    ) -> Result<Self, ReviewValidationError> {
        let dimension = normalize_dimension_field(dimension, "dimension")?;
        let severity = IssueSeverity::parse(severity.trim())?;
        let message = normalize_required(message, "message", ISSUE_MESSAGE_MAX_LENGTH)?;
        let suggested_fix =
            normalize_required(suggested_fix, "suggestedFix", SUGGESTED_FIX_MAX_LENGTH)?;

        Ok(Self {
            dimension,
            severity,
            message,
            suggested_fix,
        })
    }
}

// ═══════════════════════════════════════════════════
// 评价维度详情
// ═══════════════════════════════════════════════════

/// 单个评价维度的详细记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDimensionRecord {
    pub id: String,
    pub review_id: String,
    pub dimension_layer: ReviewDimensionLayer,
    pub dimension_name: String,
    pub score: f64,
    pub weight: f64,
    pub confidence: Option<f64>,
    pub reasoning: Option<String>,
    pub reference_context_json: String,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// 评价报告
// ═══════════════════════════════════════════════════

/// 五层评分的子结构（分别存储为 JSON 列）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequirementScores {
    #[serde(rename = "match")]
    pub requirement_match: Option<f64>,
    pub completeness: Option<f64>,
    pub clarity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VisualScores {
    pub composition: Option<f64>,
    pub color: Option<f64>,
    pub lighting: Option<f64>,
    pub texture: Option<f64>,
    pub lens_language: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContentScores {
    pub theme_match: Option<f64>,
    pub emotion_expression: Option<f64>,
    pub narrative_purpose: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommercialScores {
    pub platform_fit: Option<f64>,
    pub audience_fit: Option<f64>,
    pub conversion_potential: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalScores {
    pub clarity: Option<f64>,
    pub distortion: Option<f64>,
    pub character_consistency: Option<f64>,
    pub motion_quality: Option<f64>,
}

/// 评价报告记录（数据库行映射）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReportRecord {
    pub id: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub shot_id: Option<String>,
    pub asset_id: Option<String>,
    pub generation_attempt_id: Option<String>,
    pub reviewer_type: ReviewerType,
    pub reviewer_agent_version: Option<String>,
    pub reviewer_provider: Option<String>,
    pub requirement_scores_json: String,
    pub visual_scores_json: String,
    pub content_scores_json: String,
    pub commercial_scores_json: String,
    pub technical_scores_json: String,
    pub overall_score: f64,
    pub weighted_score: Option<f64>,
    pub issues_json: String,
    pub decision: ReviewDecision,
    pub confidence: Option<f64>,
    pub source_task_id: Option<String>,
    pub review_version: i64,
    pub created_at: String,
}

/// 视频生成前的关键帧质量门槛。
#[derive(Debug, Clone, PartialEq)]
pub struct VideoGenerationGate {
    pub overall_min: f64,
    pub character_consistency_min: f64,
    pub style_consistency_min: f64,
    pub requirement_match_min: f64,
}

impl Default for VideoGenerationGate {
    fn default() -> Self {
        Self {
            overall_min: 80.0,
            character_consistency_min: 80.0,
            style_consistency_min: 80.0,
            requirement_match_min: 80.0,
        }
    }
}

impl VideoGenerationGate {
    /// 检查报告是否通过视频生成前的质量门槛。
    pub fn passes(&self, report: &ReviewReportRecord) -> bool {
        if report.overall_score < self.overall_min {
            return false;
        }
        // 从 JSON 中提取关键子维度（尽力而为解析）
        if let Ok(tech) = serde_json::from_str::<TechnicalScores>(&report.technical_scores_json) {
            if tech.character_consistency.unwrap_or(0.0) < self.character_consistency_min {
                return false;
            }
        }
        if let Ok(content) = serde_json::from_str::<ContentScores>(&report.content_scores_json) {
            if content.theme_match.unwrap_or(0.0) < self.style_consistency_min {
                return false;
            }
        }
        if let Ok(req) = serde_json::from_str::<RequirementScores>(&report.requirement_scores_json)
        {
            if req.requirement_match.unwrap_or(0.0) < self.requirement_match_min {
                return false;
            }
        }
        true
    }
}

// ═══════════════════════════════════════════════════
// 资产版本
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetVersionStatus {
    Draft,
    Reviewing,
    Approved,
    Selected,
    UsedInDeliverable,
    Archived,
    Deprecated,
    Rejected,
}

impl AssetVersionStatus {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "draft" => Ok(Self::Draft),
            "reviewing" => Ok(Self::Reviewing),
            "approved" => Ok(Self::Approved),
            "selected" => Ok(Self::Selected),
            "used_in_deliverable" => Ok(Self::UsedInDeliverable),
            "archived" => Ok(Self::Archived),
            "deprecated" => Ok(Self::Deprecated),
            "rejected" => Ok(Self::Rejected),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "assetVersionStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewing => "reviewing",
            Self::Approved => "approved",
            Self::Selected => "selected",
            Self::UsedInDeliverable => "used_in_deliverable",
            Self::Archived => "archived",
            Self::Deprecated => "deprecated",
            Self::Rejected => "rejected",
        }
    }

    /// 是否为最终态（不再变化）。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Archived | Self::Deprecated | Self::Rejected)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersionRecord {
    pub id: String,
    pub asset_id: String,
    pub version: i64,
    pub storage_key: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub hash: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_seconds: Option<f64>,
    pub source_type: String,
    pub source_task_id: Option<String>,
    pub source_attempt_id: Option<String>,
    pub status: AssetVersionStatus,
    pub status_reason: Option<String>,
    pub review_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

// ═══════════════════════════════════════════════════
// 版权记录
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseSourceType {
    AiGenerated,
    UserUploaded,
    ThirdParty,
    Derived,
}

impl LicenseSourceType {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "ai_generated" => Ok(Self::AiGenerated),
            "user_uploaded" => Ok(Self::UserUploaded),
            "third_party" => Ok(Self::ThirdParty),
            "derived" => Ok(Self::Derived),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "licenseSourceType",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AiGenerated => "ai_generated",
            Self::UserUploaded => "user_uploaded",
            Self::ThirdParty => "third_party",
            Self::Derived => "derived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialUseStatus {
    Clear,
    NeedsReview,
    Restricted,
    Blocked,
    Unknown,
}

impl CommercialUseStatus {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "clear" => Ok(Self::Clear),
            "needs_review" => Ok(Self::NeedsReview),
            "restricted" => Ok(Self::Restricted),
            "blocked" => Ok(Self::Blocked),
            "unknown" => Ok(Self::Unknown),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "commercialUseStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::NeedsReview => "needs_review",
            Self::Restricted => "restricted",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLicenseRecord {
    pub id: String,
    pub asset_id: String,
    pub asset_version_id: Option<String>,
    pub source_type: LicenseSourceType,
    pub provider_id: Option<String>,
    pub model_name: Option<String>,
    pub commercial_use_status: CommercialUseStatus,
    pub commercial_use_details: Option<String>,
    pub source_assets_json: Option<String>,
    pub copyright_statement: Option<String>,
    pub attribution_required: bool,
    pub risk_flags_json: Option<String>,
    pub review_required: bool,
    pub review_notes: Option<String>,
    pub export_allowed: bool,
    pub export_block_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ═══════════════════════════════════════════════════
// 内容安全报告
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentGuardStatus {
    Passed,
    NeedsReview,
    Flagged,
    Blocked,
}

impl ContentGuardStatus {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "passed" => Ok(Self::Passed),
            "needs_review" => Ok(Self::NeedsReview),
            "flagged" => Ok(Self::Flagged),
            "blocked" => Ok(Self::Blocked),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "contentGuardStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::NeedsReview => "needs_review",
            Self::Flagged => "flagged",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl ContentRiskLevel {
    pub fn parse(value: &str) -> Result<Self, ReviewValidationError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            _ => Err(ReviewValidationError::InvalidChoice {
                field: "contentRiskLevel",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentGuardReportRecord {
    pub id: String,
    pub project_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub task_id: Option<String>,
    pub asset_id: Option<String>,
    pub guard_version: Option<String>,
    pub guard_provider: Option<String>,
    pub status: ContentGuardStatus,
    pub risk_level: ContentRiskLevel,
    pub checks_json: String,
    pub actions_json: String,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════
// 验证错误
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReviewValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} contains unsupported control characters")]
    ControlCharacters { field: &'static str },
    #[error("{field} is invalid")]
    InvalidChoice { field: &'static str },
    #[error("{field} is not a valid uuid")]
    InvalidUuid { field: &'static str },
    #[error("{field} must be between 0 and 100")]
    InvalidScore { field: &'static str },
    #[error("confidence must be between 0 and 1")]
    InvalidConfidence,
}

impl ReviewValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field }
            | Self::InvalidChoice { field }
            | Self::InvalidUuid { field }
            | Self::InvalidScore { field } => Some(field),
            Self::InvalidConfidence => Some("confidence"),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", field_label(field)),
            Self::TooLong { field, max_length } => {
                format!("{}不能超过 {max_length} 个字符。", field_label(field))
            }
            Self::ControlCharacters { field } => {
                format!("{}包含不支持的控制字符。", field_label(field))
            }
            Self::InvalidChoice { field } => format!("{}无效。", field_label(field)),
            Self::InvalidUuid { field } => format!("{}格式无效。", field_label(field)),
            Self::InvalidScore { field } => {
                format!("{}必须在 0 到 100 之间。", field_label(field))
            }
            Self::InvalidConfidence => "置信度必须在 0 到 1 之间。".to_owned(),
        }
    }
}

fn field_label(field: &str) -> &str {
    match field {
        "dimension" => "评价维度",
        "dimensionName" => "维度名称",
        "message" => "问题描述",
        "suggestedFix" => "修改建议",
        "score" => "评分",
        "weight" => "权重",
        "reasoning" => "评分理由",
        "feedbackText" => "反馈文本",
        "planSummary" => "计划摘要",
        "reviewerType" => "评价者类型",
        "decision" => "评价决策",
        "severity" => "严重程度",
        "dimensionLayer" => "维度层级",
        "assetVersionStatus" => "资产版本状态",
        "licenseSourceType" => "版权来源类型",
        "commercialUseStatus" => "商用权限状态",
        "contentGuardStatus" => "内容安全状态",
        "contentRiskLevel" => "内容风险等级",
        _ => "字段",
    }
}

// ─────────────────────────────────────────────────────
// 通用校验
// ─────────────────────────────────────────────────────

fn validate_uuid(value: String, field: &'static str) -> Result<String, ReviewValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReviewValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(ReviewValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, ReviewValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReviewValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(ReviewValidationError::TooLong { field, max_length });
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(ReviewValidationError::ControlCharacters { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_dimension_field(
    value: String,
    field: &'static str,
) -> Result<String, ReviewValidationError> {
    normalize_required(value, field, 80)
}

fn validate_score(value: f64, field: &'static str) -> Result<f64, ReviewValidationError> {
    if value < 0.0 || value > 100.0 {
        return Err(ReviewValidationError::InvalidScore { field });
    }
    Ok(value)
}

fn validate_confidence(value: f64) -> Result<f64, ReviewValidationError> {
    if value < 0.0 || value > 1.0 {
        return Err(ReviewValidationError::InvalidConfidence);
    }
    Ok(value)
}

// ═══════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_from_score() {
        assert_eq!(
            ReviewDecision::from_overall_score(92.0),
            ReviewDecision::Accept
        );
        assert_eq!(
            ReviewDecision::from_overall_score(85.0),
            ReviewDecision::AcceptWithSuggestions
        );
        assert_eq!(
            ReviewDecision::from_overall_score(75.0),
            ReviewDecision::Revise
        );
        assert_eq!(
            ReviewDecision::from_overall_score(65.0),
            ReviewDecision::Regenerate
        );
        assert_eq!(
            ReviewDecision::from_overall_score(55.0),
            ReviewDecision::Block
        );
    }

    #[test]
    fn video_generation_gate_blocks_low_overall() {
        let gate = VideoGenerationGate::default();
        let report = ReviewReportRecord {
            id: "test-id".into(),
            project_id: "proj-id".into(),
            run_id: None,
            shot_id: None,
            asset_id: None,
            generation_attempt_id: None,
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: None,
            reviewer_provider: None,
            requirement_scores_json: r#"{"match": 85}"#.into(),
            visual_scores_json: "{}".into(),
            content_scores_json: r#"{"themeMatch": 90}"#.into(),
            commercial_scores_json: "{}".into(),
            technical_scores_json: r#"{"characterConsistency": 85}"#.into(),
            overall_score: 75.0,
            weighted_score: None,
            issues_json: "[]".into(),
            decision: ReviewDecision::Revise,
            confidence: None,
            source_task_id: None,
            review_version: 1,
            created_at: "2026-07-20T00:00:00Z".into(),
        };
        assert!(!gate.passes(&report));
    }

    #[test]
    fn video_generation_gate_passes_good_report() {
        let gate = VideoGenerationGate::default();
        let report = ReviewReportRecord {
            id: "test-id".into(),
            project_id: "proj-id".into(),
            run_id: None,
            shot_id: None,
            asset_id: None,
            generation_attempt_id: None,
            reviewer_type: ReviewerType::Auto,
            reviewer_agent_version: None,
            reviewer_provider: None,
            requirement_scores_json: r#"{"match": 88}"#.into(),
            visual_scores_json: "{}".into(),
            content_scores_json: r#"{"themeMatch": 90}"#.into(),
            commercial_scores_json: "{}".into(),
            technical_scores_json: r#"{"characterConsistency": 85}"#.into(),
            overall_score: 86.0,
            weighted_score: None,
            issues_json: "[]".into(),
            decision: ReviewDecision::AcceptWithSuggestions,
            confidence: Some(0.85),
            source_task_id: None,
            review_version: 1,
            created_at: "2026-07-20T00:00:00Z".into(),
        };
        assert!(gate.passes(&report));
    }

    #[test]
    fn review_issue_validation() {
        let issue = ReviewIssue::try_new(
            "character_consistency".into(),
            "medium".into(),
            "主角脸型与角色参考图略有差异".into(),
            "强化角色参考，加入正脸特征约束".into(),
        )
        .unwrap();
        assert_eq!(issue.dimension, "character_consistency");
        assert_eq!(issue.severity, IssueSeverity::Medium);
    }

    #[test]
    fn parse_all_enums() {
        assert_eq!(ReviewerType::parse("auto").unwrap(), ReviewerType::Auto);
        assert_eq!(
            ReviewDecision::parse("regenerate").unwrap(),
            ReviewDecision::Regenerate
        );
        assert_eq!(IssueSeverity::parse("high").unwrap(), IssueSeverity::High);
        assert_eq!(
            ReviewDimensionLayer::parse("technical").unwrap(),
            ReviewDimensionLayer::Technical
        );
    }
}
