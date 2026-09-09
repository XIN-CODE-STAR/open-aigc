use std::sync::Arc;

use crate::application::error::AppError;
use crate::domain::analysis::{AnalysisJob, AnalysisJobDraft, AnalysisJobType};
use crate::ports::analysis_repository::AnalysisJobRepository;

/// Service for managing analysis jobs.
///
/// This service orchestrates the creation and management of analysis jobs,
/// which are used to track the progress of multi-step analysis pipelines.
pub struct AnalysisJobService {
    repo: Arc<dyn AnalysisJobRepository>,
}

impl AnalysisJobService {
    pub fn new(repo: Arc<dyn AnalysisJobRepository>) -> Self {
        Self { repo }
    }

    /// Create a new analysis job.
    pub fn create_job(&self, draft: AnalysisJobDraft) -> Result<AnalysisJob, AppError> {
        self.repo.create_job(&draft)
    }

    /// Get a job by ID.
    pub fn get_job(&self, job_id: &str) -> Result<Option<AnalysisJob>, AppError> {
        self.repo.get_job(job_id)
    }

    /// Start a job (mark as running).
    pub fn start_job(&self, job_id: &str, adapter_id: String) -> Result<AnalysisJob, AppError> {
        let mut job = self
            .repo
            .get_job(job_id)?
            .ok_or_else(|| AppError::Workflow(format!("job {job_id} not found")))?;

        job.start(adapter_id);
        self.repo.update_job(&job)?;
        Ok(job)
    }

    /// Complete a job.
    pub fn complete_job(&self, job_id: &str, result_json: String) -> Result<AnalysisJob, AppError> {
        let mut job = self
            .repo
            .get_job(job_id)?
            .ok_or_else(|| AppError::Workflow(format!("job {job_id} not found")))?;

        job.complete(result_json);
        self.repo.update_job(&job)?;

        // Wake up any jobs waiting on this one
        let waiting_jobs = self.repo.get_jobs_waiting_for(job_id)?;
        for _waiting_job in waiting_jobs {
            // In a real implementation, we would queue these jobs for processing
            // For now, we just log that they're ready
            eprintln!("[AnalysisJob] job {job_id} completed, waking up dependent jobs");
        }

        Ok(job)
    }

    /// Fail a job.
    pub fn fail_job(&self, job_id: &str, error: String) -> Result<AnalysisJob, AppError> {
        let mut job = self
            .repo
            .get_job(job_id)?
            .ok_or_else(|| AppError::Workflow(format!("job {job_id} not found")))?;

        job.fail(error);
        self.repo.update_job(&job)?;
        Ok(job)
    }

    /// Update job progress.
    pub fn update_progress(&self, job_id: &str, progress: f32) -> Result<(), AppError> {
        let mut job = self
            .repo
            .get_job(job_id)?
            .ok_or_else(|| AppError::Workflow(format!("job {job_id} not found")))?;

        job.update_progress(progress);
        self.repo.update_job(&job)?;
        Ok(())
    }

    /// Get all jobs for an asset.
    pub fn get_jobs_for_asset(&self, asset_id: &str) -> Result<Vec<AnalysisJob>, AppError> {
        self.repo.get_jobs_for_asset(asset_id)
    }

    /// Get queued jobs for processing.
    pub fn get_queued_jobs(&self, limit: usize) -> Result<Vec<AnalysisJob>, AppError> {
        self.repo.get_queued_jobs(limit)
    }

    /// Get running jobs.
    pub fn get_running_jobs(&self) -> Result<Vec<AnalysisJob>, AppError> {
        self.repo.get_running_jobs()
    }

    /// Create a captioning job for an asset.
    pub fn create_caption_job(&self, asset_id: &str) -> Result<AnalysisJob, AppError> {
        let draft = AnalysisJobDraft {
            job_type: AnalysisJobType::Caption,
            input_asset_id: Some(asset_id.to_owned()),
            input_job_id: None,
            priority: None,
        };
        self.repo.create_job(&draft)
    }

    /// Create a vision analysis job for an asset.
    pub fn create_vision_analysis_job(&self, asset_id: &str) -> Result<AnalysisJob, AppError> {
        let draft = AnalysisJobDraft {
            job_type: AnalysisJobType::VisionAnalysis,
            input_asset_id: Some(asset_id.to_owned()),
            input_job_id: None,
            priority: Some(10), // Higher priority for vision analysis
        };
        self.repo.create_job(&draft)
    }

    /// Create an OCR job for an asset.
    pub fn create_ocr_job(&self, asset_id: &str) -> Result<AnalysisJob, AppError> {
        let draft = AnalysisJobDraft {
            job_type: AnalysisJobType::Ocr,
            input_asset_id: Some(asset_id.to_owned()),
            input_job_id: None,
            priority: None,
        };
        self.repo.create_job(&draft)
    }
}
