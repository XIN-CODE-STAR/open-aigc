use serde::{Deserialize, Serialize};

/// Request for vision reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionReasoningRequest {
    /// Asset ID to reason about
    pub asset_id: String,
    /// Data URL of the resource
    pub data_url: String,
    /// Question or instruction for reasoning
    pub question: String,
    /// Optional context (e.g., from RAG)
    pub context: Option<String>,
    /// Model to use for reasoning
    pub model: Option<String>,
}

/// Result of vision reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionReasoningResult {
    /// Answer to the question
    pub answer: String,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// Reasoning steps (if available)
    pub reasoning_steps: Vec<String>,
    /// Raw response from the model
    pub raw_response: String,
    /// Tokens used
    pub tokens_used: Option<u32>,
}

/// Information about a vision reasoning adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionReasonerInfo {
    /// Unique adapter identifier
    pub adapter_id: String,
    /// Human-readable name
    pub name: String,
    /// Whether the adapter is ready to use
    pub ready: bool,
    /// Supported models
    pub supported_models: Vec<String>,
}

/// Port for vision reasoning.
///
/// This trait abstracts the vision reasoning logic, allowing the agent
/// to ask questions about images and videos and get detailed answers.
pub trait VisionReasoningPort: Send + Sync {
    /// Get adapter information.
    fn info(&self) -> VisionReasonerInfo;

    /// Get adapter ID.
    fn adapter_id(&self) -> &str;

    /// Check if the adapter is ready.
    fn is_ready(&self) -> bool;

    /// Perform vision reasoning.
    fn reason(
        &self,
        request: &VisionReasoningRequest,
    ) -> Result<VisionReasoningResult, VisionReasoningError>;
}

/// Errors that can occur during vision reasoning.
#[derive(Debug, thiserror::Error)]
pub enum VisionReasoningError {
    #[error("adapter not ready: {0}")]
    NotReady(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("reasoning failed: {0}")]
    ReasoningFailed(String),

    #[error("rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("internal error: {0}")]
    Internal(String),
}
