-- V35: Vector entries — 向量存储（画布节点/语义内容的嵌入索引）。
-- embedding 以 f32 小端字节存储；余弦相似度在应用层暴力计算（画布量级足够）。

CREATE TABLE vector_entries (
    id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,
    dimension INTEGER NOT NULL,
    asset_id TEXT NOT NULL,
    content_type TEXT NOT NULL,
    text TEXT NOT NULL,
    extra_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_vector_entries_asset ON vector_entries(asset_id);
CREATE INDEX idx_vector_entries_content ON vector_entries(content_type);
