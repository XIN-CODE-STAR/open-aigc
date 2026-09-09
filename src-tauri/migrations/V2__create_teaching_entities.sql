CREATE TABLE student (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 80),
  external_source TEXT,
  external_ref TEXT,
  email TEXT,
  notes TEXT,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'inactive', 'archived')),
  metadata_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(metadata_json)),
  origin_device_id TEXT NOT NULL REFERENCES device(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT,
  CHECK (
    (external_source IS NULL AND external_ref IS NULL) OR
    (external_source IS NOT NULL AND external_ref IS NOT NULL)
  )
) STRICT;

CREATE UNIQUE INDEX ux_student_external_ref
  ON student(external_source, external_ref)
  WHERE external_ref IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX ix_student_name
  ON student(display_name, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE TABLE classroom (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  owner_teacher_id TEXT NOT NULL REFERENCES teacher(id),
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 80),
  code TEXT,
  description TEXT,
  teaching_goal TEXT,
  status TEXT NOT NULL DEFAULT 'planned'
    CHECK (status IN ('planned', 'active', 'completed', 'archived')),
  starts_at TEXT,
  ends_at TEXT,
  origin_device_id TEXT NOT NULL REFERENCES device(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT,
  CHECK (ends_at IS NULL OR starts_at IS NULL OR ends_at >= starts_at)
) STRICT;

CREATE UNIQUE INDEX ux_classroom_code
  ON classroom(code COLLATE NOCASE)
  WHERE code IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX ix_classroom_status_updated
  ON classroom(status, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE TABLE classroom_student (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  classroom_id TEXT NOT NULL REFERENCES classroom(id),
  student_id TEXT NOT NULL REFERENCES student(id),
  roster_no TEXT,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'withdrawn', 'completed')),
  joined_at TEXT NOT NULL,
  left_at TEXT,
  origin_device_id TEXT NOT NULL REFERENCES device(id),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT,
  CHECK (left_at IS NULL OR left_at >= joined_at)
) STRICT;

CREATE UNIQUE INDEX ux_classroom_student_active
  ON classroom_student(classroom_id, student_id)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_classroom_student_student
  ON classroom_student(student_id, status)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_classroom_student_classroom
  ON classroom_student(classroom_id, status, roster_no)
  WHERE deleted_at IS NULL;
