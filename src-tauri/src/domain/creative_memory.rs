use serde::{Deserialize, Serialize};
use thiserror::Error;

const SUMMARY_MAX_LENGTH: usize = 500;

/// 记忆类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    StylePreference,
    NegativePreference,
    BrandRule,
    WorkflowHabit,
    PromptPattern,
    ReviewHistory,
}

impl MemoryType {
    pub fn parse(value: &str) -> Result<Self, MemoryValidationError> {
        match value {
            "style_preference" => Ok(Self::StylePreference),
            "negative_preference" => Ok(Self::NegativePreference),
            "brand_rule" => Ok(Self::BrandRule),
            "workflow_habit" => Ok(Self::WorkflowHabit),
            "prompt_pattern" => Ok(Self::PromptPattern),
            "review_history" => Ok(Self::ReviewHistory),
            _ => Err(MemoryValidationError::InvalidChoice {
                field: "memoryType",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::StylePreference => "style_preference",
            Self::NegativePreference => "negative_preference",
            Self::BrandRule => "brand_rule",
            Self::WorkflowHabit => "workflow_habit",
            Self::PromptPattern => "prompt_pattern",
            Self::ReviewHistory => "review_history",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StylePreference => "风格偏好",
            Self::NegativePreference => "不喜欢的风格",
            Self::BrandRule => "品牌规范",
            Self::WorkflowHabit => "工作流习惯",
            Self::PromptPattern => "Prompt 模式",
            Self::ReviewHistory => "审核历史",
        }
    }
}

/// 记忆作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    Project,
    Brand,
}

impl MemoryScope {
    pub fn parse(value: &str) -> Result<Self, MemoryValidationError> {
        match value {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "brand" => Ok(Self::Brand),
            _ => Err(MemoryValidationError::InvalidChoice { field: "scope" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Brand => "brand",
        }
    }
}

/// 记忆来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    ExplicitSave,
    ConfirmedPattern,
    ProjectTemplate,
}

impl MemorySource {
    pub fn parse(value: &str) -> Result<Self, MemoryValidationError> {
        match value {
            "explicit_save" => Ok(Self::ExplicitSave),
            "confirmed_pattern" => Ok(Self::ConfirmedPattern),
            "project_template" => Ok(Self::ProjectTemplate),
            _ => Err(MemoryValidationError::InvalidChoice { field: "source" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitSave => "explicit_save",
            Self::ConfirmedPattern => "confirmed_pattern",
            Self::ProjectTemplate => "project_template",
        }
    }
}

/// 记忆状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Paused,
    Archived,
}

impl MemoryStatus {
    pub fn parse(value: &str) -> Result<Self, MemoryValidationError> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "archived" => Ok(Self::Archived),
            _ => Err(MemoryValidationError::InvalidChoice { field: "status" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Archived => "archived",
        }
    }
}

/// 记忆事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEventType {
    Created,
    Updated,
    Paused,
    Resumed,
    Archived,
    Deleted,
    Confirmed,
}

impl MemoryEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Paused => "paused",
            Self::Resumed => "resumed",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
            Self::Confirmed => "confirmed",
        }
    }
}

/// 创意记忆记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeMemoryRecord {
    pub id: String,
    pub memory_type: MemoryType,
    pub scope: MemoryScope,
    pub scope_ref_id: Option<String>,
    pub content_json: String,
    pub summary: String,
    pub source: MemorySource,
    pub source_ref_id: Option<String>,
    pub status: MemoryStatus,
    pub confidence: f64,
    pub confirm_count: i64,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建记忆的草稿。
#[derive(Debug, Clone)]
pub struct CreativeMemoryDraft {
    pub memory_type: MemoryType,
    pub scope: MemoryScope,
    pub scope_ref_id: Option<String>,
    pub content_json: String,
    pub summary: String,
    pub source: MemorySource,
    pub source_ref_id: Option<String>,
    pub created_by: String,
}

impl CreativeMemoryDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        memory_type: String,
        scope: String,
        scope_ref_id: Option<String>,
        content_json: String,
        summary: String,
        source: String,
        source_ref_id: Option<String>,
        created_by: String,
    ) -> Result<Self, MemoryValidationError> {
        let memory_type = MemoryType::parse(memory_type.trim())?;
        let scope = MemoryScope::parse(scope.trim())?;
        let source = MemorySource::parse(source.trim())?;
        let summary = normalize_required(summary, "summary", SUMMARY_MAX_LENGTH)?;
        let content_json = validate_json_object(content_json, "contentJson")?;
        let created_by = validate_uuid(created_by, "createdBy")?;

        // scope=user 时 scope_ref_id 应为 None
        let scope_ref_id = if scope == MemoryScope::User {
            None
        } else {
            scope_ref_id
                .map(|id| validate_uuid(id, "scopeRefId"))
                .transpose()?
        };

        Ok(Self {
            memory_type,
            scope,
            scope_ref_id,
            content_json,
            summary,
            source,
            source_ref_id,
            created_by,
        })
    }
}

/// 创意记忆事件记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativeMemoryEventRecord {
    pub id: String,
    pub memory_id: String,
    pub event_type: MemoryEventType,
    pub event_detail: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// 按类型查询记忆的筛选条件。
#[derive(Debug, Clone)]
pub struct MemoryFilter {
    pub scope: Option<MemoryScope>,
    pub scope_ref_id: Option<String>,
    pub memory_type: Option<MemoryType>,
    pub status: Option<MemoryStatus>,
    pub limit: i64,
}

