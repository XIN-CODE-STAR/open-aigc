-- V34: Remove foreign key constraint from artifact_semantic_profiles
-- 语义分析可能针对不在 artifact_records 中的图片（如 Agent 对话中的图片附件），
-- 因此外键约束会导致保存失败。

-- SQLite 不支持 ALTER TABLE DROP CONSTRAINT，需要重建表
CREATE TABLE artifact_semantic_profiles_new (
    artifact_id      TEXT PRIMARY KEY,
    caption          TEXT,
    ocr_text         TEXT,
    tags_json        TEXT NOT NULL DEFAULT '[]',
    entities_json    TEXT NOT NULL DEFAULT '[]',
    embedding_id     TEXT,
    analyzer         TEXT NOT NULL,
    analyzer_version TEXT NOT NULL,
    analyzed_at      TEXT NOT NULL,
    description_short    TEXT,
    description_detailed TEXT,
    objects_json     TEXT NOT NULL DEFAULT '[]',
    scene_json       TEXT NOT NULL DEFAULT '[]',
    actions_json     TEXT NOT NULL DEFAULT '[]',
    concepts_json    TEXT NOT NULL DEFAULT '[]',
    relations_json   TEXT NOT NULL DEFAULT '[]',
    analysis_job_id  TEXT
);

INSERT INTO artifact_semantic_profiles_new SELECT * FROM artifact_semantic_profiles;
DROP TABLE artifact_semantic_profiles;
ALTER TABLE artifact_semantic_profiles_new RENAME TO artifact_semantic_profiles;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_semantic_profiles_embedding ON artifact_semantic_profiles(embedding_id)
    WHERE embedding_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_semantic_profiles_analyzer ON artifact_semantic_profiles(analyzer);
