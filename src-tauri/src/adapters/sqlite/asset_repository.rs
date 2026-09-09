use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, TransactionBehavior};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    domain::assets::{
        AssetDraft, AssetFilter, AssetImportSummary, AssetKind, AssetRecord,
        AssetReverificationSummary, IntegrityStatus, StorageNamespace,
    },
    ports::{
        asset_repository::{AssetRepository, AssetRepositoryError, ImportOptions},
        persistence::PersistenceError,
    },
};

use super::asset_import::run_import;
use super::asset_integrity::run_reverify;

pub struct SqliteAssetRepository {
    pub(super) connection: Connection,
    workspace_directory: PathBuf,
}

impl SqliteAssetRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let database_path = database_path.as_ref();
        let workspace_directory = database_path
            .parent()
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve asset repository directory",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?
            .to_path_buf();
        let connection = open_connection(database_path)?;

        Ok(Self {
            connection,
            workspace_directory,
        })
    }
}

fn open_connection(database_path: &Path) -> Result<Connection, PersistenceError> {
    use crate::adapters::sqlite::database::open_database;
    open_database(database_path)
}

impl AssetRepository for SqliteAssetRepository {
    fn list(&mut self, filter: &AssetFilter) -> Result<Vec<AssetRecord>, PersistenceError> {
        let mut query = String::from(
            r#"
            SELECT
              id,
              storage_namespace,
              asset_kind,
              display_name,
              relative_path,
              size_bytes,
              sha256,
              mime_type,
              integrity_status,
              metadata_json,
              origin_device_id,
              revision,
              created_at,
              updated_at
            FROM asset_manifest
            WHERE deleted_at IS NULL
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_index = 1;

        if let Some(search) = filter.search.as_deref() {
            query.push_str(&format!(
                " AND display_name LIKE ?{param_index} ESCAPE '\\'"
            ));
            params_vec.push(Box::new(escape_like_pattern(search)));
            param_index += 1;
        }
        if let Some(namespace) = filter.storage_namespace {
            query.push_str(&format!(" AND storage_namespace = ?{param_index}"));
            params_vec.push(Box::new(namespace.as_str().to_owned()));
            param_index += 1;
        }
        if let Some(kind) = filter.asset_kind {
            query.push_str(&format!(" AND asset_kind = ?{param_index}"));
            params_vec.push(Box::new(kind.as_str().to_owned()));
            param_index += 1;
        }
        if let Some(status) = filter.integrity_status {
            query.push_str(&format!(" AND integrity_status = ?{param_index}"));
            params_vec.push(Box::new(status.as_str().to_owned()));
            param_index += 1;
        }

        query.push_str(&format!(
            " ORDER BY updated_at DESC, created_at DESC LIMIT ?{param_index}"
        ));
        params_vec.push(Box::new(filter.limit));

        let mut statement = self
            .connection
            .prepare(&query)
            .map_err(|error| PersistenceError::new("prepare asset list", error))?;
        let params_iter = params_from_iter(params_vec.iter().map(|b| b.as_ref()));
        let rows = statement
            .query_map(params_iter, map_asset_record)
            .map_err(|error| PersistenceError::new("query asset list", error))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|error| PersistenceError::new("read asset row", error))?);
        }
        Ok(records)
    }

    fn get(&mut self, asset_id: &str) -> Result<Option<AssetRecord>, PersistenceError> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT
                  id,
                  storage_namespace,
                  asset_kind,
                  display_name,
                  relative_path,
                  size_bytes,
                  sha256,
                  mime_type,
                  integrity_status,
                  metadata_json,
                  origin_device_id,
                  revision,
                  created_at,
                  updated_at
                FROM asset_manifest
                WHERE deleted_at IS NULL AND id = ?1
                "#,
                params![asset_id],
                map_asset_record,
            )
            .optional()
            .map_err(|error| PersistenceError::new("read asset", error))?;
        Ok(record)
    }

    fn import(
        &mut self,
        source_paths: Vec<String>,
        namespace: &str,
        options: ImportOptions,
    ) -> Result<AssetImportSummary, AssetRepositoryError> {
        let device_id = current_device_id(&self.connection)?;
        run_import(
            &mut self.connection,
            &self.workspace_directory,
            source_paths,
            namespace,
            options,
            device_id.as_deref(),
        )
    }

    fn reverify(&mut self) -> Result<AssetReverificationSummary, AssetRepositoryError> {
        run_reverify(&mut self.connection, &self.workspace_directory)
    }

    fn delete(&mut self, asset_id: &str) -> Result<(), AssetRepositoryError> {
        let now = now_rfc3339()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin asset delete transaction", e))?;
        // 级联软删关联的 resource_association 记录。
        tx.execute(
            "UPDATE resource_association SET deleted_at = ?1, updated_at = ?1 WHERE asset_id = ?2 AND deleted_at IS NULL",
            params![now, asset_id],
        )
        .map_err(|e| PersistenceError::new("cascade delete resource associations", e))?;
        let affected = tx
            .execute(
                "UPDATE asset_manifest SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now, asset_id],
            )
            .map_err(|e| PersistenceError::new("soft delete asset", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::new("commit asset delete", e))?;
        if affected == 0 {
            // 不视为错误：资产可能已软删或不存在。
            return Ok(());
        }
        Ok(())
    }
}

