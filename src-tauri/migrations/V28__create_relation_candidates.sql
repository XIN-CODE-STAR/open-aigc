-- V28: RelationCandidate table — RAG → Agent → Canvas pipeline
-- Constitution §3: RAG only produces retrieval results and relation candidates, does not directly modify Canvas.
-- Constitution §6: Canvas Edge stores "confirmed relationships", RelationCandidate stores "AI speculated relationships".

CREATE TABLE canvas_relation_candidates (
    id              TEXT PRIMARY KEY,
    source_node_id  TEXT NOT NULL REFERENCES canvas_nodes(id),
    target_node_id  TEXT NOT NULL REFERENCES canvas_nodes(id),
    relation_type   TEXT NOT NULL,           -- "same_character" / "same_style" / "similar_visual" / ...
    confidence      REAL NOT NULL,           -- 0.0 ~ 1.0
    evidence_json   TEXT,                    -- JSON array of evidence strings
    source          TEXT NOT NULL,           -- 'rag' / 'agent' / 'user' / 'system'
    status          TEXT NOT NULL DEFAULT 'pending',  -- 'pending' / 'accepted' / 'rejected'
    created_at      TEXT NOT NULL,
    reviewed_at     TEXT
);

-- Query pending candidates (most common operation)
CREATE INDEX idx_rel_candidates_status ON canvas_relation_candidates(status)
    WHERE status = 'pending';

-- Query candidates for a specific source node
CREATE INDEX idx_rel_candidates_source ON canvas_relation_candidates(source_node_id);

-- Query candidates for a specific target node
CREATE INDEX idx_rel_candidates_target ON canvas_relation_candidates(target_node_id);

-- Query candidates by type
CREATE INDEX idx_rel_candidates_type ON canvas_relation_candidates(relation_type);
