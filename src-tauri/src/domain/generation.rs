#![allow(dead_code)]

use serde::Serialize;
use thiserror::Error;

const PROVIDER_NAME_MAX_LENGTH: usize = 80;
const MODEL_NAME_MAX_LENGTH: usize = 120;
const PROMPT_TEXT_MAX_LENGTH: usize = 4000;

/// 生成任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GenerationStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl GenerationStatus {
    pub fn parse(value: &str) -> Result<Self, GenerationValidationError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(GenerationValidationError::InvalidChoice { field: "status" }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// 新建生成任务的草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationTaskDraft {
    pub workspace_id: String,
    pub provider_name: String,
    pub model_name: String,
    pub prompt_text: String,
}

impl GenerationTaskDraft {
    pub fn try_new(
        workspace_id: String,
        provider_name: String,
        model_name: String,
        prompt_text: String,
    ) -> Result<Self, GenerationValidationError> {
        let workspace_id = validate_uuid(workspace_id, "workspaceId")?;
        let provider_name =
            normalize_required(provider_name, "providerName", PROVIDER_NAME_MAX_LENGTH)?;
        let model_name = normalize_required(model_name, "modelName", MODEL_NAME_MAX_LENGTH)?;
        let prompt_text = normalize_required(prompt_text, "promptText", PROMPT_TEXT_MAX_LENGTH)?;
        Ok(Self {
            workspace_id,
            provider_name,
            model_name,
            prompt_text,
        })
    }
}

/// 生成任务记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationTaskRecord {
    pub id: String,
    pub workspace_id: String,
    pub provider_name: String,
    pub model_name: String,
    pub prompt_text: String,
    pub status: GenerationStatus,
    pub error_message: Option<String>,
    /// 任务进度 0-100。
    pub progress: u8,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// 生成结果记录：关联生成任务与受管资产。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResultRecord {
    pub id: String,
    pub task_id: String,
    pub asset_id: String,
    pub created_at: String,
}

/// 查询生成任务的筛选条件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationFilter {
    pub status: Option<GenerationStatus>,
    pub provider_name: Option<String>,
    pub limit: i64,
}

impl GenerationFilter {
    pub fn try_new(
        status: Option<String>,
        provider_name: Option<String>,
        limit: Option<i64>,
    ) -> Result<Self, GenerationValidationError> {
        let status = status
            .map(|value| GenerationStatus::parse(value.trim()))
            .transpose()?;
        let provider_name = provider_name
            .map(|value| normalize_optional(value, "providerName", PROVIDER_NAME_MAX_LENGTH))
            .transpose()?;
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
        if limit <= 0 || limit > MAX_LIST_LIMIT {
            return Err(GenerationValidationError::InvalidLimit);
        }
        Ok(Self {
            status,
            provider_name,
            limit,
        })
    }
}

const DEFAULT_LIST_LIMIT: i64 = 100;
const MAX_LIST_LIMIT: i64 = 500;

// ──────────────────────────────────────────────────────────────────
// M4: Generation Attempt & Remote Job
// ──────────────────────────────────────────────────────────────────

