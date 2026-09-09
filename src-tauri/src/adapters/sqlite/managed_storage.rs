use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{domain::workspace::ManagedStorageHealth, ports::persistence::PersistenceError};

pub(crate) const MANAGED_STORAGE_DIRECTORY: &str = "managed-files";
pub(crate) const ASSETS_DIRECTORY: &str = "assets";
pub(crate) const THUMBNAILS_DIRECTORY: &str = "thumbnails";
pub(crate) const STAGING_DIRECTORY: &str = "staging";
pub(crate) const QUARANTINE_DIRECTORY: &str = "quarantine";

pub(crate) fn managed_storage_root(workspace_directory: &Path) -> PathBuf {
    workspace_directory.join(MANAGED_STORAGE_DIRECTORY)
}

pub(crate) fn assets_root(workspace_directory: &Path) -> PathBuf {
    managed_storage_root(workspace_directory).join(ASSETS_DIRECTORY)
}

pub(crate) fn staging_root(workspace_directory: &Path) -> PathBuf {
    managed_storage_root(workspace_directory).join(STAGING_DIRECTORY)
}

pub(super) fn storage_health(
    connection: &Connection,
    workspace_directory: &Path,
) -> Result<ManagedStorageHealth, PersistenceError> {
    let root = managed_storage_root(workspace_directory);
    let directories_ready = ensure_directories(&root).is_ok() && required_directories_exist(&root);
    let writable = directories_ready && write_probe(&root).is_ok();
    let manifest_ready = manifest_table_exists(connection)?;
    let (asset_count, total_bytes, missing_asset_count) = if manifest_ready {
        manifest_counts(connection)?
    } else {
        (0, 0, 0)
    };
    let checked_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| PersistenceError::new("format storage health timestamp", error))?;

    Ok(ManagedStorageHealth {
        directories_ready,
        writable,
        manifest_ready,
        asset_count,
        total_bytes,
        missing_asset_count,
        checked_at,
    })
}

fn ensure_directories(root: &Path) -> Result<(), std::io::Error> {
    for directory in required_directories(root) {
        fs::create_dir_all(directory)?;
    }

    Ok(())
}

fn required_directories(root: &Path) -> [PathBuf; 5] {
    [
        root.to_path_buf(),
        root.join(ASSETS_DIRECTORY),
        root.join(THUMBNAILS_DIRECTORY),
        root.join(STAGING_DIRECTORY),
        root.join(QUARANTINE_DIRECTORY),
    ]
}

fn required_directories_exist(root: &Path) -> bool {
    required_directories(root)
        .iter()
        .all(|directory| directory.is_dir())
}

fn write_probe(root: &Path) -> Result<(), std::io::Error> {
    let probe_path = root
        .join(STAGING_DIRECTORY)
        .join(format!(".health-{}.tmp", Uuid::new_v4()));
    let mut file = fs::File::create(&probe_path)?;
    file.write_all(b"ok")?;
    file.sync_all()?;
    drop(file);
    fs::remove_file(probe_path)?;

    Ok(())
}

fn manifest_table_exists(connection: &Connection) -> Result<bool, PersistenceError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'asset_manifest')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists == 1)
        .map_err(|error| PersistenceError::new("check asset manifest table", error))
}

fn manifest_counts(connection: &Connection) -> Result<(i64, i64, i64), PersistenceError> {
    connection
        .query_row(
            r#"
            SELECT
              COUNT(*),
              COALESCE(SUM(size_bytes), 0),
              COALESCE(SUM(CASE WHEN integrity_status = 'missing' THEN 1 ELSE 0 END), 0)
            FROM asset_manifest
            WHERE deleted_at IS NULL
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map(|counts| counts.unwrap_or((0, 0, 0)))
        .map_err(|error| PersistenceError::new("read asset manifest health", error))
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::{managed_storage_root, storage_health};

    #[test]
    fn reports_unready_manifest_without_failing_the_storage_probe() {
        let directory = tempdir().unwrap();
        let connection = Connection::open_in_memory().unwrap();

        let health = storage_health(&connection, directory.path()).unwrap();

        assert!(health.directories_ready);
        assert!(health.writable);
        assert!(!health.manifest_ready);
        assert_eq!(health.asset_count, 0);
        assert!(managed_storage_root(directory.path())
            .join("staging")
            .is_dir());
    }
}
