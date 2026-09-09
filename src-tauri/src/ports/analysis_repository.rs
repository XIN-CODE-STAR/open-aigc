use crate::application::error::AppError;
use crate::domain::analysis::{AnalysisJob, AnalysisJobDraft};

/// Port for analysis job persistence.
///
/// This trait abstracts the storage layer for analysis jobs,
/// following the Hexagonal Architecture pattern.
pub trait AnalysisJobRepository: Send + Sync {
    /// Create a new analysis job from a draft.
    fn create_job(&self, draft: &AnalysisJobDraft) -> Result<AnalysisJob, AppError>;

    /// Get a job by ID.
    fn get_job(&self, job_id: &str) -> Result<Option<AnalysisJob>, AppError>;

    /// Update a job (e.g., status change, progress update).
    fn update_job(&self, job: &AnalysisJob) -> Result<(), AppError>;

    /// Delete a job.
    fn delete_job(&self, job_id: &str) -> Result<(), AppError>;

    /// Get all jobs for an asset.
    fn get_jobs_for_asset(&self, asset_id: &str) -> Result<Vec<AnalysisJob>, AppError>;

    /// Get all jobs that are waiting on another job.
    fn get_jobs_waiting_for(&self, job_id: &str) -> Result<Vec<AnalysisJob>, AppError>;

    /// Get queued jobs (for job processing).
    fn get_queued_jobs(&self, limit: usize) -> Result<Vec<AnalysisJob>, AppError>;

    /// Get running jobs.
    fn get_running_jobs(&self) -> Result<Vec<AnalysisJob>, AppError>;

    /// Get all terminal jobs (completed or failed) for an asset.
    fn get_terminal_jobs_for_asset(&self, asset_id: &str) -> Result<Vec<AnalysisJob>, AppError>;
}
