-- V16: AI 视频/图片生成 Agent 平台核心数据模型
-- 按照 2026-07-20-ai-video-image-agent-platform-architecture.md 第9节实现

-- ═══════════════════════════════════════════════════
-- 1. 项目表
-- ═══════════════════════════════════════════════════

CREATE TABLE platform_projects (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  owner_teacher_id TEXT,
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  description TEXT,
  project_type TEXT NOT NULL DEFAULT 'promo_video'
    CHECK (project_type IN ('promo_video', 'short_film', 'product_video', 'documentary', 'social_media', 'educational')),
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'planning', 'in_progress', 'review', 'completed', 'archived')),
  target_duration_seconds INTEGER,
  target_platform TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE INDEX ix_platform_projects_workspace ON platform_projects(workspace_id, updated_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX ix_platform_projects_owner ON platform_projects(owner_teacher_id) WHERE deleted_at IS NULL;

-- ═══════════════════════════════════════════════════
-- 2. 创意运行表
-- ═══════════════════════════════════════════════════

CREATE TABLE creative_runs (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  user_goal TEXT NOT NULL CHECK (length(trim(user_goal)) BETWEEN 1 AND 4000),
  workflow_type TEXT NOT NULL DEFAULT 'promo_video'
    CHECK (workflow_type IN ('promo_video', 'short_film', 'product_video', 'documentary', 'social_media', 'educational')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'waiting_approval', 'completed', 'failed', 'cancelled')),
  current_stage TEXT NOT NULL DEFAULT 'requirement_analysis'
    CHECK (current_stage IN ('requirement_analysis', 'visual_spec', 'story_planning', 'character_planning',
      'human_approval', 'image_generation', 'video_generation', 'review', 'optimization', 'editing', 'final_review', 'export')),
  budget_limit REAL,
  cost_estimate REAL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_creative_runs_project ON creative_runs(project_id, created_at DESC);

-- ═══════════════════════════════════════════════════
-- 3. Agent 步骤表
-- ═══════════════════════════════════════════════════

CREATE TABLE agent_steps (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  run_id TEXT NOT NULL REFERENCES creative_runs(id) ON DELETE CASCADE,
  agent_name TEXT NOT NULL CHECK (length(trim(agent_name)) BETWEEN 1 AND 80),
  stage TEXT NOT NULL CHECK (length(trim(stage)) BETWEEN 1 AND 80),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'completed', 'failed', 'skipped')),
  input_snapshot_json TEXT,
  output_snapshot_json TEXT,
  error_message TEXT,
  started_at TEXT,
  finished_at TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_agent_steps_run ON agent_steps(run_id, stage);

-- ═══════════════════════════════════════════════════
-- 4. 需求规格表
-- ═══════════════════════════════════════════════════

