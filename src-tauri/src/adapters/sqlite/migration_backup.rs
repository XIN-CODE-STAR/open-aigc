use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{backup::Progress, Connection, MAIN_DB};
use uuid::Uuid;

use super::database::{current_schema_version, verify_database};
use crate::ports::persistence::PersistenceError;

pub(super) fn create_migration_backup(
    connection: &Connection,
    database_path: &Path,
    current_version: i32,
    latest_version: i32,
) -> Result<PathBuf, PersistenceError> {
    let backup_path = migration_backup_path(database_path, current_version, latest_version)?;
    let backup_directory = backup_path.parent().ok_or_else(|| {
        PersistenceError::new(
            "resolve migration backup directory",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "migration backup path has no parent directory",
            ),
        )
    })?;
    fs::create_dir_all(backup_directory)
        .map_err(|error| PersistenceError::new("create migration backup directory", error))?;

    let temporary_path = backup_directory.join(format!(".{}.tmp", Uuid::new_v4()));
    if let Err(error) = connection.backup(MAIN_DB, &temporary_path, None) {
        let _ = fs::remove_file(&temporary_path);
        return Err(PersistenceError::new(
            "create pre-migration database backup",
            error,
        ));
    }

    if let Err(error) = verify_migration_backup(&temporary_path, current_version) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = publish_migration_backup(&temporary_path, &backup_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(backup_path)
}

pub(super) fn migration_backup_path(
    database_path: &Path,
    current_version: i32,
    latest_version: i32,
) -> Result<PathBuf, PersistenceError> {
    let parent = database_path.parent().ok_or_else(|| {
        PersistenceError::new(
            "resolve migration backup path",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "database path has no parent directory",
            ),
        )
    })?;
    let file_name = database_path.file_name().ok_or_else(|| {
        PersistenceError::new(
            "resolve migration backup path",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "database path has no file name",
            ),
        )
    })?;

    Ok(parent.join("backups").join(format!(
        "{}.pre-migration-v{current_version}-to-v{latest_version}.sqlite3",
        file_name.to_string_lossy()
    )))
}

pub(super) fn restore_migration_backup(
    connection: &mut Connection,
    backup_path: &Path,
    expected_version: i32,
) -> Result<(), PersistenceError> {
    connection
        .restore(MAIN_DB, backup_path, None::<fn(Progress)>)
        .map_err(|error| PersistenceError::new("restore migration backup", error))?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| PersistenceError::new("checkpoint restored database", error))?;
    verify_database(connection)?;

    let actual_version = current_schema_version(connection)?;
    if actual_version != expected_version {
        return Err(PersistenceError::new(
            "verify restored schema version",
            std::io::Error::other(format!(
                "expected restored schema version {expected_version}, found {actual_version}"
            )),
        ));
    }

    Ok(())
}

fn verify_migration_backup(
    backup_path: &Path,
    expected_version: i32,
) -> Result<(), PersistenceError> {
    let connection = Connection::open(backup_path)
        .map_err(|error| PersistenceError::new("open migration backup", error))?;
    verify_database(&connection)?;
    let actual_version = current_schema_version(&connection)?;

    if actual_version != expected_version {
        return Err(PersistenceError::new(
            "verify migration backup schema version",
            std::io::Error::other(format!(
                "expected schema version {expected_version}, found {actual_version}"
            )),
        ));
    }

    Ok(())
}

fn publish_migration_backup(
    temporary_path: &Path,
    backup_path: &Path,
) -> Result<(), PersistenceError> {
    let previous_path = backup_path.with_extension("sqlite3.previous");
    let moved_previous = if backup_path.exists() {
        if previous_path.exists() {
            fs::remove_file(&previous_path).map_err(|error| {
                PersistenceError::new("remove previous migration backup", error)
            })?;
        }
        fs::rename(backup_path, &previous_path)
            .map_err(|error| PersistenceError::new("rotate migration backup", error))?;
        true
    } else {
        false
    };

    if let Err(error) = fs::rename(temporary_path, backup_path) {
        if moved_previous {
            let _ = fs::rename(&previous_path, backup_path);
        }
        let _ = fs::remove_file(temporary_path);
        return Err(PersistenceError::new("publish migration backup", error));
    }

    if previous_path.exists() {
        let _ = fs::remove_file(previous_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::create_migration_backup;

    #[test]
    fn replaces_a_stale_backup_after_a_retry() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migration(version INTEGER PRIMARY KEY);\
                 INSERT INTO schema_migration(version) VALUES (1);\
                 CREATE TABLE recovery_marker(value TEXT NOT NULL);\
                 INSERT INTO recovery_marker(value) VALUES ('first');",
            )
            .unwrap();

        let backup_path = create_migration_backup(&connection, &path, 1, 2).unwrap();
        connection
            .execute("UPDATE recovery_marker SET value = 'second'", [])
            .unwrap();
        create_migration_backup(&connection, &path, 1, 2).unwrap();

        let backup = Connection::open(&backup_path).unwrap();
        let marker: String = backup
            .query_row("SELECT value FROM recovery_marker", [], |row| row.get(0))
            .unwrap();

        assert_eq!(marker, "second");
        assert!(!backup_path.with_extension("sqlite3.previous").exists());
    }
}
