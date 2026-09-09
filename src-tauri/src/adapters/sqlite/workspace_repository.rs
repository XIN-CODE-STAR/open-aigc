use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    adapters::sqlite::{database::open_database, managed_storage::storage_health},
    domain::workspace::{
        DatabaseHealth, NewWorkspace, UpdateWorkspaceNameResult, WorkspaceProfile, WorkspaceStatus,
    },
    ports::{persistence::PersistenceError, workspace_repository::WorkspaceRepository},
};

pub struct SqliteWorkspaceRepository {
    connection: Connection,
    workspace_directory: PathBuf,
}

impl SqliteWorkspaceRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let database_path = database_path.as_ref();
        let workspace_directory = database_path
            .parent()
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve workspace directory",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?
            .to_path_buf();
        let connection = open_database(database_path)?;

        Ok(Self {
            connection,
            workspace_directory,
        })
    }

    fn database_health(&self) -> Result<DatabaseHealth, PersistenceError> {
        let schema_version = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migration",
                [],
                |row| row.get(0),
            )
            .map_err(|error| PersistenceError::new("read schema version", error))?;
        let sqlite_version = self
            .connection
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))
            .map_err(|error| PersistenceError::new("read SQLite version", error))?;
        let journal_mode = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .map_err(|error| PersistenceError::new("read journal mode", error))?;
        let foreign_keys_enabled = self
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .map(|enabled| enabled == 1)
            .map_err(|error| PersistenceError::new("read foreign key mode", error))?;

        Ok(DatabaseHealth {
            schema_version,
            sqlite_version,
            journal_mode,
            foreign_keys_enabled,
        })
    }
}

