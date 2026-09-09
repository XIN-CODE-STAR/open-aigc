-- V15: 持久化生成任务队列
-- generation_attempts 表记录每次 Provider 调用的完整生命周期

CREATE TABLE generation_attempts (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  task_id TEXT NOT NULL REFERENCES generation_tasks(id) ON DELETE CASCADE,
  credential_id TEXT NOT NULL REFERENCES provider_credentials(id),
  capability TEXT NOT NULL CHECK (length(trim(capability)) BETWEEN 1 AND 80),
  request_snapshot_json TEXT NOT NULL CHECK (json_valid(request_snapshot_json)),
  remote_job_id TEXT,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'submitted', 'polling', 'downloading',
                       'succeeded', 'failed', 'cancelled', 'timed-out')),
  progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
  error_code TEXT,
  error_message TEXT,
  result_asset_id TEXT REFERENCES asset_manifest(id),
  started_at TEXT,
  finished_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_generation_attempts_task ON generation_attempts(task_id, created_at DESC);
CREATE INDEX ix_generation_attempts_status ON generation_attempts(status) WHERE status NOT IN ('succeeded', 'cancelled');
CREATE INDEX ix_generation_attempts_remote ON generation_attempts(remote_job_id) WHERE remote_job_id IS NOT NULL;
