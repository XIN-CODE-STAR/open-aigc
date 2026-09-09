-- V32: Add conversation_ref to canvas_canvases for direct ID association.
-- Previously relied on name convention ("conv-{id}"), which is fragile.
-- Direct ID association is robust against rename, copy, import, multi-canvas scenarios.

ALTER TABLE canvas_canvases ADD COLUMN conversation_ref TEXT;

-- Query canvases by conversation
CREATE INDEX idx_canvas_conversation_ref ON canvas_canvases(conversation_ref)
    WHERE conversation_ref IS NOT NULL;
