use serde::{Deserialize, Serialize};

/// Request to generate embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingRequest {
    /// Text to embed
    pub text: String,
    /// Model to use for embedding
    pub model: Option<String>,
    /// Asset ID (for caching/context)
    pub asset_id: Option<String>,
}

/// Result of generating embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingResult {
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Model used
    pub model: String,
    /// Tokens used
    pub tokens_used: Option<u32>,
}

/// Information about an embedding adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingAdapterInfo {
    /// Unique adapter identifier
    pub adapter_id: String,
    /// Human-readable name
    pub name: String,
    /// Whether the adapter is ready to use
    pub ready: bool,
    /// Supported models
    pub supported_models: Vec<String>,
    /// Embedding dimension
    pub dimension: usize,
}

/// Port for generating embeddings.
///
/// This trait abstracts the embedding generation logic, allowing the system
/// to use different embedding models (local or cloud-based).
pub trait EmbeddingPort: Send + Sync {
    /// Get adapter information.
    fn info(&self) -> EmbeddingAdapterInfo;

    /// Get adapter ID.
    fn adapter_id(&self) -> &str;

    /// Check if the adapter is ready.
    fn is_ready(&self) -> bool;

    /// Get the embedding dimension.
    fn dimension(&self) -> usize;

    /// Generate embeddings for a single text.
    fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResult, EmbeddingError>;

    /// Generate embeddings for multiple texts (batch).
    fn embed_batch(
        &self,
        requests: &[EmbeddingRequest],
    ) -> Result<Vec<EmbeddingResult>, EmbeddingError> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            results.push(self.embed(request)?);
        }
        Ok(results)
    }
}

/// Errors that can occur during embedding generation.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("adapter not ready: {0}")]
    NotReady(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("internal error: {0}")]
    Internal(String),
}