CREATE TABLE requirement_specs (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  run_id TEXT REFERENCES creative_runs(id) ON DELETE SET NULL,
  content_type TEXT NOT NULL CHECK (length(trim(content_type)) BETWEEN 1 AND 80),
  duration_seconds INTEGER,
  target_audience TEXT,
  platform TEXT,
  message TEXT,
  tone TEXT,
  must_have_json TEXT CHECK (json_valid(must_have_json)),
  must_not_have_json TEXT CHECK (json_valid(must_not_have_json)),
  raw_output TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_requirement_specs_project ON requirement_specs(project_id, version DESC);

-- ═══════════════════════════════════════════════════
-- 5. 视觉规范表
-- ═══════════════════════════════════════════════════

CREATE TABLE visual_specs (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  run_id TEXT REFERENCES creative_runs(id) ON DELETE SET NULL,
  style TEXT NOT NULL,
  color_palette_json TEXT CHECK (json_valid(color_palette_json)),
  composition TEXT,
  camera TEXT,
  lighting TEXT,
  mood TEXT,
  texture TEXT,
  negative_style TEXT,
  raw_output TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_visual_specs_project ON visual_specs(project_id, version DESC);

-- ═══════════════════════════════════════════════════
-- 6. 剧本表
-- ═══════════════════════════════════════════════════

CREATE TABLE scripts (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  run_id TEXT REFERENCES creative_runs(id) ON DELETE SET NULL,
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  logline TEXT,
  synopsis TEXT,
  script_text TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'reviewed', 'approved')),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_scripts_project ON scripts(project_id, version DESC);

-- ═══════════════════════════════════════════════════
-- 7. 场景表
-- ═══════════════════════════════════════════════════

CREATE TABLE film_scenes (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  script_id TEXT REFERENCES scripts(id) ON DELETE SET NULL,
  scene_index INTEGER NOT NULL CHECK (scene_index >= 0),
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  summary TEXT,
  location TEXT,
  emotion TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_film_scenes_project ON film_scenes(project_id, scene_index);

-- ═══════════════════════════════════════════════════
-- 8. 镜头表
-- ═══════════════════════════════════════════════════

CREATE TABLE film_shots (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  scene_id TEXT REFERENCES film_scenes(id) ON DELETE SET NULL,
  shot_index INTEGER NOT NULL CHECK (shot_index >= 0),
  duration_seconds REAL,
  scene_description TEXT,
  camera TEXT,
  movement TEXT,
  composition TEXT,
  emotion TEXT,
  purpose TEXT,
  voiceover TEXT,
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'keyframe_ready', 'generating', 'completed', 'failed', 'approved')),
  selected_asset_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_film_shots_project ON film_shots(project_id, scene_id, shot_index);

-- ═══════════════════════════════════════════════════
-- 9. 角色表
-- ═══════════════════════════════════════════════════

CREATE TABLE film_characters (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
  role TEXT,
  profile TEXT,
  appearance TEXT,
  clothing TEXT,
  style_lock TEXT,
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'assets_ready', 'approved')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_film_characters_project ON film_characters(project_id, name);

-- ═══════════════════════════════════════════════════
-- 10. 角色资产表
-- ═══════════════════════════════════════════════════

CREATE TABLE character_assets (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  character_id TEXT NOT NULL REFERENCES film_characters(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES asset_manifest(id) ON DELETE CASCADE,
  asset_type TEXT NOT NULL CHECK (asset_type IN ('front_view', 'side_view', 'back_view', 'expression', 'pose', 'clothing_variant')),
  view_type TEXT,
  embedding_id TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_character_assets_character ON character_assets(character_id);

-- ═══════════════════════════════════════════════════
-- 11. Prompt 版本表
-- ═══════════════════════════════════════════════════

CREATE TABLE prompt_versions (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  shot_id TEXT REFERENCES film_shots(id) ON DELETE SET NULL,
  prompt_type TEXT NOT NULL CHECK (prompt_type IN ('image', 'video', 'voiceover', 'music')),
  provider_target TEXT,
  positive_prompt TEXT NOT NULL,
  negative_prompt TEXT,
  parameters_json TEXT CHECK (json_valid(parameters_json)),
  source_agent_step_id TEXT REFERENCES agent_steps(id) ON DELETE SET NULL,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_prompt_versions_shot ON prompt_versions(shot_id, version DESC);

-- ═══════════════════════════════════════════════════
-- 12. 生成任务表
-- ═══════════════════════════════════════════════════

CREATE TABLE platform_generation_tasks (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  shot_id TEXT REFERENCES film_shots(id) ON DELETE SET NULL,
  task_type TEXT NOT NULL CHECK (task_type IN ('image', 'video', 'voiceover', 'music')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')),
  prompt_version_id TEXT REFERENCES prompt_versions(id) ON DELETE SET NULL,
  provider_capability TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0),
  created_by TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_platform_gen_tasks_project ON platform_generation_tasks(project_id, status, created_at);
CREATE INDEX ix_platform_gen_tasks_shot ON platform_generation_tasks(shot_id, status);

-- ═══════════════════════════════════════════════════
-- 13. 生成尝试表
-- ═══════════════════════════════════════════════════

CREATE TABLE platform_generation_attempts (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  task_id TEXT NOT NULL REFERENCES platform_generation_tasks(id) ON DELETE CASCADE,
  provider_id TEXT,
  provider_account_id TEXT REFERENCES provider_accounts(id) ON DELETE SET NULL,
  model TEXT,
  request_snapshot_json TEXT CHECK (json_valid(request_snapshot_json)),
  remote_job_id TEXT,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'submitted', 'polling', 'downloading', 'succeeded', 'failed', 'cancelled', 'timed-out')),
  progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
  error_code TEXT,
  error_message TEXT,
  result_asset_id TEXT,
  started_at TEXT,
  finished_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_platform_gen_attempts_task ON platform_generation_attempts(task_id, created_at DESC);
CREATE INDEX ix_platform_gen_attempts_status ON platform_generation_attempts(status) WHERE status NOT IN ('succeeded', 'cancelled');
CREATE INDEX ix_platform_gen_attempts_remote ON platform_generation_attempts(remote_job_id) WHERE remote_job_id IS NOT NULL;

-- ═══════════════════════════════════════════════════
-- 14. 资产表
-- ═══════════════════════════════════════════════════

CREATE TABLE platform_assets (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  asset_type TEXT NOT NULL CHECK (asset_type IN ('image', 'video', 'audio', 'document')),
  storage_key TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  hash TEXT,
  width INTEGER,
  height INTEGER,
  duration_seconds REAL,
  source_type TEXT CHECK (source_type IN ('generated', 'uploaded', 'imported')),
  source_task_id TEXT REFERENCES platform_generation_tasks(id) ON DELETE SET NULL,
  source_attempt_id TEXT REFERENCES platform_generation_attempts(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_platform_assets_project ON platform_assets(project_id, asset_type, created_at);

-- ═══════════════════════════════════════════════════
-- 15. 审片报告表
-- ═══════════════════════════════════════════════════

CREATE TABLE review_reports (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  shot_id TEXT REFERENCES film_shots(id) ON DELETE SET NULL,
  asset_id TEXT REFERENCES platform_assets(id) ON DELETE SET NULL,
  review_type TEXT NOT NULL CHECK (review_type IN ('auto', 'manual')),
  character_score REAL CHECK (character_score BETWEEN 0 AND 100),
  style_score REAL CHECK (style_score BETWEEN 0 AND 100),
  composition_score REAL CHECK (composition_score BETWEEN 0 AND 100),
  camera_score REAL CHECK (camera_score BETWEEN 0 AND 100),
  requirement_match_score REAL CHECK (requirement_match_score BETWEEN 0 AND 100),
  overall_score REAL CHECK (overall_score BETWEEN 0 AND 100),
  issues_json TEXT CHECK (json_valid(issues_json)),
  recommendation TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_review_reports_shot ON review_reports(shot_id, overall_score);

-- ═══════════════════════════════════════════════════
-- 16. 优化循环表
-- ═══════════════════════════════════════════════════

CREATE TABLE optimization_cycles (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES platform_projects(id) ON DELETE CASCADE,
  shot_id TEXT REFERENCES film_shots(id) ON DELETE SET NULL,
  source_review_id TEXT REFERENCES review_reports(id) ON DELETE SET NULL,
  source_prompt_version_id TEXT REFERENCES prompt_versions(id) ON DELETE SET NULL,
  new_prompt_version_id TEXT REFERENCES prompt_versions(id) ON DELETE SET NULL,
  reason TEXT,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'applied', 'rejected')),
  created_at TEXT NOT NULL
) STRICT;

-- ═══════════════════════════════════════════════════
-- 17. Provider 账号表
-- ═══════════════════════════════════════════════════

CREATE TABLE provider_accounts (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 80),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 120),
  auth_mode TEXT NOT NULL CHECK (auth_mode IN ('api_key', 'oauth', 'account_login', 'session_cookie', 'manual')),
  credential_ref TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'expired', 'suspended', 'error')),
  last_health_check_at TEXT,
  quota_snapshot_json TEXT CHECK (json_valid(quota_snapshot_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_provider_accounts_provider ON provider_accounts(provider_id, status);

-- ═══════════════════════════════════════════════════
-- 18. 审计日志表
-- ═══════════════════════════════════════════════════

CREATE TABLE audit_logs (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  actor_id TEXT,
  project_id TEXT REFERENCES platform_projects(id) ON DELETE SET NULL,
  action TEXT NOT NULL CHECK (length(trim(action)) BETWEEN 1 AND 80),
  target_type TEXT CHECK (length(trim(target_type)) BETWEEN 1 AND 80),
  target_id TEXT,
  metadata_json TEXT CHECK (json_valid(metadata_json)),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_audit_logs_project ON audit_logs(project_id, created_at);
CREATE INDEX ix_audit_logs_actor ON audit_logs(actor_id, created_at);
