//! Edit Understanding Agent 领域模型。
//!
//! 定义用户自然语言反馈理解、结构化修改计划、Prompt Patch
//! 和参数补丁的类型、校验规则和序列化结构。
//! 按照 2026-07-20-creative-agent-critical-capability-decisions.md §5 设计。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────
// 常量
// ─────────────────────────────────────────────────────

const FEEDBACK_TEXT_MAX_LENGTH: usize = 2_000;
const INTENT_LABEL_MAX_LENGTH: usize = 80;
const PLAN_SUMMARY_MAX_LENGTH: usize = 500;
const AMBIGUOUS_REASON_MAX_LENGTH: usize = 1_000;

// ═══════════════════════════════════════════════════
// 编辑请求状态枚举
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditRequestStatus {
    Received,
    Analyzing,
    PlanReady,
    Applied,
    Rejected,
    Ambiguous,
}

impl EditRequestStatus {
    pub fn parse(value: &str) -> Result<Self, EditValidationError> {
        match value {
            "received" => Ok(Self::Received),
            "analyzing" => Ok(Self::Analyzing),
            "plan_ready" => Ok(Self::PlanReady),
            "applied" => Ok(Self::Applied),
            "rejected" => Ok(Self::Rejected),
            "ambiguous" => Ok(Self::Ambiguous),
            _ => Err(EditValidationError::InvalidChoice {
                field: "editRequestStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Analyzing => "analyzing",
            Self::PlanReady => "plan_ready",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::Ambiguous => "ambiguous",
        }
    }
}

// ═══════════════════════════════════════════════════
// 编辑上下文类型
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditContextType {
    GenerationResult,
    Storyboard,
    CharacterDesign,
    VisualSpec,
    Script,
    Deliverable,
}

impl EditContextType {
    pub fn parse(value: &str) -> Result<Self, EditValidationError> {
        match value {
            "generation_result" => Ok(Self::GenerationResult),
            "storyboard" => Ok(Self::Storyboard),
            "character_design" => Ok(Self::CharacterDesign),
            "visual_spec" => Ok(Self::VisualSpec),
            "script" => Ok(Self::Script),
            "deliverable" => Ok(Self::Deliverable),
            _ => Err(EditValidationError::InvalidChoice {
                field: "contextType",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GenerationResult => "generation_result",
            Self::Storyboard => "storyboard",
            Self::CharacterDesign => "character_design",
            Self::VisualSpec => "visual_spec",
            Self::Script => "script",
            Self::Deliverable => "deliverable",
        }
    }
}

// ═══════════════════════════════════════════════════
// 编辑操作类型
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditOperationType {
    StyleAdjustment,
    CharacterAdjustment,
    CompositionChange,
    LightingAdjustment,
    ColorGrading,
    CameraChange,
    ContentRevision,
    QualityImprovement,
    Regenerate,
}

impl EditOperationType {
    pub fn parse(value: &str) -> Result<Self, EditValidationError> {
        match value {
            "style_adjustment" => Ok(Self::StyleAdjustment),
            "character_adjustment" => Ok(Self::CharacterAdjustment),
            "composition_change" => Ok(Self::CompositionChange),
            "lighting_adjustment" => Ok(Self::LightingAdjustment),
            "color_grading" => Ok(Self::ColorGrading),
            "camera_change" => Ok(Self::CameraChange),
            "content_revision" => Ok(Self::ContentRevision),
            "quality_improvement" => Ok(Self::QualityImprovement),
            "regenerate" => Ok(Self::Regenerate),
            _ => Err(EditValidationError::InvalidChoice {
                field: "operationType",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::StyleAdjustment => "style_adjustment",
            Self::CharacterAdjustment => "character_adjustment",
            Self::CompositionChange => "composition_change",
            Self::LightingAdjustment => "lighting_adjustment",
            Self::ColorGrading => "color_grading",
            Self::CameraChange => "camera_change",
            Self::ContentRevision => "content_revision",
            Self::QualityImprovement => "quality_improvement",
            Self::Regenerate => "regenerate",
        }
    }
}

// ═══════════════════════════════════════════════════
// 编辑计划状态
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditPlanStatus {
    Draft,
    Ready,
    Executing,
    Executed,
    Rejected,
}

impl EditPlanStatus {
    pub fn parse(value: &str) -> Result<Self, EditValidationError> {
        match value {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "executing" => Ok(Self::Executing),
            "executed" => Ok(Self::Executed),
            "rejected" => Ok(Self::Rejected),
            _ => Err(EditValidationError::InvalidChoice {
                field: "editPlanStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Executing => "executing",
            Self::Executed => "executed",
            Self::Rejected => "rejected",
        }
    }
}

// ═══════════════════════════════════════════════════
// 影响与风险评估
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
}

impl ImpactLevel {
    pub fn parse(value: &str) -> Result<Self, EditValidationError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(EditValidationError::InvalidChoice {
                field: "impactLevel",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

// ═══════════════════════════════════════════════════
// 修改目标（targets_json 的 Rust 表示）
// ═══════════════════════════════════════════════════

/// 单个修改目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    pub id: String,
    pub operation: String,
}

// ═══════════════════════════════════════════════════
// Prompt Patch
// ═══════════════════════════════════════════════════

/// Prompt 补丁：描述对 Prompt 的增删改操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PromptPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<std::collections::HashMap<String, String>>,
}

// ═══════════════════════════════════════════════════
// 参数补丁
// ═══════════════════════════════════════════════════

/// 参数补丁：描述对生成参数的调整。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParameterPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    // 允许自定义扩展字段
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════
// 编辑请求记录
// ═══════════════════════════════════════════════════

/// 编辑请求记录（数据库行映射）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequestRecord {
    pub id: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub feedback_text: String,
    pub context_type: EditContextType,
    pub context_ref_id: Option<String>,
    pub source_review_id: Option<String>,
    pub intent_json: Option<String>,
    pub status: EditRequestStatus,
    pub ambiguous_reason: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

/// 编辑请求草稿（创建时使用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRequestDraft {
    pub project_id: String,
    pub run_id: Option<String>,
    pub feedback_text: String,
    pub context_type: EditContextType,
    pub context_ref_id: Option<String>,
    pub source_review_id: Option<String>,
    pub created_by: String,
}

impl EditRequestDraft {
    pub fn try_new(
        project_id: String,
        run_id: Option<String>,
        feedback_text: String,
        context_type: String,
        context_ref_id: Option<String>,
        source_review_id: Option<String>,
        created_by: String,
    ) -> Result<Self, EditValidationError> {
        let project_id = validate_uuid(project_id, "projectId")?;
        let run_id = run_id.map(|id| validate_uuid(id, "runId")).transpose()?;
        let feedback_text =
            normalize_required(feedback_text, "feedbackText", FEEDBACK_TEXT_MAX_LENGTH)?;
        let context_type = EditContextType::parse(context_type.trim())?;
        let context_ref_id = context_ref_id
            .map(|id| validate_uuid(id, "contextRefId"))
            .transpose()?;
        let source_review_id = source_review_id
            .map(|id| validate_uuid(id, "sourceReviewId"))
            .transpose()?;
        let created_by = validate_uuid(created_by, "createdBy")?;

        Ok(Self {
            project_id,
            run_id,
            feedback_text,
            context_type,
            context_ref_id,
            source_review_id,
            created_by,
        })
    }
}

// ═══════════════════════════════════════════════════
// 编辑计划记录
// ═══════════════════════════════════════════════════

/// 编辑计划记录（数据库行映射）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlanRecord {
    pub id: String,
    pub edit_request_id: String,
    pub project_id: String,
    pub plan_summary: String,
    pub operation_type: EditOperationType,
    pub scope: String,
    pub targets_json: String,
    pub prompt_patch_json: Option<String>,
    pub parameter_patch_json: Option<String>,
    pub reference_asset_patch_json: Option<String>,
    pub requires_regeneration: bool,
    pub requires_critic_rerun: bool,
    pub estimated_impact: Option<String>,
    pub risk_level: Option<String>,
    pub status: EditPlanStatus,
    pub execution_result_json: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub executed_at: Option<String>,
}

// ═══════════════════════════════════════════════════
// Edit Understanding Agent 输出结构（非持久化）
// ═══════════════════════════════════════════════════

/// Edit Understanding Agent 语义理解结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditUnderstandingResult {
    /// 用户原始反馈文本。
    pub feedback: String,
    /// 识别出的意图主标签。
    pub intent: String,
    /// 意图子标签（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_intent: Option<String>,
    /// 理解置信度 (0.0-1.0)。
    pub confidence: f64,
    /// 修改目标列表。
    pub targets: Vec<EditTarget>,
    /// Prompt 补丁。
    pub prompt_patch: Option<PromptPatch>,
    /// 参数补丁。
    pub parameter_patch: Option<ParameterPatch>,
    /// 是否需要重新走 Critic。
    pub requires_critic_rerun: bool,
    /// 风险等级。
    pub risk: ImpactLevel,
}

// ═══════════════════════════════════════════════════
// 常见反馈→参数映射（参考数据）
// ═══════════════════════════════════════════════════

/// 预定义的常见用户反馈到修改操作的映射。
/// 用于 Edit Understanding Agent 的快速匹配和上下文增强。
#[derive(Debug, Clone)]
pub struct FeedbackPattern {
    pub trigger_keywords: &'static [&'static str],
    pub intent: EditOperationType,
    pub prompt_add: &'static [&'static str],
    pub prompt_remove: &'static [&'static str],
}

const PREDEFINED_PATTERNS: &[FeedbackPattern] = &[
    // "太假了" → 增加真实感
    FeedbackPattern {
        trigger_keywords: &["太假", "不真实", "CG感", "像动画", "假人"],
        intent: EditOperationType::StyleAdjustment,
        prompt_add: &[
            "photorealistic",
            "natural texture",
            "real-world lighting",
            "organic imperfections",
        ],
        prompt_remove: &[
            "CG render",
            "perfect symmetry",
            "plastic skin",
            "over-polished",
        ],
    },
    // "不够高级/大气" → 提升空间尺度和质感
    FeedbackPattern {
        trigger_keywords: &["不够大气", "不够高级", "太小气", "小气"],
        intent: EditOperationType::CompositionChange,
        prompt_add: &[
            "grand scale",
            "wide angle lens",
            "cinematic depth",
            "negative space",
        ],
        prompt_remove: &["close-up framing", "shallow depth", "crowded composition"],
    },
    // "不像纪录片/央视风格" → 加强纪实风格
    FeedbackPattern {
        trigger_keywords: &["不像纪录片", "不是央视", "太网红", "不像纪实"],
        intent: EditOperationType::StyleAdjustment,
        prompt_add: &[
            "documentary cinematography",
            "natural handheld camera",
            "observational framing",
            "muted color grading",
        ],
        prompt_remove: &[
            "glamour lighting",
            "influencer style",
            "beauty filter",
            "fashion aesthetic",
        ],
    },
    // "人物不像创业者" → 调整人物特征
    FeedbackPattern {
        trigger_keywords: &["不像创业者", "太像模特", "太网红", "不真实"],
        intent: EditOperationType::CharacterAdjustment,
        prompt_add: &[
            "ordinary person",
            "natural expression",
            "workplace attire",
            "realistic age features",
        ],
        prompt_remove: &[
            "fashion model",
            "perfect skin",
            "studio lighting",
            "glamour pose",
        ],
    },
    // "色调太冷/太暖" → 调整色彩
    FeedbackPattern {
        trigger_keywords: &["太冷", "太暖", "颜色不对", "色调"],
        intent: EditOperationType::ColorGrading,
        prompt_add: &["balanced color temperature", "natural color palette"],
        prompt_remove: &["extreme color cast", "oversaturated"],
    },
    // "节奏太慢/太快" → 调整摄像机运动
    FeedbackPattern {
        trigger_keywords: &["节奏", "太慢", "太快", "拖沓", "赶"],
        intent: EditOperationType::CameraChange,
        prompt_add: &["dynamic pacing", "appropriate tempo"],
        prompt_remove: &["static camera", "rapid cuts"],
    },
];

impl FeedbackPattern {
    /// 根据用户反馈文本查找最匹配的模式。
    pub fn find_match(feedback: &str) -> Option<FeedbackPattern> {
        let feedback_lower = feedback.to_lowercase();
        for pattern in PREDEFINED_PATTERNS {
            for keyword in pattern.trigger_keywords.iter() {
                if feedback_lower.contains(keyword) {
                    return Some(pattern.clone());
                }
            }
        }
        None
    }
}

// ═══════════════════════════════════════════════════
// 验证错误
// ═══════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditValidationError {
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
    #[error("confidence must be between 0 and 1")]
    InvalidConfidence,
}

impl EditValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field }
            | Self::InvalidChoice { field }
            | Self::InvalidUuid { field } => Some(field),
            Self::InvalidConfidence => Some("confidence"),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", edit_field_label(field)),
            Self::TooLong { field, max_length } => {
                format!("{}不能超过 {max_length} 个字符。", edit_field_label(field))
            }
            Self::ControlCharacters { field } => {
                format!("{}包含不支持的控制字符。", edit_field_label(field))
            }
            Self::InvalidChoice { field } => format!("{}无效。", edit_field_label(field)),
            Self::InvalidUuid { field } => format!("{}格式无效。", edit_field_label(field)),
            Self::InvalidConfidence => "置信度必须在 0 到 1 之间。".to_owned(),
        }
    }
}

