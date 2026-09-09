CREATE TABLE asset_manifest (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  storage_namespace TEXT NOT NULL DEFAULT 'workspace'
    CHECK (storage_namespace IN ('workspace', 'generation', 'teaching-resource', 'system')),
  asset_kind TEXT NOT NULL
    CHECK (asset_kind IN ('image', 'video', 'audio', 'document', 'archive', 'other')),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
  relative_path TEXT NOT NULL CHECK (
    length(trim(relative_path)) BETWEEN 1 AND 500
    AND substr(relative_path, 1, 1) <> '/'
    AND instr(relative_path, char(92)) = 0
    AND relative_path NOT GLOB '[A-Za-z]:*'
    AND relative_path <> '.'
    AND relative_path <> '..'
    AND relative_path NOT LIKE '../%'
    AND relative_path NOT LIKE '%/../%'
  ),
  size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
  sha256 TEXT CHECK (
    sha256 IS NULL OR (
      length(sha256) = 64
      AND sha256 = lower(sha256)
      AND sha256 NOT GLOB '*[^0-9a-f]*'
    )
  ),
  mime_type TEXT CHECK (mime_type IS NULL OR length(trim(mime_type)) BETWEEN 1 AND 120),
  integrity_status TEXT NOT NULL DEFAULT 'unverified'
    CHECK (integrity_status IN ('unverified', 'valid', 'missing', 'corrupt', 'quarantined')),
  metadata_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(metadata_json)),
  origin_device_id TEXT REFERENCES device(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE UNIQUE INDEX ux_asset_manifest_active_path
  ON asset_manifest(relative_path)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_asset_manifest_namespace_kind
  ON asset_manifest(storage_namespace, asset_kind, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_asset_manifest_integrity
  ON asset_manifest(integrity_status, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_asset_manifest_hash
  ON asset_manifest(sha256, size_bytes)
  WHERE sha256 IS NOT NULL AND deleted_at IS NULL;