/// 生成尝试状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttemptStatus {
    Pending,
    Submitted,
    Polling,
    Downloading,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl AttemptStatus {
    pub fn parse(v: &str) -> Result<Self, GenerationValidationError> {
        match v {
            "pending" => Ok(Self::Pending),
            "submitted" => Ok(Self::Submitted),
            "polling" => Ok(Self::Polling),
            "downloading" => Ok(Self::Downloading),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "timed-out" => Ok(Self::TimedOut),
            _ => Err(GenerationValidationError::InvalidChoice {
                field: "attemptStatus",
            }),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Submitted => "submitted",
            Self::Polling => "polling",
            Self::Downloading => "downloading",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

/// 生成尝试草稿。
#[derive(Debug, Clone)]
pub struct GenerationAttemptDraft {
    pub task_id: String,
    pub credential_id: String,
    pub capability: String,
    pub request_snapshot_json: String,
    /// 目标 Provider 标识（PollWorker 用此从 Registry 解析实例）。
    pub provider_id: String,
}

/// 生成尝试记录：一次具体 Provider 调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationAttemptRecord {
    pub id: String,
    pub task_id: String,
    pub credential_id: String,
    pub capability: String,
    pub request_snapshot_json: String,
    pub remote_job_id: Option<String>,
    pub status: AttemptStatus,
    pub progress: u8,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub result_asset_id: Option<String>,
    /// Provider 标识（用于 PollWorker 从 Registry 解析 Provider 实例）。
    pub provider_id: String,
    /// 连续轮询失败次数（超过阈值标记 timed_out）。
    pub consecutive_failures: u32,
    /// 上次轮询时间（RFC3339）。
    pub last_poll_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 远端任务状态（从 Provider 轮询）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteJobStatus {
    pub remote_job_id: String,
    pub status: String,
    pub progress: u8,
    pub result_url: Option<String>,
    pub error_message: Option<String>,
}

/// 错误分类（用于 UI 展示和重试策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GenerationErrorKind {
    AuthExpired,
    QuotaExhausted,
    RateLimited,
    InvalidParameter,
    ContentPolicy,
    NetworkError,
    DownloadFailed,
    ProviderError,
    Unknown,
}

impl GenerationErrorKind {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::AuthExpired => "账号已过期，请重新登录",
            Self::QuotaExhausted => "额度不足，请切换账号或充值",
            Self::RateLimited => "平台限流，请稍后重试",
            Self::InvalidParameter => "参数不支持，请调整参数",
            Self::ContentPolicy => "内容不符合平台策略，请修改 Prompt",
            Self::NetworkError => "网络错误，请检查网络后重试",
            Self::DownloadFailed => "结果下载失败，可稍后重试",
            Self::ProviderError => "平台错误，请稍后重试",
            Self::Unknown => "未知错误",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::NetworkError | Self::DownloadFailed | Self::Unknown
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GenerationValidationError {
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
    #[error("limit must be between 1 and 500")]
    InvalidLimit,
}

impl GenerationValidationError {
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
            Self::InvalidLimit => "查询数量限制必须在 1 到 500 之间。".to_owned(),
        }
    }
}

fn label(field: &str) -> &str {
    match field {
        "workspaceId" => "工作空间标识",
        "providerName" => "供应商名称",
        "modelName" => "模型名称",
        "promptText" => "提示词",
        "status" => "任务状态",
        _ => "字段",
    }
}

fn validate_uuid(value: String, field: &'static str) -> Result<String, GenerationValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GenerationValidationError::Required { field });
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(GenerationValidationError::InvalidUuid { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_required(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, GenerationValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GenerationValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(GenerationValidationError::TooLong { field, max_length });
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(GenerationValidationError::InvalidChoice { field });
    }
    Ok(trimmed.to_owned())
}

fn normalize_optional(
    value: String,
    field: &'static str,
    max_length: usize,
) -> Result<String, GenerationValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GenerationValidationError::Required { field });
    }
    if trimmed.chars().count() > max_length {
        return Err(GenerationValidationError::TooLong { field, max_length });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn parses_known_statuses() {
        assert_eq!(
            GenerationStatus::parse("pending").unwrap(),
            GenerationStatus::Pending
        );
        assert_eq!(
            GenerationStatus::parse("succeeded").unwrap(),
            GenerationStatus::Succeeded
        );
        assert!(GenerationStatus::parse("unknown").is_err());
    }

    #[test]
    fn accepts_valid_draft() {
        let draft = GenerationTaskDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "Seedance".to_owned(),
            "seedance-v2".to_owned(),
            "生成一段舞蹈视频".to_owned(),
        )
        .unwrap();
        assert_eq!(draft.provider_name, "Seedance");
        assert_eq!(draft.workspace_id, SAMPLE_UUID);
    }

    #[test]
    fn rejects_empty_workspace_id() {
        let error = GenerationTaskDraft::try_new(
            "  ".to_owned(),
            "Seedance".to_owned(),
            "seedance-v2".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            GenerationValidationError::Required {
                field: "workspaceId"
            }
        ));
    }

    #[test]
    fn rejects_empty_provider_name() {
        let error = GenerationTaskDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "  ".to_owned(),
            "model".to_owned(),
            "prompt".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            GenerationValidationError::Required {
                field: "providerName"
            }
        ));
    }

    #[test]
    fn rejects_empty_prompt_text() {
        let error = GenerationTaskDraft::try_new(
            SAMPLE_UUID.to_owned(),
            "Seedance".to_owned(),
            "model".to_owned(),
            "  ".to_owned(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            GenerationValidationError::Required {
                field: "promptText"
            }
        ));
    }

    #[test]
    fn rejects_invalid_filter_limit() {
        assert!(matches!(
            GenerationFilter::try_new(None, None, Some(0)),
            Err(GenerationValidationError::InvalidLimit)
        ));
        assert!(matches!(
            GenerationFilter::try_new(None, None, Some(501)),
            Err(GenerationValidationError::InvalidLimit)
        ));
    }

    #[test]
    fn rejects_unknown_status_in_filter() {
        let error = GenerationFilter::try_new(Some("unknown".to_owned()), None, None).unwrap_err();
        assert!(matches!(
            error,
            GenerationValidationError::InvalidChoice { field: "status" }
        ));
    }
}