impl MemoryFilter {
    pub fn try_new(
        scope: Option<String>,
        scope_ref_id: Option<String>,
        memory_type: Option<String>,
        status: Option<String>,
        limit: Option<i64>,
    ) -> Result<Self, MemoryValidationError> {
        let scope = scope.map(|s| MemoryScope::parse(s.trim())).transpose()?;
        let memory_type = memory_type
            .map(|s| MemoryType::parse(s.trim()))
            .transpose()?;
        let status = status.map(|s| MemoryStatus::parse(s.trim())).transpose()?;
        let limit = limit.unwrap_or(50);
        if limit <= 0 || limit > 200 {
            return Err(MemoryValidationError::InvalidLimit);
        }
        Ok(Self {
            scope,
            scope_ref_id,
            memory_type,
            status,
            limit,
        })
    }
}

// ─────────────────────────────────────────────────────
// 验证错误
// ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryValidationError {
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
    #[error("{field} must be a valid JSON object")]
    InvalidJson { field: &'static str },
    #[error("limit must be between 1 and 200")]
    InvalidLimit,
}

impl MemoryValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field }
            | Self::InvalidChoice { field }
            | Self::InvalidUuid { field }
            | Self::InvalidJson { field } => Some(field),
            Self::InvalidLimit => Some("limit"),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", memory_field_label(field)),
            Self::TooLong { field, max_length } => {
                format!(
                    "{}不能超过 {max_length} 个字符。",
                    memory_field_label(field)
                )
            }
            Self::ControlCharacters { field } => {
                format!("{}包含不支持的控制字符。", memory_field_label(field))
            }
            Self::InvalidChoice { field } => format!("{}无效。", memory_field_label(field)),
            Self::InvalidUuid { field } => format!("{}格式无效。", memory_field_label(field)),
            Self::InvalidJson { field } => {
                format!("{}必须是合法的 JSON 对象。", memory_field_label(field))
            }
            Self::InvalidLimit => "查询数量限制必须在 1 到 200 之间。".to_owned(),
        }
    }
}

fn memory_field_label(field: &str) -> &str {
    match field {
        "memoryType" => "记忆类型",
        "scope" => "作用域",
        "scopeRefId" => "作用域引用",
        "contentJson" => "记忆内容",
        "summary" => "摘要",
        "source" => "来源",
        "createdBy" => "创建者",
        "status" => "状态",
        _ => "字段",
    }
}

// ─────────────────────────────────────────────────────
// 校验工具
// ─────────────────────────────────────────────────────

fn validate_uuid(value: String, field: &'static str) -> Result<String, MemoryValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MemoryValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(MemoryValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, MemoryValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MemoryValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(MemoryValidationError::TooLong { field, max_length });
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(MemoryValidationError::ControlCharacters { field });
    }
    Ok(trimmed.to_owned())
}

fn validate_json_object(
    value: String,
    field: &'static str,
) -> Result<String, MemoryValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MemoryValidationError::Required { field });
    }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(MemoryValidationError::InvalidJson { field });
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|_| MemoryValidationError::InvalidJson { field })?;
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn creates_valid_draft() {
        let draft = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            r#"{"color_palette": ["warm", "muted"]}"#.into(),
            "偏好暖色调和低饱和度".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();
        assert_eq!(draft.memory_type, MemoryType::StylePreference);
        assert_eq!(draft.scope, MemoryScope::User);
        assert_eq!(draft.scope_ref_id, None);
    }

    #[test]
    fn rejects_empty_summary() {
        let err = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            r#"{"key": "value"}"#.into(),
            "   ".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MemoryValidationError::Required { field: "summary" }
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        let err = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            "not json".into(),
            "summary".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MemoryValidationError::InvalidJson {
                field: "contentJson"
            }
        ));
    }

    #[test]
    fn rejects_array_json() {
        let err = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            None,
            "[1, 2, 3]".into(),
            "summary".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MemoryValidationError::InvalidJson {
                field: "contentJson"
            }
        ));
    }

    #[test]
    fn clears_scope_ref_for_user_scope() {
        let draft = CreativeMemoryDraft::try_new(
            "style_preference".into(),
            "user".into(),
            Some("should-be-cleared".into()),
            r#"{"key": "value"}"#.into(),
            "summary".into(),
            "explicit_save".into(),
            None,
            SAMPLE_UUID.into(),
        )
        .unwrap();
        assert_eq!(draft.scope_ref_id, None);
    }

    #[test]
    fn parses_all_memory_types() {
        assert_eq!(
            MemoryType::parse("style_preference").unwrap(),
            MemoryType::StylePreference
        );
        assert_eq!(
            MemoryType::parse("negative_preference").unwrap(),
            MemoryType::NegativePreference
        );
        assert_eq!(
            MemoryType::parse("brand_rule").unwrap(),
            MemoryType::BrandRule
        );
        assert_eq!(
            MemoryType::parse("workflow_habit").unwrap(),
            MemoryType::WorkflowHabit
        );
        assert_eq!(
            MemoryType::parse("prompt_pattern").unwrap(),
            MemoryType::PromptPattern
        );
        assert_eq!(
            MemoryType::parse("review_history").unwrap(),
            MemoryType::ReviewHistory
        );
    }

    #[test]
    fn memory_filter_with_defaults() {
        let filter = MemoryFilter::try_new(None, None, None, None, None).unwrap();
        assert_eq!(filter.limit, 50);
        assert_eq!(filter.scope, None);
    }

    #[test]
    fn rejects_invalid_filter_limit() {
        assert!(MemoryFilter::try_new(None, None, None, None, Some(0)).is_err());
        assert!(MemoryFilter::try_new(None, None, None, None, Some(201)).is_err());
    }
}
