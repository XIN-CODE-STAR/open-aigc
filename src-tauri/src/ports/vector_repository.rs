use serde::{Deserialize, Serialize};

/// A vector entry in the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEntry {
    /// Unique identifier for the vector
    pub id: String,
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Metadata associated with the vector
    pub metadata: VectorMetadata,
}

/// Metadata associated with a vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorMetadata {
    /// Asset ID this vector belongs to
    pub asset_id: String,
    /// Type of content (e.g., "caption", "ocr", "description")
    pub content_type: String,
    /// The original text that was embedded
    pub text: String,
    /// Additional metadata
    pub extra: Option<serde_json::Value>,
}

/// Request to search for similar vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchRequest {
    /// The query vector
    pub embedding: Vec<f32>,
    /// Number of results to return
    pub limit: usize,
    /// Minimum similarity score (0.0 to 1.0)
    pub min_score: Option<f32>,
    /// Filter by asset IDs
    pub asset_ids: Option<Vec<String>>,
    /// Filter by content type
    pub content_type: Option<String>,
}

/// Result of a vector search.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResult {
    /// The matching vector entry
    pub entry: VectorEntry,
    /// Similarity score (0.0 to 1.0)
    pub score: f32,
}

/// Port for vector storage and retrieval.
///
/// This trait abstracts the vector storage layer, allowing the system
/// to use different vector databases (LanceDB, Pinecone, etc.).
pub trait VectorRepository: Send + Sync {
    /// Store a vector entry.
    fn store(&self, entry: &VectorEntry) -> Result<(), VectorError>;

    /// Store multiple vector entries (batch).
    fn store_batch(&self, entries: &[VectorEntry]) -> Result<(), VectorError> {
        for entry in entries {
            self.store(entry)?;
        }
        Ok(())
    }

    /// Get a vector entry by ID.
    fn get(&self, id: &str) -> Result<Option<VectorEntry>, VectorError>;

    /// Delete a vector entry by ID.
    fn delete(&self, id: &str) -> Result<(), VectorError>;

    /// Delete all vectors for an asset.
    fn delete_by_asset(&self, asset_id: &str) -> Result<(), VectorError>;

    /// Search for similar vectors.
    fn search(&self, request: &VectorSearchRequest)
        -> Result<Vec<VectorSearchResult>, VectorError>;

    /// Get the total number of vectors.
    fn count(&self) -> Result<usize, VectorError>;
}

/// Errors that can occur during vector operations.
#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("storage failed: {0}")]
    StorageFailed(String),

    #[error("retrieval failed: {0}")]
    RetrievalFailed(String),

    #[error("search failed: {0}")]
    SearchFailed(String),

    #[error("deletion failed: {0}")]
    DeletionFailed(String),

    #[error("internal error: {0}")]
    Internal(String),
}