/// 读取当前工作空间的本机设备 id，用作导入资产的 origin_device_id。
/// 工作区未初始化时返回 None，导入流程会写入 NULL。
fn current_device_id(connection: &Connection) -> Result<Option<String>, PersistenceError> {
    use rusqlite::OptionalExtension;
    connection
        .query_row(
            "SELECT id FROM device ORDER BY first_seen_at ASC, id ASC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::new("read current device", error))
}

/// 在已有事务里写入一条 manifest 记录。返回构造好的 AssetRecord。
/// 调用方负责 commit / rollback，以及在 commit 成功后完成文件移动。
pub(super) fn insert_asset_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    draft: &AssetDraft,
    device_id: Option<&str>,
    created_at: &str,
) -> Result<AssetRecord, PersistenceError> {
    let id = Uuid::new_v4().to_string();
    transaction
        .execute(
            r#"
            INSERT INTO asset_manifest (
              id,
              storage_namespace,
              asset_kind,
              display_name,
              relative_path,
              size_bytes,
              sha256,
              mime_type,
              integrity_status,
              metadata_json,
              origin_device_id,
              revision,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)
            "#,
            params![
                id,
                draft.storage_namespace.as_str(),
                draft.asset_kind.as_str(),
                draft.display_name,
                draft.relative_path,
                draft.size_bytes,
                draft.sha256,
                draft.mime_type,
                IntegrityStatus::Valid.as_str(),
                draft.metadata_json,
                device_id,
                created_at,
            ],
        )
        .map_err(|error| PersistenceError::new("insert asset manifest", error))?;

    Ok(AssetRecord {
        id,
        storage_namespace: draft.storage_namespace,
        asset_kind: draft.asset_kind,
        display_name: draft.display_name.clone(),
        relative_path: draft.relative_path.clone(),
        size_bytes: draft.size_bytes,
        sha256: draft.sha256.clone(),
        mime_type: draft.mime_type.clone(),
        integrity_status: IntegrityStatus::Valid,
        metadata_json: draft.metadata_json.clone(),
        origin_device_id: device_id.map(str::to_owned),
        revision: 1,
        created_at: created_at.to_owned(),
        updated_at: created_at.to_owned(),
    })
}

/// 暴露给 asset_import 模块的内部事务入口：以 Immediate 行为开事务，
/// 让导入和 manifest 写入使用一致的隔离级别。
pub(super) fn begin_import_tx(
    connection: &mut Connection,
) -> Result<rusqlite::Transaction<'_>, PersistenceError> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| PersistenceError::new("begin asset import transaction", error))
}

pub(super) fn now_rfc3339() -> Result<String, PersistenceError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| PersistenceError::new("format asset timestamp", error))
}

pub(super) fn delete_manifest_by_id(
    connection: &Connection,
    asset_id: &str,
) -> Result<(), PersistenceError> {
    connection
        .execute(
            "DELETE FROM asset_manifest WHERE id = ?1",
            params![asset_id],
        )
        .map_err(|error| PersistenceError::new("delete asset manifest", error))?;
    Ok(())
}