fn edit_field_label(field: &str) -> &str {
    match field {
        "projectId" => "项目标识",
        "runId" => "运行标识",
        "feedbackText" => "反馈文本",
        "contextType" => "上下文类型",
        "contextRefId" => "上下文引用",
        "sourceReviewId" => "审片引用",
        "createdBy" => "创建者",
        "planSummary" => "计划摘要",
        "editRequestStatus" => "编辑请求状态",
        "operationType" => "操作类型",
        "editPlanStatus" => "计划状态",
        "impactLevel" => "影响等级",
        _ => "字段",
    }
}

// ─────────────────────────────────────────────────────
// 通用校验
// ─────────────────────────────────────────────────────

fn validate_uuid(value: String, field: &'static str) -> Result<String, EditValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EditValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(EditValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, EditValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EditValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(EditValidationError::TooLong { field, max_length });
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(EditValidationError::ControlCharacters { field });
    }
    Ok(trimmed.to_owned())
}

fn validate_confidence(value: f64) -> Result<f64, EditValidationError> {
    if value < 0.0 || value > 1.0 {
        return Err(EditValidationError::InvalidConfidence);
    }
    Ok(value)
}

// ═══════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn creates_valid_edit_request_draft() {
        let draft = EditRequestDraft::try_new(
            SAMPLE_UUID.into(),
            None,
            "不要这么网红，人物更像真实创业者".into(),
            "generation_result".into(),
            None,
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();

        assert_eq!(draft.context_type, EditContextType::GenerationResult);
        assert_eq!(draft.feedback_text, "不要这么网红，人物更像真实创业者");
    }

    #[test]
    fn rejects_empty_feedback() {
        let error = EditRequestDraft::try_new(
            SAMPLE_UUID.into(),
            None,
            "   ".into(),
            "generation_result".into(),
            None,
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            EditValidationError::Required {
                field: "feedbackText"
            }
        ));
    }

    #[test]
    fn rejects_invalid_uuids() {
        assert!(EditRequestDraft::try_new(
            "bad".into(),
            None,
            "修改一下".into(),
            "generation_result".into(),
            None,
            None,
            SAMPLE_UUID.into(),
        )
        .is_err());
    }

    #[test]
    fn rejects_unknown_context_type() {
        assert!(EditRequestDraft::try_new(
            SAMPLE_UUID.into(),
            None,
            "修改一下".into(),
            "unknown_context".into(),
            None,
            None,
            SAMPLE_UUID.into(),
        )
        .is_err());
    }

    #[test]
    fn feedback_pattern_matching() {
        let result = FeedbackPattern::find_match("太假了，角色不像真实的人");
        assert!(result.is_some());
        let pattern = result.unwrap();
        assert_eq!(pattern.intent, EditOperationType::StyleAdjustment);

        let result = FeedbackPattern::find_match("不够大气，画面太小了");
        assert!(result.is_some());
        let pattern = result.unwrap();
        assert_eq!(pattern.intent, EditOperationType::CompositionChange);

        let result = FeedbackPattern::find_match("色调太冷了，暖一点");
        assert!(result.is_some());
        let pattern = result.unwrap();
        assert_eq!(pattern.intent, EditOperationType::ColorGrading);

        let no_match = FeedbackPattern::find_match("这个画面我很喜欢，不需要修改");
        assert!(no_match.is_none());
    }

    #[test]
    fn parse_all_enums() {
        assert_eq!(
            EditRequestStatus::parse("analyzing").unwrap(),
            EditRequestStatus::Analyzing
        );
        assert_eq!(
            EditContextType::parse("character_design").unwrap(),
            EditContextType::CharacterDesign
        );
        assert_eq!(
            EditOperationType::parse("color_grading").unwrap(),
            EditOperationType::ColorGrading
        );
        assert_eq!(
            EditPlanStatus::parse("executing").unwrap(),
            EditPlanStatus::Executing
        );
    }

    #[test]
    fn prompt_patch_serialization() {
        let patch = PromptPatch {
            add: Some(vec![
                "realistic founder".into(),
                "documentary portrait".into(),
            ]),
            remove: Some(vec!["fashion model".into(), "glamour lighting".into()]),
            replace: None,
        };

        let json = serde_json::to_string(&patch).unwrap();
        let deserialized: PromptPatch = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.add.unwrap().len(), 2);
        assert_eq!(deserialized.remove.unwrap().len(), 2);
    }
}
