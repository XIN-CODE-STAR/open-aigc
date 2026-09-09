-- V29: ArtifactSemanticProfile table — Resource Intelligence layer
-- Constitution §5: Artifact is the resource fact source; tags, embedding, caption are rebuildable semantic indexes.

CREATE TABLE artifact_semantic_profiles (
    artifact_id      TEXT PRIMARY KEY REFERENCES artifact_records(id),
    caption          TEXT,                     -- image/video description
    ocr_text         TEXT,                     -- OCR extracted text
    tags_json        TEXT NOT NULL DEFAULT '[]',  -- JSON array of SemanticTag
    entities_json    TEXT NOT NULL DEFAULT '[]',  -- JSON array of SemanticEntity
    embedding_id     TEXT,                     -- pointer to embedding store
    analyzer         TEXT NOT NULL,            -- analyzer identifier (e.g. "gpt-4o", "local-clip")
    analyzer_version TEXT NOT NULL,            -- analyzer version
    analyzed_at      TEXT NOT NULL             -- analysis timestamp
);

-- Query by embedding ID (vector search entry point)
CREATE INDEX idx_semantic_profiles_embedding ON artifact_semantic_profiles(embedding_id)
    WHERE embedding_id IS NOT NULL;

-- Query by analyzer (for re-analysis / version tracking)
CREATE INDEX idx_semantic_profiles_analyzer ON artifact_semantic_profiles(analyzer);