/// 资源库未在 search 索引上建 LIKE，且用户搜索串不会包含通配符字面值时的转义。
fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(super) fn map_asset_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRecord> {
    let storage_namespace_str: String = row.get(1)?;
    let asset_kind_str: String = row.get(2)?;
    let integrity_status_str: String = row.get(8)?;
    let storage_namespace = StorageNamespace::parse(&storage_namespace_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let asset_kind = AssetKind::parse(&asset_kind_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let integrity_status = IntegrityStatus::parse(&integrity_status_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(AssetRecord {
        id: row.get(0)?,
        storage_namespace,
        asset_kind,
        display_name: row.get(3)?,
        relative_path: row.get(4)?,
        size_bytes: row.get(5)?,
        sha256: row.get(6)?,
        mime_type: row.get(7)?,
        integrity_status,
        metadata_json: row.get(9)?,
        origin_device_id: row.get(10)?,
        revision: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::assets::{AssetDraft, AssetFilter};
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::asset_repository::{AssetRepository, ImportOptions};
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed_repository() -> (tempfile::TempDir, SqliteAssetRepository) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        // 跑一遍 workspace 仓库以确保迁移已执行并建好 manifest 表，
        // 同时初始化工作空间以建立 device 记录，让 asset_manifest 的外键有真实引用。
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        let request = NewWorkspace::try_new("测试工作空间", "测试教师").unwrap();
        workspace.initialize(&request).unwrap();
        drop(workspace);
        let repository = SqliteAssetRepository::open(&path).unwrap();
        (directory, repository)
    }

    #[test]
    fn returns_empty_list_when_manifest_is_empty() {
        let (_directory, mut repository) = seed_repository();

        let filter = AssetFilter::try_new(None, None, None, None, None).unwrap();
        let records = repository.list(&filter).unwrap();

        assert!(records.is_empty());
    }

    #[test]
    fn inserts_and_reads_back_asset_records() {
        let (_directory, mut repository) = seed_repository();
        let created_at = now_rfc3339().unwrap();
        let device_id = current_device_id(&repository.connection).unwrap().unwrap();

        let draft = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "示例图片".to_owned(),
            "assets/sample.png".to_owned(),
            4096,
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned()),
            Some("image/png".to_owned()),
            None,
        )
        .unwrap();

        let transaction = begin_import_tx(&mut repository.connection).unwrap();
        let inserted =
            insert_asset_in_tx(&transaction, &draft, Some(&device_id), &created_at).unwrap();
        transaction.commit().unwrap();

        let fetched = repository.get(&inserted.id).unwrap().unwrap();
        assert_eq!(fetched, inserted);
        assert_eq!(fetched.integrity_status, IntegrityStatus::Valid);
        assert_eq!(
            fetched.origin_device_id.as_deref(),
            Some(device_id.as_str())
        );

        let filter =
            AssetFilter::try_new(None, None, Some("image".to_owned()), None, None).unwrap();
        assert_eq!(repository.list(&filter).unwrap().len(), 1);
    }

    #[test]
    fn filters_by_namespace_and_status() {
        let (_directory, mut repository) = seed_repository();
        let created_at = now_rfc3339().unwrap();

        let drafts = [
            (
                "teaching-resource",
                AssetKind::Document,
                "教案".to_owned(),
                "assets/lesson.pdf".to_owned(),
            ),
            (
                "workspace",
                AssetKind::Image,
                "封面".to_owned(),
                "assets/cover.png".to_owned(),
            ),
        ];

        for (namespace, kind, name, path) in drafts {
            let draft =
                AssetDraft::try_new(namespace.to_owned(), kind, name, path, 1, None, None, None)
                    .unwrap();
            let tx = begin_import_tx(&mut repository.connection).unwrap();
            insert_asset_in_tx(&tx, &draft, None, &created_at).unwrap();
            tx.commit().unwrap();
        }

        let filter =
            AssetFilter::try_new(None, Some("teaching-resource".to_owned()), None, None, None)
                .unwrap();
        let records = repository.list(&filter).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].display_name, "教案");
    }

    #[test]
    fn list_omits_soft_deleted_assets() {
        let (_directory, mut repository) = seed_repository();
        let created_at = now_rfc3339().unwrap();

        let draft = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "已删除".to_owned(),
            "assets/deleted.png".to_owned(),
            1,
            None,
            None,
            None,
        )
        .unwrap();
        let tx = begin_import_tx(&mut repository.connection).unwrap();
        let inserted = insert_asset_in_tx(&tx, &draft, None, &created_at).unwrap();
        tx.commit().unwrap();

        repository
            .connection
            .execute(
                "UPDATE asset_manifest SET deleted_at = ?1 WHERE id = ?2",
                params![created_at, inserted.id],
            )
            .unwrap();

        let filter = AssetFilter::try_new(None, None, None, None, None).unwrap();
        assert!(repository.list(&filter).unwrap().is_empty());
        assert!(repository.get(&inserted.id).unwrap().is_none());
    }

    #[test]
    fn import_rejects_unknown_namespace_without_touching_database() {
        let (_directory, mut repository) = seed_repository();
        let summary = repository
            .import(Vec::new(), "not-a-namespace", ImportOptions::default())
            .unwrap();

        assert!(summary.is_empty());
    }

    #[test]
    fn import_persists_real_files_and_skips_duplicates() {
        let (directory, mut repository) = seed_repository();
        let source_dir = directory.path().join("sources");
        std::fs::create_dir_all(&source_dir).unwrap();

        let first_source = source_dir.join("preview.png");
        std::fs::write(&first_source, b"asset-bytes").unwrap();
        let first_path = first_source.to_string_lossy().to_string();

        let summary = repository
            .import(
                vec![first_path.clone()],
                "workspace",
                ImportOptions::default(),
            )
            .unwrap();

        assert_eq!(summary.imported.len(), 1);
        assert_eq!(summary.skipped.len(), 0);
        assert_eq!(summary.failures.len(), 0);
        let imported = &summary.imported[0];
        assert_eq!(imported.asset.display_name, "preview.png");
        assert_eq!(imported.asset.size_bytes, b"asset-bytes".len() as i64);
        assert!(imported.asset.sha256.is_some());
        assert_eq!(imported.asset.integrity_status, IntegrityStatus::Valid);

        // manifest 记录可被 list 查到
        let filter = AssetFilter::try_new(None, None, None, None, None).unwrap();
        assert_eq!(repository.list(&filter).unwrap().len(), 1);

        // assets 目录下确实有文件
        let assets_dir = directory.path().join("managed-files").join("assets");
        assert_eq!(std::fs::read_dir(&assets_dir).unwrap().count(), 1);

        // 再次导入同一文件应该命中重复检测
        let repeat = repository
            .import(vec![first_path], "workspace", ImportOptions::default())
            .unwrap();
        assert_eq!(repeat.imported.len(), 0);
        assert_eq!(repeat.skipped.len(), 1);
        assert_eq!(repeat.failures.len(), 0);
        assert!(repeat.skipped[0].existing_asset_id.is_some());

        // manifest 仍然只有一条
        assert_eq!(repository.list(&filter).unwrap().len(), 1);
    }

    #[test]
    fn import_marks_missing_source_as_failure_without_aborting_batch() {
        let (directory, mut repository) = seed_repository();
        let source_dir = directory.path().join("sources");
        std::fs::create_dir_all(&source_dir).unwrap();

        let good_source = source_dir.join("good.txt");
        std::fs::write(&good_source, b"hello").unwrap();
        let good_path = good_source.to_string_lossy().to_string();
        let missing_path = source_dir.join("missing.txt").to_string_lossy().to_string();

        let summary = repository
            .import(
                vec![missing_path, good_path],
                "workspace",
                ImportOptions::default(),
            )
            .unwrap();

        assert_eq!(summary.imported.len(), 1);
        assert_eq!(summary.failures.len(), 1);
        assert!(summary.failures[0].reason.contains("无法读取源文件"));
    }

    #[test]
    fn import_rejects_file_exceeding_size_limit() {
        let (directory, mut repository) = seed_repository();
        let source_dir = directory.path().join("sources");
        std::fs::create_dir_all(&source_dir).unwrap();

        let oversized = source_dir.join("big.bin");
        std::fs::write(&oversized, b"abcdef").unwrap();
        let path = oversized.to_string_lossy().to_string();

        let options = ImportOptions { max_bytes: 3 };
        let summary = repository.import(vec![path], "workspace", options).unwrap();

        assert_eq!(summary.imported.len(), 0);
        assert_eq!(summary.failures.len(), 1);
        assert!(summary.failures[0].reason.contains("超过"));
    }
}
