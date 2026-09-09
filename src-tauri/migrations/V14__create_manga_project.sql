-- V14: AI 漫剧项目数据模型
-- MangaProject → StoryBible / CharacterProfile / Scene → Shot → ShotAssetBinding

CREATE TABLE manga_projects (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  classroom_id TEXT,
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  theme TEXT,
  teaching_goal TEXT,
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'in-progress', 'completed', 'archived')),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_manga_projects_workspace ON manga_projects(workspace_id, updated_at DESC);

CREATE TABLE story_bibles (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES manga_projects(id) ON DELETE CASCADE,
  logline TEXT,
  synopsis TEXT,
  style_guide TEXT,
  visual_style TEXT,
  tone TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE character_profiles (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES manga_projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
  role TEXT,
  appearance TEXT,
  personality TEXT,
  consistency_prompt TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_character_profiles_project ON character_profiles(project_id);

CREATE TABLE scenes (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  project_id TEXT NOT NULL REFERENCES manga_projects(id) ON DELETE CASCADE,
  idx INTEGER NOT NULL CHECK (idx >= 0),
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  summary TEXT,
  location TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_scenes_project ON scenes(project_id, idx);

CREATE TABLE shots (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
  idx INTEGER NOT NULL CHECK (idx >= 0),
  shot_type TEXT,
  camera_motion TEXT,
  duration TEXT,
  prompt TEXT,
  negative_prompt TEXT,
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'ready', 'generating', 'completed', 'failed')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_shots_scene ON shots(scene_id, idx);

CREATE TABLE shot_asset_bindings (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  shot_id TEXT NOT NULL REFERENCES shots(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES asset_manifest(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (length(trim(kind)) BETWEEN 1 AND 100),
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_shot_asset_bindings_shot ON shot_asset_bindings(shot_id);
CREATE INDEX ix_shot_asset_bindings_asset ON shot_asset_bindings(asset_id);
