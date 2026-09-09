use serde::{Deserialize, Serialize};
use thiserror::Error;

const WORKSPACE_NAME_MAX_LENGTH: usize = 80;
const TEACHER_NAME_MAX_LENGTH: usize = 60;
const MANIFEST_NOTE_MAX_LENGTH: usize = 500;

/// 备份归档内嵌的 manifest.json 描述。记录备份来源、schema 版本、资产数量等元信息。
/// 反序列化时严格校验字段，避免读取来路不明的备份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    /// 备份格式版本，当前固定为 1。
    pub format_version: i32,
    /// 备份来源工作空间名称。
    pub workspace_name: String,
    /// 备份来源教师姓名。
    pub teacher_name: String,
    /// 备份时刻工作空间的 schema 版本。
    pub schema_version: i32,
    /// 备份时刻 manifest 中未软删的资产数量。
    pub asset_count: i64,
    /// 备份时刻受管文件总字节数。
    pub total_bytes: i64,
    /// 备份生成时刻（RFC3339）。
    pub created_at: String,
    /// 用户输入的备注（可选）。
    pub note: Option<String>,
}

impl BackupManifest {
    pub const CURRENT_FORMAT_VERSION: i32 = 1;
}

/// 创建备份时的输入草稿。在 service 层校验后传给 adapter。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupDraft {
    pub workspace_name: String,
    pub teacher_name: String,
    pub schema_version: i32,
    pub asset_count: i64,
    pub total_bytes: i64,
    pub note: Option<String>,
}

impl BackupDraft {
    pub fn try_new(
        workspace_name: String,
        teacher_name: String,
        schema_version: i32,
        asset_count: i64,
        total_bytes: i64,
        note: Option<String>,
    ) -> Result<Self, BackupValidationError> {
        let workspace_name =
            normalize_required(workspace_name, "workspaceName", WORKSPACE_NAME_MAX_LENGTH)?;
        let teacher_name =
            normalize_required(teacher_name, "teacherName", TEACHER_NAME_MAX_LENGTH)?;
        if schema_version <= 0 {
            return Err(BackupValidationError::InvalidSchemaVersion);
        }
        if asset_count < 0 {
            return Err(BackupValidationError::InvalidAssetCount);
        }
        if total_bytes < 0 {
            return Err(BackupValidationError::InvalidTotalBytes);
        }
        let note = normalize_optional(note, MANIFEST_NOTE_MAX_LENGTH)?;
        Ok(Self {
            workspace_name,
            teacher_name,
            schema_version,
            asset_count,
            total_bytes,
            note,
        })
    }

    pub fn into_manifest(self, created_at: String) -> BackupManifest {
        BackupManifest {
            format_version: BackupManifest::CURRENT_FORMAT_VERSION,
            workspace_name: self.workspace_name,
            teacher_name: self.teacher_name,
            schema_version: self.schema_version,
            asset_count: self.asset_count,
            total_bytes: self.total_bytes,
            created_at,
            note: self.note,
        }
    }
}

/// 创建备份后的汇总信息。返回给前端展示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    /// 备份归档绝对路径。
    pub archive_path: String,
    /// 归档字节数。
    pub archive_size: i64,
    /// 写入归档的受管文件数。
    pub managed_file_count: i64,
    /// 写入归档的受管文件总字节数（可能与 manifest 中的 total_bytes 不同，反映实际打包的文件）。
    pub managed_total_bytes: i64,
    /// 备份时刻。
    pub created_at: String,
}

/// 从备份归档读取的预览信息。不修改任何本地状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    /// 备份归档绝对路径。
    pub archive_path: String,
    /// 归档内的 manifest。
    pub manifest: BackupManifest,
    /// 归档内包含的受管文件数。
    pub managed_file_count: i64,
    /// 归档内受管文件总字节数。
    pub managed_total_bytes: i64,
}

/// 恢复完成后的汇总信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    /// 使用的备份归档路径。
    pub archive_path: String,
    /// 恢复前自动创建的安全备份路径（若失败则为 None）。
    pub safety_backup_path: Option<String>,
    /// 恢复的受管文件数。
    pub restored_file_count: i64,
    /// 恢复的受管文件总字节数。
    pub restored_total_bytes: i64,
    /// 恢复时刻。
    pub restored_at: String,
    /// 是否需要重启应用以让所有服务重新加载。
    pub requires_restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BackupValidationError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} must be at most {max_length} characters")]
    TooLong {
        field: &'static str,
        max_length: usize,
    },
    #[error("{field} contains unsupported control characters")]
    ControlCharacters { field: &'static str },
    #[error("schemaVersion must be positive")]
    InvalidSchemaVersion,
    #[error("assetCount must not be negative")]
    InvalidAssetCount,
    #[error("totalBytes must not be negative")]
    InvalidTotalBytes,
    /// 备份格式版本不受支持。当前保留供前端层校验使用；adapter 使用 InvalidArchive。
    #[allow(dead_code)]
    #[error("backup format version is unsupported")]
    UnsupportedFormatVersion,
    /// 备份 manifest 损坏。当前保留供前端层校验使用；adapter 使用 InvalidArchive。
    #[allow(dead_code)]
    #[error("backup manifest is malformed")]
    MalformedManifest,
}

