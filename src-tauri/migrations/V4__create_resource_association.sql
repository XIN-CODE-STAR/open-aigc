-- 资源语义层：把 manifest 中的资产与教学资源、项目附件、生成输出等业务上下文关联起来。
-- manifest 只记录文件本身，业务含义通过本表表达。
CREATE TABLE resource_association (
  id TEXT PRIMARY KEY CHECK (length(id) = 36),
  asset_id TEXT NOT NULL REFERENCES asset_manifest(id),
  -- 业务上下文类型：教学资源 / 项目附件 / 生成输出
  context_kind TEXT NOT NULL
    CHECK (context_kind IN ('teaching-resource', 'project-attachment', 'generation-output')),
  -- 上下文引用：对教学资源为 classroom/student 等业务实体 id；对生成输出为生成批次 id。
  -- 形如 "classroom:<uuid>"。空字符串表示无具体绑定（如全局教学资源）。
  context_ref TEXT NOT NULL DEFAULT '',
  -- 该资产在上下文中的角色：source（原始素材）/ result（生成产物）/ reference（参考）/ attachment（附件）
  role TEXT NOT NULL DEFAULT 'attachment'
    CHECK (role IN ('source', 'result', 'reference', 'attachment')),
  notes TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

-- 同一资产在同一上下文 + 角色下只保留一条有效关联。
CREATE UNIQUE INDEX ux_resource_association_active
  ON resource_association(asset_id, context_kind, context_ref, role)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_resource_association_asset
  ON resource_association(asset_id, updated_at DESC)
  WHERE deleted_at IS NULL;

CREATE INDEX ix_resource_association_context
  ON resource_association(context_kind, context_ref, updated_at DESC)
  WHERE deleted_at IS NULL;
