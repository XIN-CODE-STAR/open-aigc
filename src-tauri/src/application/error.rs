use thiserror::Error;

use crate::{
    domain::{
        agent::AgentValidationError, assets::AssetValidationError, backup::BackupValidationError,
        creative_memory::MemoryValidationError, credentials::CredentialValidationError,
        edit::EditValidationError, generation::GenerationValidationError, manga::MangaError,
        providers::ProviderError, resources::ResourceValidationError,
        review::ReviewValidationError, workspace::WorkspaceValidationError,
    },
    ports::{
        agent_repository::AgentRepositoryError, agent_tool_executor::AgentToolError,
        asset_repository::AssetRepositoryError, backup_repository::BackupRepositoryError,
        creative_memory_repository::CreativeMemoryRepositoryError,
        creative_state_repository::CreativeStateRepositoryError,
        credential_repository::CredentialRepositoryError, edit_repository::EditRepositoryError,
        generation_attempt_repository::AttemptRepositoryError,
        generation_repository::GenerationRepositoryError, manga_repository::MangaRepositoryError,
        persistence::PersistenceError, plan_repository::PlanRepositoryError,
        resource_repository::ResourceRepositoryError, review_repository::ReviewRepositoryError,
        vector_repository::VectorError,
    },
};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Validation(#[from] WorkspaceValidationError),
    #[error(transparent)]
    AssetValidation(#[from] AssetValidationError),
    #[error(transparent)]
    ResourceValidation(#[from] ResourceValidationError),
    #[error(transparent)]
    BackupValidation(#[from] BackupValidationError),
    #[error(transparent)]
    GenerationValidation(#[from] GenerationValidationError),
    #[error(transparent)]
    CredentialValidation(#[from] CredentialValidationError),
    #[error(transparent)]
    AgentValidation(#[from] AgentValidationError),
    #[error(transparent)]
    MangaValidation(#[from] MangaError),
    #[error(transparent)]
    MangaRepository(#[from] MangaRepositoryError),
    #[error(transparent)]
    AttemptRepository(#[from] AttemptRepositoryError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("workspace is already initialized")]
    AlreadyInitialized,
    #[error("workspace state lock is unavailable")]
    StateUnavailable,
    #[error("agent loop exceeded max iterations ({0})")]
    AgentLoopTooManyIterations(u8),
    #[error("agent chat LLM error: {0}")]
    AgentLlmError(String),
    #[error("agent tool execution failed: {0}")]
    AgentToolFailed(String),
    #[error("agent is waiting for user to answer a question")]
    AgentUserQuestionPending,
    #[error("generation submit failed: {0}")]
    AgentGenerationSubmitFailed(String),
    #[error("memory service is not available")]
    MemoryServiceUnavailable,
    #[error("memory service error: {0}")]
    MemoryServiceError(String),
    #[error("platform run {0} not found")]
    PlatformRunNotFound(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error(transparent)]
    PlanRepository(#[from] PlanRepositoryError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error(transparent)]
    AssetRepository(#[from] AssetRepositoryError),
    #[error(transparent)]
    ResourceRepository(#[from] ResourceRepositoryError),
    #[error(transparent)]
    BackupRepository(#[from] BackupRepositoryError),
    #[error(transparent)]
    GenerationRepository(#[from] GenerationRepositoryError),
    #[error(transparent)]
    CredentialRepository(#[from] CredentialRepositoryError),
    #[error(transparent)]
    AgentRepository(#[from] AgentRepositoryError),
    #[error(transparent)]
    ReviewRepository(#[from] ReviewRepositoryError),
    #[error(transparent)]
    EditRepository(#[from] EditRepositoryError),
    #[error(transparent)]
    ReviewValidation(#[from] ReviewValidationError),
    #[error(transparent)]
    EditValidation(#[from] EditValidationError),
    #[error(transparent)]
    MemoryValidation(#[from] MemoryValidationError),
    #[error(transparent)]
    CreativeMemoryRepository(#[from] CreativeMemoryRepositoryError),
    #[error(transparent)]
    CreativeStateRepository(#[from] CreativeStateRepositoryError),
    #[error("workflow error: {0}")]
    Workflow(String),
}

impl From<AgentToolError> for AppError {
    fn from(error: AgentToolError) -> Self {
        match error {
            AgentToolError::Persistence(p) => AppError::Persistence(p),
            other => AppError::AgentToolFailed(other.to_string()),
        }
    }
}

impl From<VectorError> for AppError {
    fn from(error: VectorError) -> Self {
        AppError::Workflow(format!("vector repository error: {}", error))
    }
}

impl AppError {
    /// Construct an AppError from an error message for generic I/O failures.
    pub fn new(context: impl Into<String>, error: impl std::fmt::Display) -> Self {
        AppError::Persistence(PersistenceError::new(
            Box::leak(context.into().into_boxed_str()),
            std::io::Error::other(error.to_string()),
        ))
    }

    /// Create a workflow error.
    pub fn workflow(error: impl std::fmt::Display) -> Self {
        AppError::Workflow(error.to_string())
    }
}
