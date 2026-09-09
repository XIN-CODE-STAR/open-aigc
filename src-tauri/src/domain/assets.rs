use serde::Serialize;
use thiserror::Error;

const DISPLAY_NAME_MAX_LENGTH: usize = 160;
const RELATIVE_PATH_MAX_LENGTH: usize = 500;
const MIME_TYPE_MAX_LENGTH: usize = 120;
const METADATA_JSON_MAX_LENGTH: usize = 8_000;
const SEARCH_MAX_LENGTH: usize = 80;
const DEFAULT_ASSET_LIST_LIMIT: i64 = 500;
const MAX_ASSET_LIST_LIMIT: i64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageNamespace {
    Workspace,
    Generation,
    TeachingResource,
    System,
}

impl StorageNamespace {
    pub fn parse(value: &str) -> Result<Self, AssetValidationError> {
        match value {
            "workspace" => Ok(Self::Workspace),
            "generation" => Ok(Self::Generation),
            "teaching-resource" => Ok(Self::TeachingResource),
            "system" => Ok(Self::System),
            _ => Err(AssetValidationError::InvalidChoice {
                field: "storageNamespace",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Generation => "generation",
            Self::TeachingResource => "teaching-resource",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Other,
}

impl AssetKind {
    pub fn parse(value: &str) -> Result<Self, AssetValidationError> {
        match value {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "document" => Ok(Self::Document),
            "archive" => Ok(Self::Archive),
            "other" => Ok(Self::Other),
            _ => Err(AssetValidationError::InvalidChoice { field: "assetKind" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Archive => "archive",
            Self::Other => "other",
        }
    }

    /// 根据扩展名和 MIME 类型推断资产类型。优先使用 MIME，扩展名作为兜底。
    pub fn infer(mime_type: Option<&str>, file_name: &str) -> Self {
        if let Some(mime) = mime_type.map(str::to_ascii_lowercase).as_deref() {
            if mime.starts_with("image/") {
                return Self::Image;
            }
            if mime.starts_with("video/") {
                return Self::Video;
            }
            if mime.starts_with("audio/") {
                return Self::Audio;
            }
            if mime == "application/zip"
                || mime == "application/x-tar"
                || mime == "application/gzip"
                || mime == "application/x-7z-compressed"
                || mime == "application/x-rar-compressed"
            {
                return Self::Archive;
            }
            if mime == "application/pdf"
                || mime.starts_with("text/")
                || mime == "application/msword"
                || mime.starts_with("application/vnd.openxmlformats-officedocument")
                || mime.starts_with("application/vnd.ms-")
                || mime.starts_with("application/vnd.google-apps")
            {
                return Self::Document;
            }
        }

        let extension = file_name
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tiff" | "ico" => Self::Image,
            "mp4" | "mov" | "avi" | "mkv" | "webm" | "flv" | "wmv" | "m4v" => Self::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => Self::Audio,
            "zip" | "tar" | "gz" | "7z" | "rar" | "bz2" | "xz" => Self::Archive,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv"
            | "json" | "xml" | "html" | "rtf" => Self::Document,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrityStatus {
    Unverified,
    Valid,
    Missing,
    Corrupt,
    Quarantined,
}

impl IntegrityStatus {
    pub fn parse(value: &str) -> Result<Self, AssetValidationError> {
        match value {
            "unverified" => Ok(Self::Unverified),
            "valid" => Ok(Self::Valid),
            "missing" => Ok(Self::Missing),
            "corrupt" => Ok(Self::Corrupt),
            "quarantined" => Ok(Self::Quarantined),
            _ => Err(AssetValidationError::InvalidChoice {
                field: "integrityStatus",
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Valid => "valid",
            Self::Missing => "missing",
            Self::Corrupt => "corrupt",
            Self::Quarantined => "quarantined",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFilter {
    pub search: Option<String>,
    pub storage_namespace: Option<StorageNamespace>,
    pub asset_kind: Option<AssetKind>,
    pub integrity_status: Option<IntegrityStatus>,
    pub limit: i64,
}

impl AssetFilter {
    pub fn try_new(
        search: Option<String>,
        storage_namespace: Option<String>,
        asset_kind: Option<String>,
        integrity_status: Option<String>,
        limit: Option<i64>,
    ) -> Result<Self, AssetValidationError> {
        let search = normalize_optional(search, "search", SEARCH_MAX_LENGTH)?;
        let storage_namespace = storage_namespace
            .map(|value| StorageNamespace::parse(value.trim()))
            .transpose()?;
        let asset_kind = asset_kind
            .map(|value| AssetKind::parse(value.trim()))
            .transpose()?;
        let integrity_status = integrity_status
            .map(|value| IntegrityStatus::parse(value.trim()))
            .transpose()?;
        let limit = limit.unwrap_or(DEFAULT_ASSET_LIST_LIMIT);
        if limit <= 0 || limit > MAX_ASSET_LIST_LIMIT {
            return Err(AssetValidationError::InvalidLimit);
        }

        Ok(Self {
            search,
            storage_namespace,
            asset_kind,
            integrity_status,
            limit,
        })
    }
}

/// 已校验的导入条目。由文件导入服务在 staging 完成后构造，用于写入 manifest。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDraft {
    pub storage_namespace: StorageNamespace,
    pub asset_kind: AssetKind,
    pub display_name: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub mime_type: Option<String>,
    pub metadata_json: String,
}

impl AssetDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        storage_namespace: String,
        asset_kind: AssetKind,
        display_name: String,
        relative_path: String,
        size_bytes: i64,
        sha256: Option<String>,
        mime_type: Option<String>,
        metadata_json: Option<String>,
    ) -> Result<Self, AssetValidationError> {
        let storage_namespace = StorageNamespace::parse(storage_namespace.trim())?;
        let display_name =
            normalize_required(display_name, "displayName", DISPLAY_NAME_MAX_LENGTH)?;
        let relative_path = normalize_relative_path(relative_path)?;
        if size_bytes < 0 {
            return Err(AssetValidationError::InvalidSize);
        }
        let sha256 = match sha256 {
            Some(value) => {
                let value = value.trim().to_ascii_lowercase();
                if value.is_empty() {
                    None
                } else {
                    validate_sha256(&value)?;
                    Some(value)
                }
            }
            None => None,
        };
        let mime_type = normalize_optional(mime_type, "mimeType", MIME_TYPE_MAX_LENGTH)?;
        let metadata_json = metadata_json.unwrap_or_else(|| "{}".to_owned());
        if metadata_json.len() > METADATA_JSON_MAX_LENGTH {
            return Err(AssetValidationError::TooLong {
                field: "metadataJson",
                max_length: METADATA_JSON_MAX_LENGTH,
            });
        }
        if !json_is_valid(&metadata_json) {
            return Err(AssetValidationError::InvalidMetadata);
        }

        Ok(Self {
            storage_namespace,
            asset_kind,
            display_name,
            relative_path,
            size_bytes,
            sha256,
            mime_type,
            metadata_json,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRecord {
    pub id: String,
    pub storage_namespace: StorageNamespace,
    pub asset_kind: AssetKind,
    pub display_name: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub mime_type: Option<String>,
    pub integrity_status: IntegrityStatus,
    pub metadata_json: String,
    pub origin_device_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportOutcome {
    pub asset: AssetRecord,
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportSkipped {
    pub source_path: String,
    pub reason: String,
    pub existing_asset_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportFailure {
    pub source_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImportSummary {
    pub imported: Vec<AssetImportOutcome>,
    pub skipped: Vec<AssetImportSkipped>,
    pub failures: Vec<AssetImportFailure>,
}

impl AssetImportSummary {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.imported.is_empty() && self.skipped.is_empty() && self.failures.is_empty()
    }
}

/// 文件完整性复检结果。对每条 manifest 记录检查受管文件是否存在，
/// 有 SHA-256 的资产会重算哈希对比，状态更新为 valid / missing / corrupt。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetReverificationSummary {
    pub checked: u64,
    pub valid: u64,
    pub missing: u64,
    pub corrupt: u64,
    pub quarantined: u64,
}

impl AssetReverificationSummary {
    pub fn empty() -> Self {
        Self {
            checked: 0,
            valid: 0,
            missing: 0,
            corrupt: 0,
            quarantined: 0,
        }
    }

    pub fn record(&mut self, status: IntegrityStatus) {
        self.checked += 1;
        match status {
            IntegrityStatus::Valid => self.valid += 1,
            IntegrityStatus::Missing => self.missing += 1,
            IntegrityStatus::Corrupt => self.corrupt += 1,
            IntegrityStatus::Quarantined => self.quarantined += 1,
            IntegrityStatus::Unverified => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssetValidationError {
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
    #[error("{field} is not a valid managed relative path")]
    InvalidPath { field: &'static str },
    #[error("sizeBytes must not be negative")]
    InvalidSize,
    #[error("metadataJson must be a valid JSON object")]
    InvalidMetadata,
    #[error("sha256 must be a 64-character lowercase hex string")]
    InvalidHash,
    #[error("limit must be between 1 and 1000")]
    InvalidLimit,
}

impl AssetValidationError {
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::Required { field }
            | Self::TooLong { field, .. }
            | Self::ControlCharacters { field }
            | Self::InvalidChoice { field }
            | Self::InvalidPath { field } => Some(field),
            Self::InvalidSize => Some("sizeBytes"),
            Self::InvalidMetadata => Some("metadataJson"),
            Self::InvalidHash => Some("sha256"),
            Self::InvalidLimit => Some("limit"),
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
            Self::InvalidPath { field } => format!("{}不是有效的受管路径。", field_label(field)),
            Self::InvalidSize => "文件大小不能为负。".to_owned(),
            Self::InvalidMetadata => "元数据必须是合法的 JSON 对象。".to_owned(),
            Self::InvalidHash => "文件哈希格式无效。".to_owned(),
            Self::InvalidLimit => "查询数量限制必须在 1 到 1000 之间。".to_owned(),
        }
    }
}

fn field_label(field: &str) -> &str {
    match field {
        "displayName" => "资产名称",
        "relativePath" => "受管路径",
        "mimeType" => "MIME 类型",
        "metadataJson" => "元数据",
        "search" => "搜索内容",
        "storageNamespace" => "存储命名空间",
        "assetKind" => "资产类型",
        "integrityStatus" => "完整性状态",
        _ => "字段",
    }
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, AssetValidationError> {
    normalize_optional(Some(value), field, max_length)?
        .ok_or(AssetValidationError::Required { field })
}

fn normalize_optional(
    value: Option<String>,
    field: &'static str,
    max_length: usize,
) -> Result<Option<String>, AssetValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_length {
        return Err(AssetValidationError::TooLong { field, max_length });
    }
    if value.chars().any(char::is_control) {
        return Err(AssetValidationError::ControlCharacters { field });
    }
    Ok(Some(value.to_owned()))
}

/// 校验受管相对路径。规则与 V3 migration 的 CHECK 约束保持一致，
/// 额外拒绝空段、连续斜杠和以 staging/quarantine 等保留目录开头的路径。
fn normalize_relative_path(value: String) -> Result<String, AssetValidationError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > RELATIVE_PATH_MAX_LENGTH {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if value.starts_with('/') {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if value.contains('\\') {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if value.chars().next().map(|c| c.is_ascii_alphabetic()) == Some(true)
        && value.chars().nth(1) == Some(':')
    {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if value == "." || value == ".." {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if value.starts_with("../") || value.contains("/../") || value.ends_with("/..") {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }

    let mut segments = value.split('/');
    let first = segments.next();
    if first != Some("assets") {
        // 导入服务只会写入 assets 命名空间下的文件；其他根目录由后续模块管理。
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }
    if segments.clone().any(|segment| segment.is_empty()) {
        return Err(AssetValidationError::InvalidPath {
            field: "relativePath",
        });
    }

    Ok(value.to_owned())
}

fn validate_sha256(value: &str) -> Result<(), AssetValidationError> {
    if value.len() != 64
        || !value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        return Err(AssetValidationError::InvalidHash);
    }
    Ok(())
}

fn json_is_valid(value: &str) -> bool {
    // SQLite 的 json_valid 在 strict 模式下接受任意 JSON 值；这里只接受对象字面量，避免元数据变成数组或标量。
    let trimmed = value.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return false;
    }
    // 走 serde_json 验证一次结构完整性。serde_json 已经在依赖树里（通过 tauri/tauri-build 间接引入）。
    serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{
        AssetDraft, AssetFilter, AssetKind, AssetValidationError, IntegrityStatus, StorageNamespace,
    };

    #[test]
    fn infers_asset_kind_from_mime_and_extension() {
        assert_eq!(
            AssetKind::infer(Some("image/png"), "photo.png"),
            AssetKind::Image
        );
        assert_eq!(AssetKind::infer(None, "intro.mp4"), AssetKind::Video);
        assert_eq!(
            AssetKind::infer(Some("application/zip"), "archive.zip"),
            AssetKind::Archive
        );
        assert_eq!(
            AssetKind::infer(Some("application/pdf"), "report.pdf"),
            AssetKind::Document
        );
        assert_eq!(AssetKind::infer(None, "unknown.xyz"), AssetKind::Other);
    }

    #[test]
    fn rejects_non_assets_relative_paths() {
        let error = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "preview".to_owned(),
            "staging/preview.png".to_owned(),
            10,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AssetValidationError::InvalidPath {
                field: "relativePath"
            }
        ));
    }

    #[test]
    fn accepts_assets_relative_paths() {
        let draft = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "preview".to_owned(),
            "assets/preview.png".to_owned(),
            10,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(draft.storage_namespace, StorageNamespace::Workspace);
        assert_eq!(draft.asset_kind, AssetKind::Image);
        assert_eq!(draft.relative_path, "assets/preview.png");
    }

    #[test]
    fn rejects_uppercase_or_short_sha256() {
        let error = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "preview".to_owned(),
            "assets/preview.png".to_owned(),
            10,
            Some("ABCDEF".to_owned()),
            None,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AssetValidationError::InvalidHash));
    }

    #[test]
    fn rejects_invalid_filter_limit() {
        let error = AssetFilter::try_new(None, None, None, None, Some(0)).unwrap_err();
        assert!(matches!(error, AssetValidationError::InvalidLimit));

        let large = AssetFilter::try_new(None, None, None, None, Some(10_000)).unwrap_err();
        assert!(matches!(large, AssetValidationError::InvalidLimit));
    }

    #[test]
    fn parses_known_enums() {
        assert_eq!(
            StorageNamespace::parse("teaching-resource").unwrap(),
            StorageNamespace::TeachingResource
        );
        assert_eq!(
            IntegrityStatus::parse("quarantined").unwrap(),
            IntegrityStatus::Quarantined
        );
        assert!(StorageNamespace::parse("unknown").is_err());
    }
}
