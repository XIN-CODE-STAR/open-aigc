use serde::Serialize;
use thiserror::Error;

const CONTEXT_REF_MAX_LENGTH: usize = 200;
const NOTES_MAX_LENGTH: usize = 500;

/// 资源关联的上下文类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssociationContextKind {
    TeachingResource,
    ProjectAttachment,
    GenerationOutput,
}

impl AssociationContextKind {
    pub fn parse(value: &str) -> Result<Self, ResourceValidationError> {
        match value {
            "teaching-resource" => Ok(Self::TeachingResource),
            "project-attachment" => Ok(Self::ProjectAttachment),
            "generation-output" => Ok(Self::GenerationOutput),
            _ => Err(ResourceValidationError::InvalidChoice {
                field: "contextKind",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TeachingResource => "teaching-resource",
            Self::ProjectAttachment => "project-attachment",
            Self::GenerationOutput => "generation-output",
        }
    }
}

/// 资产在上下文中的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssociationRole {
    Source,
    Result,
    Reference,
    Attachment,
}

impl AssociationRole {
    pub fn parse(value: &str) -> Result<Self, ResourceValidationError> {
        match value {
            "source" => Ok(Self::Source),
            "result" => Ok(Self::Result),
            "reference" => Ok(Self::Reference),
            "attachment" => Ok(Self::Attachment),
            _ => Err(ResourceValidationError::InvalidChoice { field: "role" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Result => "result",
            Self::Reference => "reference",
            Self::Attachment => "attachment",
        }
    }
}

/// 查询资源关联的筛选条件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationFilter {
    pub asset_id: Option<String>,
    pub context_kind: Option<AssociationContextKind>,
    pub context_ref: Option<String>,
    pub role: Option<AssociationRole>,
    pub limit: i64,
}

impl AssociationFilter {
    pub fn try_new(
        asset_id: Option<String>,
        context_kind: Option<String>,
        context_ref: Option<String>,
        role: Option<String>,
        limit: Option<i64>,
    ) -> Result<Self, ResourceValidationError> {
        let asset_id = asset_id
            .map(|value| validate_uuid(value, "assetId"))
            .transpose()?;
        let context_kind = context_kind
            .map(|value| AssociationContextKind::parse(value.trim()))
            .transpose()?;
        let context_ref = context_ref.map(validate_context_ref).transpose()?;
        let role = role
            .map(|value| AssociationRole::parse(value.trim()))
            .transpose()?;
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
        if limit <= 0 || limit > MAX_LIST_LIMIT {
            return Err(ResourceValidationError::InvalidLimit);
        }
        Ok(Self {
            asset_id,
            context_kind,
            context_ref,
            role,
            limit,
        })
    }
}

const DEFAULT_LIST_LIMIT: i64 = 500;
const MAX_LIST_LIMIT: i64 = 1_000;

/// 新建关联的草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationDraft {
    pub asset_id: String,
    pub context_kind: AssociationContextKind,
    pub context_ref: String,
    pub role: AssociationRole,
    pub notes: String,
}

impl AssociationDraft {
    pub fn try_new(
        asset_id: String,
        context_kind: String,
        context_ref: String,
        role: String,
        notes: Option<String>,
    ) -> Result<Self, ResourceValidationError> {
        let asset_id = validate_uuid(asset_id, "assetId")?;
        let context_kind = AssociationContextKind::parse(context_kind.trim())?;
        let context_ref = validate_context_ref(context_ref)?;
        let role = AssociationRole::parse(role.trim())?;
        let notes = notes.unwrap_or_default();
        if notes.chars().count() > NOTES_MAX_LENGTH {
            return Err(ResourceValidationError::TooLong {
                field: "notes",
                max_length: NOTES_MAX_LENGTH,
            });
        }
        Ok(Self {
            asset_id,
            context_kind,
            context_ref,
            role,
            notes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssociationRecord {
    pub id: String,
    pub asset_id: String,
    pub context_kind: AssociationContextKind,
    pub context_ref: String,
    pub role: AssociationRole,
    pub notes: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourceValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} is invalid")]
    InvalidChoice { field: &'static str },
    #[error("{field} is not a valid uuid")]
    InvalidUuid { field: &'static str },
    #[error("limit must be between 1 and 1000")]
    InvalidLimit,
}

impl ResourceValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::InvalidChoice { field }
            | Self::InvalidUuid { field } => Some(field),
            Self::InvalidLimit => Some("limit"),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Required { field } => format!("{}不能为空。", label(field)),
            Self::TooLong { field, max_length } => {
                format!("{}不能超过 {max_length} 个字符。", label(field))
            }
            Self::InvalidChoice { field } => format!("{}无效。", label(field)),
            Self::InvalidUuid { field } => format!("{}格式无效。", label(field)),
            Self::InvalidLimit => "查询数量限制必须在 1 到 1000 之间。".to_owned(),
        }
    }
}

fn label(field: &str) -> &str {
    match field {
        "assetId" => "资产标识",
        "contextKind" => "上下文类型",
        "contextRef" => "上下文引用",
        "role" => "角色",
        "notes" => "备注",
        _ => "字段",
    }
}

fn validate_uuid(value: String, field: &'static str) -> Result<String, ResourceValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ResourceValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(ResourceValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn validate_context_ref(value: String) -> Result<String, ResourceValidationError> {
    let trimmed = value.trim();
    if trimmed.chars().count() > CONTEXT_REF_MAX_LENGTH {
        return Err(ResourceValidationError::TooLong {
            field: "contextRef",
            max_length: CONTEXT_REF_MAX_LENGTH,
        });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn parses_known_enums() {
        assert_eq!(
            AssociationContextKind::parse("teaching-resource").unwrap(),
            AssociationContextKind::TeachingResource
        );
        assert_eq!(
            AssociationRole::parse("source").unwrap(),
            AssociationRole::Source
        );
        assert!(AssociationContextKind::parse("unknown").is_err());
    }

    #[test]
    fn rejects_invalid_asset_id() {
        assert!(matches!(
            AssociationDraft::try_new(
                "not-a-uuid".to_owned(),
                "teaching-resource".to_owned(),
                String::new(),
                "source".to_owned(),
                None,
            ),
            Err(ResourceValidationError::InvalidUuid { field: "assetId" })
        ));
    }

    #[test]
    fn accepts_valid_draft() {
        let draft = AssociationDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "teaching-resource".to_owned(),
            "classroom:abc".to_owned(),
            "source".to_owned(),
            None,
        )
        .unwrap();
        assert_eq!(draft.asset_id, SAMPLE_UUID);
        assert_eq!(draft.context_kind, AssociationContextKind::TeachingResource);
    }

    #[test]
    fn rejects_invalid_filter_limit() {
        assert!(matches!(
            AssociationFilter::try_new(None, None, None, None, Some(0)),
            Err(ResourceValidationError::InvalidLimit)
        ));
    }
}
