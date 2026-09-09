CREATE TABLE device (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 80),
  platform TEXT NOT NULL CHECK (length(trim(platform)) > 0),
  app_version TEXT NOT NULL CHECK (length(trim(app_version)) > 0),
  sync_capability TEXT NOT NULL DEFAULT 'local-only'
    CHECK (sync_capability IN ('local-only', 'sync-ready', 'sync-enabled')),
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  retired_at TEXT
) STRICT;

CREATE TABLE teacher (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 60),
  email TEXT,
  locale TEXT NOT NULL DEFAULT 'zh-CN',
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'disabled', 'archived')),
  preferences_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(preferences_json)),
  origin_device_id TEXT NOT NULL REFERENCES device(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE TABLE workspace (
  singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
  id TEXT NOT NULL UNIQUE CHECK (length(id) = 36),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 80),
  mode TEXT NOT NULL DEFAULT 'single-teacher'
    CHECK (mode IN ('single-teacher', 'organization')),
  owner_teacher_id TEXT NOT NULL REFERENCES teacher(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX ix_teacher_origin_device ON teacher(origin_device_id);
