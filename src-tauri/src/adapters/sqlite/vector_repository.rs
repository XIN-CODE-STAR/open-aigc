//! SqliteVectorRepository — VectorRepository 的 SQLite 实现。
//!
//! 向量以 f32 小端字节存 BLOB；搜索为全表暴力余弦（画布/语义库量级
//! 几千条以内足够快，避免引入向量数据库依赖）。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::adapters::sqlite::database::open_database;
use crate::ports::vector_repository::{
    VectorEntry, VectorError, VectorMetadata, VectorRepository, VectorSearchRequest,
    VectorSearchResult,
};

pub struct SqliteVectorRepository {
    connection: Mutex<Connection>,
}

impl SqliteVectorRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, VectorError> {
        let connection =
            open_database(database_path).map_err(|e| VectorError::StorageFailed(e.to_string()))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect()
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

impl VectorRepository for SqliteVectorRepository {
    fn store(&self, entry: &VectorEntry) -> Result<(), VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::StorageFailed("connection lock poisoned".to_owned()))?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        connection
            .execute(
                "INSERT INTO vector_entries (id, embedding, dimension, asset_id, content_type, text, extra_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET embedding = excluded.embedding, dimension = excluded.dimension, asset_id = excluded.asset_id, content_type = excluded.content_type, text = excluded.text, extra_json = excluded.extra_json, updated_at = excluded.updated_at",
                params![
                    entry.id,
                    Self::embedding_to_bytes(&entry.embedding),
                    entry.embedding.len() as i64,
                    entry.metadata.asset_id,
                    entry.metadata.content_type,
                    entry.metadata.text,
                    entry.metadata.extra.as_ref().map(|v| v.to_string()),
                    now,
                ],
            )
            .map_err(|e| VectorError::StorageFailed(e.to_string()))?;
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<VectorEntry>, VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::RetrievalFailed("connection lock poisoned".to_owned()))?;
        connection
            .query_row(
                "SELECT id, embedding, asset_id, content_type, text, extra_json FROM vector_entries WHERE id = ?1",
                params![id],
                |row| {
                    let embedding_bytes: Vec<u8> = row.get(1)?;
                    let extra_json: Option<String> = row.get(5)?;
                    Ok(VectorEntry {
                        id: row.get(0)?,
                        embedding: Self::bytes_to_embedding(&embedding_bytes),
                        metadata: VectorMetadata {
                            asset_id: row.get(2)?,
                            content_type: row.get(3)?,
                            text: row.get(4)?,
                            extra: extra_json.and_then(|s| serde_json::from_str(&s).ok()),
                        },
                    })
                },
            )
            .optional()
            .map_err(|e| VectorError::RetrievalFailed(e.to_string()))
    }

    fn delete(&self, id: &str) -> Result<(), VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::DeletionFailed("connection lock poisoned".to_owned()))?;
        connection
            .execute("DELETE FROM vector_entries WHERE id = ?1", params![id])
            .map_err(|e| VectorError::DeletionFailed(e.to_string()))?;
        Ok(())
    }

    fn delete_by_asset(&self, asset_id: &str) -> Result<(), VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::DeletionFailed("connection lock poisoned".to_owned()))?;
        connection
            .execute(
                "DELETE FROM vector_entries WHERE asset_id = ?1",
                params![asset_id],
            )
            .map_err(|e| VectorError::DeletionFailed(e.to_string()))?;
        Ok(())
    }

    fn search(
        &self,
        request: &VectorSearchRequest,
    ) -> Result<Vec<VectorSearchResult>, VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::SearchFailed("connection lock poisoned".to_owned()))?;
        let mut statement = connection
            .prepare(
                "SELECT id, embedding, asset_id, content_type, text, extra_json FROM vector_entries",
            )
            .map_err(|e| VectorError::SearchFailed(e.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                let embedding_bytes: Vec<u8> = row.get(1)?;
                let extra_json: Option<String> = row.get(5)?;
                Ok(VectorEntry {
                    id: row.get(0)?,
                    embedding: Self::bytes_to_embedding(&embedding_bytes),
                    metadata: VectorMetadata {
                        asset_id: row.get(2)?,
                        content_type: row.get(3)?,
                        text: row.get(4)?,
                        extra: extra_json.and_then(|s| serde_json::from_str(&s).ok()),
                    },
                })
            })
            .map_err(|e| VectorError::SearchFailed(e.to_string()))?;

        let min_score = request.min_score.unwrap_or(0.0);
        let mut results: Vec<VectorSearchResult> = rows
            .flatten()
            .filter(|entry| {
                request
                    .content_type
                    .as_ref()
                    .is_none_or(|filter| entry.metadata.content_type == *filter)
            })
            .filter(|entry| {
                request
                    .asset_ids
                    .as_ref()
                    .is_none_or(|ids| ids.iter().any(|id| id == &entry.metadata.asset_id))
            })
            .map(|entry| {
                let score = cosine(&request.embedding, &entry.embedding);
                VectorSearchResult { entry, score }
            })
            .filter(|result| result.score >= min_score)
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(request.limit);
        Ok(results)
    }

    fn count(&self) -> Result<usize, VectorError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VectorError::RetrievalFailed("connection lock poisoned".to_owned()))?;
        connection
            .query_row("SELECT COUNT(*) FROM vector_entries", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(|e| VectorError::RetrievalFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SqliteVectorRepository {
        let dir = tempfile::tempdir().unwrap();
        SqliteVectorRepository::open(dir.path().join("test.sqlite3")).unwrap()
    }

    fn entry(id: &str, vector: Vec<f32>) -> VectorEntry {
        VectorEntry {
            id: id.to_owned(),
            embedding: vector,
            metadata: VectorMetadata {
                asset_id: "asset-1".to_owned(),
                content_type: "canvas-node".to_owned(),
                text: format!("text of {id}"),
                extra: None,
            },
        }
    }

    #[test]
    fn store_get_roundtrip() {
        let repo = setup();
        repo.store(&entry("v1", vec![0.1, 0.2, 0.3])).unwrap();
        let fetched = repo.get("v1").unwrap().unwrap();
        assert_eq!(fetched.id, "v1");
        assert_eq!(fetched.embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(fetched.metadata.content_type, "canvas-node");
        assert_eq!(repo.count().unwrap(), 1);
    }

    #[test]
    fn search_ranks_by_cosine_and_filters() {
        let repo = setup();
        repo.store(&entry("close", vec![1.0, 0.0])).unwrap();
        repo.store(&entry("far", vec![0.0, 1.0])).unwrap();
        repo.store(&entry("opposite", vec![-1.0, 0.0])).unwrap();

        let results = repo
            .search(&VectorSearchRequest {
                embedding: vec![1.0, 0.0],
                limit: 10,
                min_score: Some(-1.0),
                asset_ids: None,
                content_type: Some("canvas-node".to_owned()),
            })
            .unwrap();
        assert_eq!(results[0].entry.id, "close");
        assert!(results.iter().any(|r| r.entry.id == "far"));

        let filtered = repo
            .search(&VectorSearchRequest {
                embedding: vec![1.0, 0.0],
                limit: 10,
                min_score: Some(0.5),
                asset_ids: None,
                content_type: None,
            })
            .unwrap();
        assert_eq!(filtered.len(), 1, "min_score 应过滤掉低分项");
    }

    #[test]
    fn upsert_and_delete_by_asset() {
        let repo = setup();
        repo.store(&entry("v1", vec![1.0])).unwrap();
        repo.store(&entry("v1", vec![0.5])).unwrap();
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(repo.get("v1").unwrap().unwrap().embedding, vec![0.5]);

        repo.delete_by_asset("asset-1").unwrap();
        assert_eq!(repo.count().unwrap(), 0);
    }
}
