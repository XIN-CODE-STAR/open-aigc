use serde::{Deserialize, Serialize};

/// Request to parse a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseRequest {
    /// Asset ID to parse
    pub asset_id: String,
    /// Data URL of the resource (e.g., "data:image/png;base64,...")
    pub data_url: String,
    /// Optional instruction for parsing
    pub instruction: Option<String>,
}

/// Result of parsing a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseResult {
    /// Parsed description
    pub description: String,
    /// Extracted objects
    pub objects: Vec<String>,
    /// Scene information
    pub scene: Vec<String>,
    /// Actions or activities
    pub actions: Vec<String>,
    /// Abstract concepts
    pub concepts: Vec<String>,
    /// Extracted text (OCR)
    pub ocr_text: Option<String>,
    /// Raw response from the parser
    pub raw_response: String,
    /// Tokens used (if applicable)
    pub tokens_used: Option<u32>,
}

/// Information about a parser adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserInfo {
    /// Unique adapter identifier
    pub adapter_id: String,
    /// Human-readable name
    pub name: String,
    /// Whether the adapter is ready to use
    pub ready: bool,
    /// Supported models
    pub supported_models: Vec<String>,
}

/// Port for parsing resources.
///
/// This trait abstracts the parsing logic for different resource types
/// (images, videos, etc.), following the Hexagonal Architecture pattern.
pub trait ParserPort: Send + Sync {
    /// Get adapter information.
    fn info(&self) -> ParserInfo;

    /// Get adapter ID.
    fn adapter_id(&self) -> &str;

    /// Check if the adapter is ready.
    fn is_ready(&self) -> bool;

    /// Parse a resource.
    fn parse(&self, request: &ParseRequest) -> Result<ParseResult, ParseError>;
}

/// Errors that can occur during parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("adapter not ready: {0}")]
    NotReady(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("parsing failed: {0}")]
    ParsingFailed(String),

    #[error("rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("internal error: {0}")]
    Internal(String),
}
