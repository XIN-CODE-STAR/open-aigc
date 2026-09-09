use serde::{Deserialize, Serialize};

/// Analysis job types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AnalysisJobType {
    Hash,
    Metadata,
    Ocr,
    Asr,
    FrameExtract,
    Caption,
    VisionAnalysis,
}

impl AnalysisJobType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Hash" => Some(Self::Hash),
            "Metadata" => Some(Self::Metadata),
            "Ocr" => Some(Self::Ocr),
            "Asr" => Some(Self::Asr),
            "FrameExtract" => Some(Self::FrameExtract),
            "Caption" => Some(Self::Caption),
            "VisionAnalysis" => Some(Self::VisionAnalysis),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hash => "Hash",
            Self::Metadata => "Metadata",
            Self::Ocr => "Ocr",
            Self::Asr => "Asr",
            Self::FrameExtract => "FrameExtract",
            Self::Caption => "Caption",
            Self::VisionAnalysis => "VisionAnalysis",
        }
    }
}

/// Analysis job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AnalysisJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

impl AnalysisJobStatus {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Queued" => Some(Self::Queued),
            "Running" => Some(Self::Running),
            "Completed" => Some(Self::Completed),
            "Failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }
}

/// Analysis job domain model
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisJob {
    /// Unique job identifier
    pub id: String,
    /// Type of analysis
    pub job_type: AnalysisJobType,
    /// Current status
    pub status: AnalysisJobStatus,
    /// Which adapter is processing this job
    pub adapter_id: Option<String>,
    /// Priority (0 = normal, higher = more priority)
    pub priority: i32,
    /// When the job was created
    pub created_at: String,
    /// When the job started processing
    pub started_at: Option<String>,
    /// When the job completed
    pub completed_at: Option<String>,
    /// Progress (0.0 to 1.0)
    pub progress: Option<f32>,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Serialized result JSON
    pub result_json: Option<String>,
    /// Input asset ID (for jobs processing an asset)
    pub input_asset_id: Option<String>,
    /// Input job ID (for jobs processing output of another job)
    pub input_job_id: Option<String>,
}

impl AnalysisJob {
    /// Create a new queued job
    pub fn new(
        id: String,
        job_type: AnalysisJobType,
        input_asset_id: Option<String>,
        input_job_id: Option<String>,
    ) -> Self {
        Self {
            id,
            job_type,
            status: AnalysisJobStatus::Queued,
            adapter_id: None,
            priority: 0,
            created_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
            started_at: None,
            completed_at: None,
            progress: None,
            error_message: None,
            result_json: None,
            input_asset_id,
            input_job_id,
        }
    }

    /// Mark job as running
    pub fn start(&mut self, adapter_id: String) {
        self.status = AnalysisJobStatus::Running;
        self.adapter_id = Some(adapter_id);
        self.started_at = Some(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
        );
    }

    /// Mark job as completed
    pub fn complete(&mut self, result_json: String) {
        self.status = AnalysisJobStatus::Completed;
        self.progress = Some(1.0);
        self.result_json = Some(result_json);
        self.completed_at = Some(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
        );
    }

    /// Mark job as failed
    pub fn fail(&mut self, error: String) {
        self.status = AnalysisJobStatus::Failed;
        self.error_message = Some(error);
        self.completed_at = Some(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
        );
    }

    /// Update progress
    pub fn update_progress(&mut self, progress: f32) {
        self.progress = Some(progress.clamp(0.0, 1.0));
    }

    /// Check if job is terminal (completed or failed)
    pub fn is_terminal(&self) -> bool {
        self.status == AnalysisJobStatus::Completed || self.status == AnalysisJobStatus::Failed
    }

    /// Check if job is active (queued or running)
    pub fn is_active(&self) -> bool {
        self.status == AnalysisJobStatus::Queued || self.status == AnalysisJobStatus::Running
    }
}

/// Draft for creating a new analysis job
#[derive(Debug, Clone)]
pub struct AnalysisJobDraft {
    pub job_type: AnalysisJobType,
    pub input_asset_id: Option<String>,
    pub input_job_id: Option<String>,
    pub priority: Option<i32>,
}

impl AnalysisJobDraft {
    /// Validate the draft
    pub fn validate(&self) -> Result<(), String> {
        if self.input_asset_id.is_none() && self.input_job_id.is_none() {
            return Err("Either input_asset_id or input_job_id must be provided".to_owned());
        }
        Ok(())
    }
}
