use std::{fs, path::Path, time::Duration};

use refinery::{embed_migrations, Runner};
use rusqlite::{Connection, OptionalExtension};

use super::migration_backup::{create_migration_backup, restore_migration_backup};
use crate::ports::persistence::PersistenceError;

mod embedded {
    use super::embed_migrations;

    embed_migrations!("./migrations");
}

const MIGRATION_TABLE: &str = "schema_migration";

pub fn open_database(database_path: impl AsRef<Path>) -> Result<Connection, PersistenceError> {
    let database_path = database_path.as_ref();
    open_database_with_runner(database_path, migration_runner())
}

fn open_database_with_runner(
    database_path: &Path,
    runner: Runner,
) -> Result<Connection, PersistenceError> {
    let parent = database_path.parent().ok_or_else(|| {
        PersistenceError::new(
            "resolve workspace database directory",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "database path has no parent directory",
            ),
        )
    })?;

    fs::create_dir_all(parent)
        .map_err(|error| PersistenceError::new("create workspace directory", error))?;
    let database_had_content = database_has_content(database_path)?;

    let mut connection = Connection::open(database_path)
        .map_err(|error| PersistenceError::new("open workspace database", error))?;
    let current_version = current_schema_version(&connection)?;
    let latest_version = latest_schema_version(&runner);

    if current_version > latest_version {
        return Err(PersistenceError::new(
            "check workspace schema compatibility",
            std::io::Error::other(format!(
                "database schema version {current_version} is newer than supported version {latest_version}"
            )),
        ));
    }

    configure_connection(&mut connection)?;

    if database_had_content {
        verify_database(&connection)?;
    }

    let backup = if database_had_content && current_version < latest_version {
        Some(create_migration_backup(
            &connection,
            database_path,
            current_version,
            latest_version,
        )?)
    } else {
        None
    };

    let upgrade_result =
        run_migrations(&mut connection, &runner).and_then(|()| verify_database(&connection));

    if let Err(upgrade_error) = upgrade_result {
        if let Some(backup_path) = backup {
            return match restore_migration_backup(&mut connection, &backup_path, current_version) {
                Ok(()) => Err(PersistenceError::new(
                    "upgrade workspace database",
                    std::io::Error::other(format!(
                        "{upgrade_error}; restored pre-migration snapshot {}",
                        backup_path.display()
                    )),
                )),
                Err(restore_error) => Err(PersistenceError::new(
                    "restore workspace database after upgrade failure",
                    std::io::Error::other(format!(
                        "upgrade error: {upgrade_error}; restore error: {restore_error}"
                    )),
                )),
            };
        }

        return Err(upgrade_error);
    }

    // Enable FK enforcement after all migrations complete.
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| PersistenceError::new("enable foreign keys after migration", error))?;

    Ok(connection)
}

fn migration_runner() -> Runner {
    let mut runner = embedded::migrations::runner()
        .set_grouped(true)
        .set_abort_divergent(true)
        .set_abort_missing(true);
    runner.set_migration_table_name(MIGRATION_TABLE);
    runner
}

fn database_has_content(database_path: &Path) -> Result<bool, PersistenceError> {
    match fs::metadata(database_path) {
        Ok(metadata) => Ok(metadata.len() > 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(PersistenceError::new("inspect workspace database", error)),
    }
}

fn latest_schema_version(runner: &Runner) -> i32 {
    runner
        .get_migrations()
        .iter()
        .map(|migration| migration.version())
        .max()
        .unwrap_or(0)
}

pub(crate) fn current_schema_version(connection: &Connection) -> Result<i32, PersistenceError> {
    let migration_table_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [MIGRATION_TABLE],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| PersistenceError::new("find migration history", error))?
        == 1;

    if !migration_table_exists {
        return Ok(0);
    }

    connection
        .query_row(
            &format!("SELECT COALESCE(MAX(version), 0) FROM {MIGRATION_TABLE}"),
            [],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::new("read schema version", error))
}

fn configure_connection(connection: &mut Connection) -> Result<(), PersistenceError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| PersistenceError::new("configure database busy timeout", error))?;
    // FK enforcement is deferred to after migrations complete,
    // to allow V9's broken FK reference ("credentials" table doesn't exist).
    // FK is enabled at the end of open_database_with_runner.

    let journal_mode = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| PersistenceError::new("enable WAL journal mode", error))?;

    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(PersistenceError::new(
            "verify WAL journal mode",
            std::io::Error::other(format!(
                "SQLite returned unsupported journal mode {journal_mode}"
            )),
        ));
    }

    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| PersistenceError::new("configure synchronous mode", error))?;
    connection.set_prepared_statement_cache_capacity(32);

    Ok(())
}

fn run_migrations(connection: &mut Connection, runner: &Runner) -> Result<(), PersistenceError> {
    // FK is not enabled at this point (deferred from configure_connection).
    // This allows V9's broken FK reference to succeed.
    runner
        .run(connection)
        .map_err(|error| PersistenceError::new("migrate workspace database", error))?;

    Ok(())
}