impl WorkspaceRepository for SqliteWorkspaceRepository {
    fn get_status(&mut self) -> Result<WorkspaceStatus, PersistenceError> {
        let workspace = self
            .connection
            .query_row(
                r#"
                SELECT
                  workspace.id,
                  workspace.display_name,
                  teacher.id,
                  teacher.display_name,
                  workspace.created_at
                FROM workspace
                JOIN teacher ON teacher.id = workspace.owner_teacher_id
                WHERE workspace.singleton_key = 1
                "#,
                [],
                |row| {
                    Ok(WorkspaceProfile {
                        workspace_id: row.get(0)?,
                        workspace_name: row.get(1)?,
                        teacher_id: row.get(2)?,
                        teacher_name: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|error| PersistenceError::new("read workspace profile", error))?;

        Ok(WorkspaceStatus {
            initialized: workspace.is_some(),
            workspace,
            database: self.database_health()?,
            storage: storage_health(&self.connection, &self.workspace_directory)?,
        })
    }

    fn initialize(
        &mut self,
        workspace: &NewWorkspace,
    ) -> Result<WorkspaceProfile, PersistenceError> {
        let workspace_id = Uuid::new_v4().to_string();
        let teacher_id = Uuid::new_v4().to_string();
        let device_id = Uuid::new_v4().to_string();
        let created_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format workspace timestamp", error))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin workspace transaction", error))?;

        transaction
            .execute(
                r#"
                INSERT INTO device (
                  id, display_name, platform, app_version, first_seen_at, last_seen_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                "#,
                params![
                    device_id,
                    "本机设备",
                    std::env::consts::OS,
                    env!("CARGO_PKG_VERSION"),
                    created_at,
                ],
            )
            .map_err(|error| PersistenceError::new("create local device", error))?;

        transaction
            .execute(
                r#"
                INSERT INTO teacher (
                  id, display_name, origin_device_id, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?4)
                "#,
                params![teacher_id, workspace.teacher_name, device_id, created_at,],
            )
            .map_err(|error| PersistenceError::new("create teacher profile", error))?;

        transaction
            .execute(
                r#"
                INSERT INTO workspace (
                  singleton_key, id, display_name, owner_teacher_id, created_at, updated_at
                ) VALUES (1, ?1, ?2, ?3, ?4, ?4)
                "#,
                params![
                    workspace_id,
                    workspace.workspace_name,
                    teacher_id,
                    created_at,
                ],
            )
            .map_err(|error| PersistenceError::new("create workspace profile", error))?;

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit workspace transaction", error))?;

        Ok(WorkspaceProfile {
            workspace_id,
            workspace_name: workspace.workspace_name.clone(),
            teacher_id,
            teacher_name: workspace.teacher_name.clone(),
            created_at,
        })
    }

    fn update_workspace_name(
        &mut self,
        workspace_name: &str,
    ) -> Result<UpdateWorkspaceNameResult, PersistenceError> {
        // 校验记录存在（更新行数 > 0），但 0 行时不应报错——留给 service 层
        // 抛 `AppError::AlreadyInitialized`（与 initialize 行为一致）。
        let updated_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format workspace timestamp", error))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin rename workspace transaction", error))?;

        let updated_rows = transaction
            .execute(
                "UPDATE workspace SET display_name = ?1, updated_at = ?2 WHERE singleton_key = 1",
                params![workspace_name, updated_at],
            )
            .map_err(|error| PersistenceError::new("update workspace name", error))?;

        if updated_rows == 0 {
            transaction.rollback().map_err(|error| {
                PersistenceError::new("rollback rename workspace transaction", error)
            })?;
            return Ok(UpdateWorkspaceNameResult::NotInitialized);
        }

        let profile = transaction
            .query_row(
                r#"
                SELECT
                  workspace.id,
                  workspace.display_name,
                  teacher.id,
                  teacher.display_name,
                  workspace.created_at
                FROM workspace
                JOIN teacher ON teacher.id = workspace.owner_teacher_id
                WHERE workspace.singleton_key = 1
                "#,
                [],
                |row| {
                    Ok(WorkspaceProfile {
                        workspace_id: row.get(0)?,
                        workspace_name: row.get(1)?,
                        teacher_id: row.get(2)?,
                        teacher_name: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .map_err(|error| PersistenceError::new("read renamed workspace profile", error))?;

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit rename workspace transaction", error))?;

        Ok(UpdateWorkspaceNameResult::Updated(profile))
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::SqliteWorkspaceRepository;
    use crate::{
        domain::workspace::NewWorkspace, ports::workspace_repository::WorkspaceRepository,
    };

    #[test]
    fn migrates_and_configures_a_new_database() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut repository = SqliteWorkspaceRepository::open(&path).unwrap();

        let status = repository.get_status().unwrap();

        assert!(!status.initialized);
        let expected_version = crate::adapters::sqlite::database::current_schema_version(
            &rusqlite::Connection::open(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(status.database.schema_version, expected_version);
        assert_eq!(status.database.journal_mode, "wal");
        assert!(status.database.foreign_keys_enabled);
        assert!(!status.database.sqlite_version.is_empty());
        assert!(status.storage.directories_ready);
        assert!(status.storage.writable);
        assert!(status.storage.manifest_ready);
        assert_eq!(status.storage.asset_count, 0);
        assert_eq!(status.storage.total_bytes, 0);
        assert_eq!(status.storage.missing_asset_count, 0);
        assert!(directory
            .path()
            .join("managed-files")
            .join("assets")
            .is_dir());
        assert!(directory
            .path()
            .join("managed-files")
            .join("staging")
            .is_dir());
    }

    #[test]
    fn reports_asset_manifest_counts_in_storage_health() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut repository = SqliteWorkspaceRepository::open(&path).unwrap();

        repository
            .connection
            .execute(
                r#"
                INSERT INTO asset_manifest (
                  id,
                  storage_namespace,
                  asset_kind,
                  display_name,
                  relative_path,
                  size_bytes,
                  integrity_status,
                  created_at,
                  updated_at
                ) VALUES (?1, 'workspace', 'image', 'Preview', 'assets/preview.png', 4096, 'missing', ?2, ?2)
                "#,
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    "2026-07-16T00:00:00Z"
                ],
            )
            .unwrap();

        let status = repository.get_status().unwrap();

        assert_eq!(status.storage.asset_count, 1);
        assert_eq!(status.storage.total_bytes, 4096);
        assert_eq!(status.storage.missing_asset_count, 1);
    }

    #[test]
    fn rejects_absolute_or_parent_relative_manifest_paths() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();

        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        let error = connection
            .execute(
                r#"
                INSERT INTO asset_manifest (
                  id,
                  storage_namespace,
                  asset_kind,
                  display_name,
                  relative_path,
                  size_bytes,
                  integrity_status,
                  created_at,
                  updated_at
                ) VALUES (?1, 'workspace', 'image', 'Invalid', '../outside.png', 1, 'unverified', ?2, ?2)
                "#,
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    "2026-07-16T00:00:00Z"
                ],
            )
            .unwrap_err();
        drop(repository);

        assert!(matches!(error, rusqlite::Error::SqliteFailure(_, _)));
    }

    #[test]
    fn persists_workspace_and_teacher_as_one_profile() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut repository = SqliteWorkspaceRepository::open(&path).unwrap();
        let request = NewWorkspace::try_new("春季课程", "王老师").unwrap();

        let created = repository.initialize(&request).unwrap();
        drop(repository);

        let mut reopened = SqliteWorkspaceRepository::open(&path).unwrap();
        let status = reopened.get_status().unwrap();
        let persisted = status.workspace.unwrap();

        assert!(status.initialized);
        assert_eq!(persisted, created);
        assert_eq!(persisted.workspace_name, "春季课程");
        assert_eq!(persisted.teacher_name, "王老师");
    }

    #[test]
    fn rejects_a_divergent_migration_checksum() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();
        drop(repository);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE schema_migration SET checksum = '0' WHERE version = 1",
                [],
            )
            .unwrap();
        drop(connection);

        assert!(SqliteWorkspaceRepository::open(&path).is_err());
    }

    #[test]
    fn creates_teaching_tables_with_foreign_keys() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();
        drop(repository);

        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();

        for table in ["student", "classroom", "classroom_student"] {
            let exists: i64 = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing table {table}");
        }

        let error = connection
            .execute(
                r#"
                INSERT INTO classroom (
                  id, owner_teacher_id, name, origin_device_id, created_at, updated_at
                ) VALUES (?1, ?2, '无效班级', ?3, ?4, ?4)
                "#,
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    "2026-07-16T00:00:00Z"
                ],
            )
            .unwrap_err();

        assert!(matches!(error, rusqlite::Error::SqliteFailure(_, _)));
    }

    #[test]
    fn rejects_existing_foreign_key_violations() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();
        drop(repository);

        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO classroom (
                  id, owner_teacher_id, name, origin_device_id, created_at, updated_at
                ) VALUES (?1, ?2, '损坏班级', ?3, ?4, ?4)
                "#,
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    "2026-07-16T00:00:00Z"
                ],
            )
            .unwrap();
        drop(connection);

        assert!(SqliteWorkspaceRepository::open(&path).is_err());
    }
}
