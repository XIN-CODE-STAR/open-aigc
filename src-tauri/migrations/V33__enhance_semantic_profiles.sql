-- V33: Enhance ArtifactSemanticProfile for Resource Intelligence Layer
-- Adds description_short, description_detailed, objects, scene, actions, concepts, relations, analysis_job_id

-- Add new columns to artifact_semantic_profiles
ALTER TABLE artifact_semantic_profiles ADD COLUMN description_short TEXT;
ALTER TABLE artifact_semantic_profiles ADD COLUMN description_detailed TEXT;
ALTER TABLE artifact_semantic_profiles ADD COLUMN objects_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE artifact_semantic_profiles ADD COLUMN scene_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE artifact_semantic_profiles ADD COLUMN actions_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE artifact_semantic_profiles ADD COLUMN concepts_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE artifact_semantic_profiles ADD COLUMN relations_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE artifact_semantic_profiles ADD COLUMN analysis_job_id TEXT;

-- Create AnalysisJob table
CREATE TABLE analysis_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL CHECK (job_type IN ('Hash', 'Metadata', 'Ocr', 'Asr', 'FrameExtract', 'Caption', 'VisionAnalysis')),
    status TEXT NOT NULL CHECK (status IN ('Queued', 'Running', 'Completed', 'Failed')) DEFAULT 'Queued',
    adapter_id TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    progress REAL CHECK (progress >= 0 AND progress <= 1),
    error_message TEXT,
    result_json TEXT,
    input_asset_id TEXT,
    input_job_id TEXT,
    CHECK (input_asset_id IS NOT NULL OR input_job_id IS NOT NULL)
);

-- Create indexes for AnalysisJob
CREATE INDEX idx_analysis_jobs_status ON analysis_jobs(status) WHERE status IN ('Queued', 'Running');
CREATE INDEX idx_analysis_jobs_asset ON analysis_jobs(input_asset_id) WHERE input_asset_id IS NOT NULL;
CREATE INDEX idx_analysis_jobs_job ON analysis_jobs(input_job_id) WHERE input_job_id IS NOT NULL;
