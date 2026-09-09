use serde::Serialize;
use thiserror::Error;

const WORKSPACE_NAME_MAX_LENGTH: usize = 80;
const TEACHER_NAME_MAX_LENGTH: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorkspace {
    pub workspace_name: String,
    pub teacher_name: String,
}

impl NewWorkspace {
    pub fn try_new(
        workspace_name: impl Into<String>,
        teacher_name: impl Into<String>,
    ) -> Result<Self, WorkspaceValidationError> {
        let workspace_name = normalize_name(
            workspace_name.into(),
            "workspaceName",
            WORKSPACE_NAME_MAX_LENGTH,
        )?;
        let teacher_name =
            normalize_name(teacher_name.into(), "teacherName", TEACHER_NAME_MAX_LENGTH)?;

        Ok(Self {
            workspace_name,
            teacher_name,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfile {
    pub workspace_id: String,
    pub workspace_name: String,
    pub teacher_id: String,
    pub teacher_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseHealth {
    pub schema_version: i32,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub foreign_keys_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStorageHealth {
    pub directories_ready: bool,
    pub writable: bool,
    pub manifest_ready: bool,
    pub asset_count: i64,
    pub total_bytes: i64,
    pub missing_asset_count: i64,
    pub checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    pub initialized: bool,
    pub workspace: Option<WorkspaceProfile>,
    pub database: DatabaseHealth,
    pub storage: ManagedStorageHealth,
}

/// 重命名工作空间时的内部返回类型：
/// - `Updated`：更新成功，返回最新 profile。
/// - `NotInitialized`：当前未初始化（无 singleton 行）。
///
/// 之所以不直接用 `PersistenceError::NotFound`，是因为 AppError 暂未提供
/// 通用 NotFound 变体；本枚举与 `initialize` 的 `AlreadyInitialized` 行为一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateWorkspaceNameResult {
    Updated(WorkspaceProfile),
    NotInitialized,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} contains unsupported control characters")]
    ControlCharacters { field: &'static str },
}

impl WorkspaceValidationError {
    pub fn field(&self) -> &'static str {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field } => field,
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
        }
    }
}

fn normalize_name(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, WorkspaceValidationError> {
    let value = value.trim();

    if value.is_empty() {
        return Err(WorkspaceValidationError::Required { field });
    }

    if value.chars().count() > max_length {
        return Err(WorkspaceValidationError::TooLong { field, max_length });
    }

    if value.chars().any(char::is_control) {
        return Err(WorkspaceValidationError::ControlCharacters { field });
    }

    Ok(value.to_owned())
}

fn field_label(field: &str) -> &str {
    match field {
        "workspaceName" => "工作空间名称",
        "teacherName" => "教师姓名",
        _ => "字段",
    }
}

#[cfg(test)]
mod tests {
    use super::{NewWorkspace, WorkspaceValidationError};

    #[test]
    fn trims_valid_names() {
        let workspace = NewWorkspace::try_new("  春季课程  ", "  王老师 ").unwrap();

        assert_eq!(workspace.workspace_name, "春季课程");
        assert_eq!(workspace.teacher_name, "王老师");
    }

    #[test]
    fn rejects_empty_and_control_character_names() {
        assert_eq!(
            NewWorkspace::try_new("   ", "王老师").unwrap_err(),
            WorkspaceValidationError::Required {
                field: "workspaceName"
            }
        );
        assert_eq!(
            NewWorkspace::try_new("春季课程", "王\n老师").unwrap_err(),
            WorkspaceValidationError::ControlCharacters {
                field: "teacherName"
            }
        );
    }
}
