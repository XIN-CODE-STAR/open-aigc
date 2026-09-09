-- V30: Canvas node extended references table
-- Keeps core refs as direct columns, extended refs in this relation table.
-- New ref types can be added without ALTER TABLE migrations.

CREATE TABLE canvas_node_refs (
    node_id   TEXT NOT NULL REFERENCES canvas_nodes(id),
    ref_type  TEXT NOT NULL,   -- 'character', 'prompt', 'document', 'memory', 'agent', 'conversation', ...
    ref_id    TEXT NOT NULL,
    PRIMARY KEY (node_id, ref_type, ref_id)
);

-- Query by ref_type + ref_id (e.g. "find all nodes referencing character X")
CREATE INDEX idx_node_refs_lookup ON canvas_node_refs(ref_type, ref_id);
