use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, TransactionBehavior};

use sha2::{Digest, Sha256};

use crate::{
    adapters::sqlite::{
        asset_repository::{map_asset_record, now_rfc3339},
        managed_storage::{assets_root, managed_storage_root},
    },
    domain::assets::{AssetRecord, AssetReverificationSummary, IntegrityStatus},
    ports::{
        asset_repository::{AssetRepositoryError, IntegrityError},
        persistence::PersistenceError,
    },
};

const READ_BUFFER_SIZE: usize = 65_536;

/// 遍历所有未软删的 manifest 记录，逐条校验受管文件。
/// `quarantined` 状态保持不变（由人工或后续流程处理），其余状态按实际检查结果更新。
pub(super) fn run_reverify(
    connection: &mut Connection,
    workspace_directory: &Path,
) -> Result<AssetReverificationSummary, AssetRepositoryError> {
    let managed_root = managed_storage_root(workspace_directory);
    let assets_directory = assets_root(workspace_directory);
    // assets 目录可能尚未创建（无资产时）。若不存在，所有记录标记为 missing。
    let assets_dir_exists = assets_directory.is_dir();

    let records = load_active_records(connection)?;
    let mut summary = AssetReverificationSummary::empty();

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| PersistenceError::new("begin asset reverify transaction", error))?;

    for record in records {
        if record.integrity_status == IntegrityStatus::Quarantined {
            summary.record(IntegrityStatus::Quarantined);
            continue;
        }

        let new_status = determine_status(&managed_root, assets_dir_exists, &record, &mut summary)?;

        if new_status != record.integrity_status {
            update_status_in_tx(&transaction, &record.id, new_status)?;
        }
    }

    transaction
        .commit()
        .map_err(|error| PersistenceError::new("commit asset reverify", error))?;

    Ok(summary)
}

fn load_active_records(connection: &Connection) -> Result<Vec<AssetRecord>, PersistenceError> {
    let mut statement = connection
        .prepare(
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
        )
        .map_err(|error| PersistenceError::new("prepare reverify list", error))?;
    let rows = statement
        .query_map([], map_asset_record)
        .map_err(|error| PersistenceError::new("query reverify list", error))?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| PersistenceError::new("read reverify row", error))?);
    }
    Ok(records)
}

/// 根据文件存在性和哈希确定新的完整性状态，并累加统计。
/// `managed_root` 是受管存储根目录（`<workspace>/managed-files`）；
/// `relative_path` 形如 `assets/<filename>`，直接拼接到受管根上得到绝对路径。
fn determine_status(
    managed_root: &Path,
    assets_dir_exists: bool,
    record: &AssetRecord,
    summary: &mut AssetReverificationSummary,
) -> Result<IntegrityStatus, AssetRepositoryError> {
    if !assets_dir_exists {
        let status = IntegrityStatus::Missing;
        summary.record(status);
        return Ok(status);
    }

    let absolute = resolve_asset_path(managed_root, &record.relative_path)?;
    if !absolute.exists() {
        let status = IntegrityStatus::Missing;
        summary.record(status);
        return Ok(status);
    }

    let status = match record.sha256.as_deref() {
        Some(expected_hash) => match recompute_sha256(&absolute) {
            Ok(actual) if actual == expected_hash => IntegrityStatus::Valid,
            Ok(_) => IntegrityStatus::Corrupt,
            Err(error) => {
                return Err(AssetRepositoryError::Integrity(IntegrityError::Io {
                    operation: "recompute asset hash",
                    source: error,
                }));
            }
        },
        None => {
            // 没有哈希的资产（如空文件或早期未计算的）按文件存在性判定。
            IntegrityStatus::Valid
        }
    };
    summary.record(status);
    Ok(status)
}