impl BackupValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field } => Some(field),
            Self::InvalidSchemaVersion => Some("schemaVersion"),
            Self::InvalidAssetCount => Some("assetCount"),
            Self::InvalidTotalBytes => Some("totalBytes"),
            Self::UnsupportedFormatVersion | Self::MalformedManifest => None,
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
            Self::InvalidSchemaVersion => "备份的 schema 版本无效。".to_owned(),
            Self::InvalidAssetCount => "备份的资产数量不能为负。".to_owned(),
            Self::InvalidTotalBytes => "备份的总字节数不能为负。".to_owned(),
            Self::UnsupportedFormatVersion => "备份格式版本不受支持。".to_owned(),
            Self::MalformedManifest => "备份归档内的 manifest 损坏或缺失。".to_owned(),
        }
    }
}

fn field_label(field: &str) -> &str {
    match field {
        "workspaceName" => "工作空间名称",
        "teacherName" => "教师姓名",
        "note" => "备注",
        _ => "字段",
    }
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, BackupValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(BackupValidationError::Required { field });
    }
    if value.chars().count() > max_length {
        return Err(BackupValidationError::TooLong { field, max_length });
    }
    if value.chars().any(char::is_control) {
        return Err(BackupValidationError::ControlCharacters { field });
    }
    Ok(value.to_owned())
}

fn normalize_optional(
    value: Option<String>,
    max_length: usize,
) -> Result<Option<String>, BackupValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_length {
        return Err(BackupValidationError::TooLong {
            field: "note",
            max_length,
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(BackupValidationError::ControlCharacters { field: "note" });
    }
    Ok(Some(trimmed.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn rejects_empty_workspace_name() {
        let error =
            BackupDraft::try_new("  ".to_owned(), "王老师".to_owned(), 7, 0, 0, None).unwrap_err();
        assert!(matches!(
            error,
            BackupValidationError::Required {
                field: "workspaceName"
            }
        ));
    }

    #[test]
    fn rejects_negative_asset_count() {
        let error = BackupDraft::try_new("测试".to_owned(), "王老师".to_owned(), 7, -1, 0, None)
            .unwrap_err();
        assert!(matches!(error, BackupValidationError::InvalidAssetCount));
    }

    #[test]
    fn accepts_valid_draft_and_converts_to_manifest() {
        let draft = BackupDraft::try_new(
            "春季课程".to_owned(),
            "王老师".to_owned(),
            6,
            12,
            1_024,
            Some("  期末归档  ".to_owned()),
        )
        .unwrap();
        assert_eq!(draft.workspace_name, "春季课程");
        assert_eq!(draft.note.as_deref(), Some("期末归档"));

        let manifest = draft.into_manifest("2026-07-16T00:00:00Z".to_owned());
        assert_eq!(manifest.format_version, 1);
        assert_eq!(manifest.workspace_name, "春季课程");
        assert_eq!(manifest.asset_count, 12);
        assert_eq!(manifest.note.as_deref(), Some("期末归档"));
    }

    #[test]
    fn rejects_malformed_manifest_with_missing_fields() {
        let json = r#"{"formatVersion":1,"workspaceName":"测试"}"#;
        let result: Result<BackupManifest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn parses_a_complete_manifest() {
        let json = r#"{
            "formatVersion": 1,
            "workspaceName": "春季课程",
            "teacherName": "王老师",
            "schemaVersion": 6,
            "assetCount": 5,
            "totalBytes": 2048,
            "createdAt": "2026-07-16T00:00:00Z",
            "note": "期末归档"
        }"#;
        let manifest: BackupManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.workspace_name, "春季课程");
        assert_eq!(manifest.asset_count, 5);
        assert_eq!(manifest.note.as_deref(), Some("期末归档"));
    }

    #[test]
    fn trims_note_whitespace() {
        let draft = BackupDraft::try_new(
            "测试".to_owned(),
            "王老师".to_owned(),
            6,
            0,
            0,
            Some("   ".to_owned()),
        )
        .unwrap();
        assert!(draft.note.is_none());
    }

    #[test]
    fn rejects_too_long_note() {
        let long_note = "x".repeat(MANIFEST_NOTE_MAX_LENGTH + 1);
        let error = BackupDraft::try_new(
            "测试".to_owned(),
            "王老师".to_owned(),
            6,
            0,
            0,
            Some(long_note),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BackupValidationError::TooLong {
                field: "note",
                max_length: MANIFEST_NOTE_MAX_LENGTH
            }
        ));
    }

    // 哑引用以避免 unused 警告；SAMPLE_UUID 用于在未来扩展时保持一致格式。
    #[test]
    fn sample_uuid_format_is_preserved() {
        assert_eq!(SAMPLE_UUID.len(), 36);
    }
}