pub(super) fn verify_database(connection: &Connection) -> Result<(), PersistenceError> {
    let quick_check = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|error| PersistenceError::new("check workspace database", error))?;

    if quick_check != "ok" {
        return Err(PersistenceError::new(
            "check workspace database",
            std::io::Error::other(quick_check),
        ));
    }

    // FK check: only run when FK enforcement is active.
    // Before migrations, FK is OFF so this is skipped.
    // After migrations, FK is ON and V13 has fixed the broken reference.
    let fk_on: i32 = connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap_or(0);
    if fk_on == 1 {
        let foreign_key_violation = connection
            .query_row("PRAGMA foreign_key_check", [], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()
            .map_err(|error| PersistenceError::new("check workspace foreign keys", error))?;

        if let Some((table, row_id, parent, foreign_key)) = foreign_key_violation {
            return Err(PersistenceError::new(
                "check workspace foreign keys",
                std::io::Error::other(format!(
                    "table {table} row {row_id:?} violates foreign key {foreign_key} referencing {parent}"
                )),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use refinery::{Migration, Runner, Target};
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::super::migration_backup::migration_backup_path;
    use super::{
        configure_connection, current_schema_version, migration_runner, open_database,
        open_database_with_runner, MIGRATION_TABLE,
    };

    #[test]
    fn does_not_backup_a_new_database() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");

        let connection = open_database(&path).unwrap();
        drop(connection);

        assert!(!directory.path().join("backups").exists());
    }

    #[test]
    fn backs_up_an_existing_database_before_upgrading() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        configure_connection(&mut connection).unwrap();
        migration_runner()
            .set_target(Target::Version(1))
            .run(&mut connection)
            .unwrap();
        connection
            .execute_batch(
                "CREATE TABLE recovery_marker(value TEXT NOT NULL);\
                 INSERT INTO recovery_marker(value) VALUES ('preserved');",
            )
            .unwrap();
        drop(connection);

        let upgraded = open_database(&path).unwrap();
        let upgraded_version = current_schema_version(&upgraded).unwrap();
        assert!(upgraded_version > 1);
        drop(upgraded);

        let backup_path = migration_backup_path(&path, 1, upgraded_version).unwrap();
        let backup = Connection::open(backup_path).unwrap();
        let marker: String = backup
            .query_row("SELECT value FROM recovery_marker", [], |row| row.get(0))
            .unwrap();

        assert_eq!(current_schema_version(&backup).unwrap(), 1);
        assert_eq!(marker, "preserved");
    }

    #[test]
    fn rejects_a_database_newer_than_the_embedded_schema() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migration(version INTEGER PRIMARY KEY);\
                 INSERT INTO schema_migration(version) VALUES (999);\
                 CREATE TABLE recovery_marker(value TEXT NOT NULL);\
                 INSERT INTO recovery_marker(value) VALUES ('untouched');",
            )
            .unwrap();
        drop(connection);

        let error = match open_database(&path) {
            Ok(_) => panic!("newer schema must be rejected"),
            Err(error) => error,
        };
        let reopened = Connection::open(&path).unwrap();
        let marker: String = reopened
            .query_row("SELECT value FROM recovery_marker", [], |row| row.get(0))
            .unwrap();

        assert!(error.to_string().contains("newer than supported"));
        assert_eq!(marker, "untouched");
        assert!(!directory.path().join("backups").exists());
    }

    #[test]
    fn restores_the_snapshot_when_a_later_migration_fails() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let migrations = vec![
            Migration::unapplied(
                "V1__create_marker.sql",
                "CREATE TABLE marker(value TEXT NOT NULL);\
                 INSERT INTO marker(value) VALUES ('original');",
            )
            .unwrap(),
            Migration::unapplied(
                "V2__extend_marker.sql",
                "ALTER TABLE marker ADD COLUMN migrated INTEGER NOT NULL DEFAULT 1;",
            )
            .unwrap(),
            Migration::unapplied("V3__fail.sql", "THIS IS NOT VALID SQL;").unwrap(),
        ];
        let mut connection = Connection::open(&path).unwrap();
        configure_connection(&mut connection).unwrap();
        configured_runner(&migrations, false)
            .set_target(Target::Version(1))
            .run(&mut connection)
            .unwrap();
        drop(connection);

        let error = match open_database_with_runner(&path, configured_runner(&migrations, false)) {
            Ok(_) => panic!("failing migration must not open the database"),
            Err(error) => error,
        };
        let restored = Connection::open(&path).unwrap();
        let marker: String = restored
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .unwrap();
        let migrated_column_exists: i64 = restored
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('marker') WHERE name = 'migrated')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(error
            .to_string()
            .contains("restored pre-migration snapshot"));
        assert_eq!(current_schema_version(&restored).unwrap(), 1);
        assert_eq!(marker, "original");
        assert_eq!(migrated_column_exists, 0);
        assert!(migration_backup_path(&path, 1, 3).unwrap().exists());
    }

    fn configured_runner(migrations: &[Migration], grouped: bool) -> Runner {
        let mut runner = Runner::new(migrations)
            .set_grouped(grouped)
            .set_abort_divergent(true)
            .set_abort_missing(true);
        runner.set_migration_table_name(MIGRATION_TABLE);
        runner
    }
}