/// 把 manifest 的 relative_path（形如 `assets/<name>`）拼成受管目录下的绝对路径。
/// 额外拒绝绝对路径、`..` 跳转等危险输入，避免逃逸受管目录。
fn resolve_asset_path(
    assets_directory: &Path,
    relative_path: &str,
) -> Result<PathBuf, AssetRepositoryError> {
    if relative_path.is_empty() || relative_path.starts_with('/') || relative_path.contains('\\') {
        return Err(AssetRepositoryError::Persistence(PersistenceError::new(
            "resolve asset path",
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "asset relative path is malformed",
            ),
        )));
    }
    let mut full = assets_directory.to_path_buf();
    for segment in relative_path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(AssetRepositoryError::Persistence(PersistenceError::new(
                "resolve asset path",
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "asset relative path contains empty or traversal segment",
                ),
            )));
        }
        full.push(segment);
    }
    Ok(full)
}

fn recompute_sha256(path: &Path) -> Result<String, std::io::Error> {
    let mut reader = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; READ_BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn update_status_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    asset_id: &str,
    status: IntegrityStatus,
) -> Result<(), PersistenceError> {
    let updated_at = now_rfc3339()?;
    transaction
        .execute(
            "UPDATE asset_manifest SET integrity_status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), updated_at, asset_id],
        )
        .map_err(|error| PersistenceError::new("update asset integrity", error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::asset_repository::{
        begin_import_tx, insert_asset_in_tx, SqliteAssetRepository,
    };
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::assets::{AssetDraft, AssetKind, IntegrityStatus};
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::asset_repository::AssetRepository;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed() -> (tempfile::TempDir, SqliteAssetRepository) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
            .unwrap();
        drop(workspace);
        let repository = SqliteAssetRepository::open(&path).unwrap();
        (directory, repository)
    }

    fn insert_manual_record(
        repository: &mut SqliteAssetRepository,
        relative_path: &str,
        sha256: Option<&str>,
        status: IntegrityStatus,
    ) -> String {
        let created_at = now_rfc3339().unwrap();
        let draft = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "示例".to_owned(),
            relative_path.to_owned(),
            4,
            sha256.map(str::to_owned),
            None,
            None,
        )
        .unwrap();
        let tx = begin_import_tx(&mut repository.connection).unwrap();
        let record = insert_asset_in_tx(&tx, &draft, None, &created_at).unwrap();
        tx.execute(
            "UPDATE asset_manifest SET integrity_status = ?1 WHERE id = ?2",
            params![status.as_str(), record.id],
        )
        .unwrap();
        tx.commit().unwrap();
        record.id
    }

    #[test]
    fn reverify_marks_missing_when_asset_file_absent() {
        let (_directory, mut repository) = seed();
        insert_manual_record(
            &mut repository,
            "assets/missing.png",
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"),
            IntegrityStatus::Valid,
        );

        let summary = repository.reverify().unwrap();
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(summary.valid, 0);
    }

    #[test]
    fn reverify_marks_valid_when_file_matches_hash() {
        let (directory, mut repository) = seed();
        let assets_dir = directory.path().join("managed-files").join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        let bytes = b"hello";
        let asset_path = assets_dir.join("ok.png");
        std::fs::write(&asset_path, bytes).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let hash = format!("{:x}", hasher.finalize());

        insert_manual_record(
            &mut repository,
            "assets/ok.png",
            Some(&hash),
            IntegrityStatus::Unverified,
        );

        let summary = repository.reverify().unwrap();
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.valid, 1);
    }

    #[test]
    fn reverify_marks_corrupt_when_hash_differs() {
        let (directory, mut repository) = seed();
        let assets_dir = directory.path().join("managed-files").join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("tampered.png"), b"tampered-content").unwrap();

        insert_manual_record(
            &mut repository,
            "assets/tampered.png",
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"),
            IntegrityStatus::Valid,
        );

        let summary = repository.reverify().unwrap();
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.corrupt, 1);
    }

    #[test]
    fn reverify_preserves_quarantined_records() {
        let (directory, mut repository) = seed();
        let assets_dir = directory.path().join("managed-files").join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("quarantined.png"), b"x").unwrap();

        insert_manual_record(
            &mut repository,
            "assets/quarantined.png",
            None,
            IntegrityStatus::Quarantined,
        );

        let summary = repository.reverify().unwrap();
        assert_eq!(summary.quarantined, 1);
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.valid, 0);
    }
}
