use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::backup::Progress;
use rusqlite::{Connection, MAIN_DB};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    adapters::sqlite::{
        database::{current_schema_version, open_database, verify_database},
        managed_storage::managed_storage_root,
    },
    domain::backup::{BackupDraft, BackupManifest, BackupSummary, RestorePreview, RestoreSummary},
    ports::{
        backup_repository::{BackupRepository, BackupRepositoryError},
        persistence::PersistenceError,
    },
};

const MANIFEST_ENTRY: &str = "manifest.json";
const DATABASE_ENTRY: &str = "workspace.sqlite3";
const MANAGED_FILES_PREFIX: &str = "managed-files/";

/// 备份与恢复的 SQLite adapter 实现。
///
/// - 持有一个独立的 `Connection`，与业务 service 的连接分离，避免在恢复时阻塞业务操作。
/// - 受管目录由 `workspace_directory` 推导（`<workspace>/managed-files`）。
pub struct SqliteBackupRepository {
    connection: Connection,
    workspace_directory: PathBuf,
    database_path: PathBuf,
}

impl SqliteBackupRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let database_path = database_path.as_ref().to_path_buf();
        let workspace_directory = database_path
            .parent()
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve backup repository directory",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?
            .to_path_buf();
        let connection = open_database(&database_path)?;
        Ok(Self {
            connection,
            workspace_directory,
            database_path,
        })
    }

    fn now_rfc3339(&self) -> Result<String, PersistenceError> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format backup timestamp", error))
    }

    fn managed_root(&self) -> PathBuf {
        managed_storage_root(&self.workspace_directory)
    }

    /// 把 SQLite 快照备份到临时文件，然后把临时文件写入 zip writer。
    /// 返回快照字节数。
    fn write_database_snapshot(
        &mut self,
        writer: &mut ZipWriter<fs::File>,
    ) -> Result<u64, BackupRepositoryError> {
        let backup_dir = self
            .database_path
            .parent()
            .map(|p| p.join("backups"))
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve snapshot directory",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?;
        fs::create_dir_all(&backup_dir)
            .map_err(|error| PersistenceError::new("create snapshot directory", error))?;

        let snapshot_path = backup_dir.join(format!(".snapshot-{}.sqlite3", Uuid::new_v4()));
        // rusqlite::backup 需要 AsRef<Path> 目标，直接把 SQLite 数据库备份到临时文件。
        self.connection
            .backup(MAIN_DB, &snapshot_path, None::<fn(Progress)>)
            .map_err(|error| {
                PersistenceError::new("backup workspace database to snapshot", error)
            })?;

        let snapshot_bytes = fs::metadata(&snapshot_path).map(|m| m.len()).unwrap_or(0);

        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file(DATABASE_ENTRY, options)
            .map_err(|error| PersistenceError::new("begin backup database entry", error))?;

        let mut source = fs::File::open(&snapshot_path)
            .map_err(|error| PersistenceError::new("open snapshot for archive copy", error))?;
        let mut buffer = [0u8; 65_536];
        loop {
            let read = source
                .read(&mut buffer)
                .map_err(|error| PersistenceError::new("read snapshot bytes", error))?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .map_err(|error| PersistenceError::new("write snapshot bytes to archive", error))?;
        }

        let _ = fs::remove_file(&snapshot_path);
        Ok(snapshot_bytes)
    }

    /// 把 manifest.json 写入 zip writer。
    fn write_manifest_entry(
        &self,
        writer: &mut ZipWriter<fs::File>,
        manifest: &BackupManifest,
    ) -> Result<(), BackupRepositoryError> {
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
            PersistenceError::new(
                "serialize backup manifest",
                std::io::Error::other(error.to_string()),
            )
        })?;
        writer
            .start_file(MANIFEST_ENTRY, options)
            .map_err(|error| PersistenceError::new("begin backup manifest entry", error))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(|error| PersistenceError::new("write backup manifest", error))?;
        Ok(())
    }

    /// 递归遍历 managed_root，把所有文件加入归档。
    /// 返回 (文件数, 总字节数)。
    fn write_managed_files(
        &self,
        writer: &mut ZipWriter<fs::File>,
    ) -> Result<(i64, i64), BackupRepositoryError> {
        let managed_root = self.managed_root();
        if !managed_root.is_dir() {
            return Ok((0, 0));
        }

        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        let mut file_count: i64 = 0;
        let mut total_bytes: i64 = 0;
        let mut stack: Vec<PathBuf> = vec![managed_root.clone()];

        while let Some(directory) = stack.pop() {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    return Err(BackupRepositoryError::Persistence(PersistenceError::new(
                        "walk managed files directory",
                        error,
                    )));
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let entry_type = match entry.file_type() {
                    Ok(t) => t,
                    Err(error) => {
                        return Err(BackupRepositoryError::Persistence(PersistenceError::new(
                            "inspect managed file type",
                            error,
                        )));
                    }
                };

                if entry_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !entry_type.is_file() {
                    continue;
                }

                let relative = match path.strip_prefix(&managed_root) {
                    Ok(rel) => rel,
                    Err(_) => continue,
                };
                // zip 内统一使用正斜杠，便于跨平台恢复。
                let entry_name = format!(
                    "{MANAGED_FILES_PREFIX}{}",
                    relative.to_string_lossy().replace('\\', "/")
                );

                writer
                    .start_file(&entry_name, options)
                    .map_err(|error| PersistenceError::new("begin managed file entry", error))?;

                let mut source = fs::File::open(&path).map_err(|error| {
                    PersistenceError::new("open managed file for backup", error)
                })?;
                let mut buffer = [0u8; 65_536];
                loop {
                    let read = source.read(&mut buffer).map_err(|error| {
                        PersistenceError::new("read managed file for backup", error)
                    })?;
                    if read == 0 {
                        break;
                    }
                    writer.write_all(&buffer[..read]).map_err(|error| {
                        PersistenceError::new("write managed file into archive", error)
                    })?;
                    total_bytes += read as i64;
                }
                file_count += 1;
            }
        }

        Ok((file_count, total_bytes))
    }

    /// 从归档中读取 manifest，并校验格式版本与 schema 兼容性。
    fn read_archive_manifest(
        &self,
        archive_path: &Path,
    ) -> Result<(ZipArchive<fs::File>, BackupManifest), BackupRepositoryError> {
        let file = fs::File::open(archive_path)
            .map_err(|error| PersistenceError::new("open backup archive", error))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| BackupRepositoryError::InvalidArchive(error.to_string()))?;

        let mut manifest_bytes = Vec::new();
        {
            let mut entry = archive
                .by_name(MANIFEST_ENTRY)
                .map_err(|error| BackupRepositoryError::InvalidArchive(error.to_string()))?;
            entry
                .read_to_end(&mut manifest_bytes)
                .map_err(|error| PersistenceError::new("read backup manifest", error))?;
        }
        let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| BackupRepositoryError::InvalidArchive(error.to_string()))?;

        if manifest.format_version != BackupManifest::CURRENT_FORMAT_VERSION {
            return Err(BackupRepositoryError::InvalidArchive(format!(
                "unsupported backup format version: {}",
                manifest.format_version
            )));
        }

        let current = current_schema_version(&self.connection)?;
        if manifest.schema_version > current {
            return Err(BackupRepositoryError::IncompatibleSchema {
                backup_version: manifest.schema_version,
                current_version: current,
            });
        }

        Ok((archive, manifest))
    }

    /// 把 SQLite 快照从归档恢复到指定路径，并校验完整性。
    fn extract_database_snapshot(
        &self,
        archive: &mut ZipArchive<fs::File>,
        snapshot_path: &Path,
    ) -> Result<(), BackupRepositoryError> {
        if let Some(parent) = snapshot_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                PersistenceError::new("create snapshot extraction directory", error)
            })?;
        }

        let temporary_path = snapshot_path.with_extension("sqlite3.tmp");
        {
            let mut entry = archive
                .by_name(DATABASE_ENTRY)
                .map_err(|error| BackupRepositoryError::InvalidArchive(error.to_string()))?;
            let mut writer = fs::File::create(&temporary_path)
                .map_err(|error| PersistenceError::new("create snapshot target", error))?;
            let mut buffer = [0u8; 65_536];
            loop {
                let read = entry
                    .read(&mut buffer)
                    .map_err(|error| PersistenceError::new("read backup database entry", error))?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| PersistenceError::new("write snapshot to disk", error))?;
            }
            writer
                .sync_all()
                .map_err(|error| PersistenceError::new("sync snapshot to disk", error))?;
        }

        // 校验快照可读、schema 版本与 manifest 一致。
        let verification_result = (|| -> Result<i32, PersistenceError> {
            let connection = Connection::open(&temporary_path)
                .map_err(|error| PersistenceError::new("open snapshot for verification", error))?;
            verify_database(&connection)?;
            current_schema_version(&connection)
        })();
        let actual_version = match verification_result {
            Ok(version) => version,
            Err(error) => {
                let _ = fs::remove_file(&temporary_path);
                return Err(BackupRepositoryError::InvalidArchive(format!(
                    "snapshot verification failed: {error}"
                )));
            }
        };

        fs::rename(&temporary_path, snapshot_path)
            .map_err(|error| PersistenceError::new("publish extracted snapshot", error))?;
        let _ = actual_version; // schema version 检查在 restore_backup 中做
        Ok(())
    }

    /// 从归档恢复受管文件到指定目录，返回 (文件数, 总字节数)。
    /// 目标目录会被先清空再写入，确保与备份时刻状态一致。
    fn extract_managed_files(
        &self,
        archive: &mut ZipArchive<fs::File>,
        target_root: &Path,
    ) -> Result<(i64, i64), BackupRepositoryError> {
        if target_root.exists() {
            fs::remove_dir_all(target_root)
                .map_err(|error| PersistenceError::new("clear managed files for restore", error))?;
        }
        fs::create_dir_all(target_root)
            .map_err(|error| PersistenceError::new("recreate managed files directory", error))?;

        let mut file_count: i64 = 0;
        let mut total_bytes: i64 = 0;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| PersistenceError::new("open archive entry", error))?;
            let entry_name = entry.name().to_owned();
            if !entry_name.starts_with(MANAGED_FILES_PREFIX) {
                continue;
            }
            // 防御路径穿越：归档内的路径不能逃逸 target_root。
            let relative = &entry_name[MANAGED_FILES_PREFIX.len()..];
            if relative.is_empty() || relative.contains("..") {
                continue;
            }
            let target_path = target_root.join(relative);
            // 确保父目录存在。
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    PersistenceError::new("create managed file parent directory", error)
                })?;
            }
            if entry.is_dir() {
                fs::create_dir_all(&target_path)
                    .map_err(|error| PersistenceError::new("extract managed directory", error))?;
                continue;
            }
            let mut writer = fs::File::create(&target_path)
                .map_err(|error| PersistenceError::new("extract managed file", error))?;
            let mut buffer = [0u8; 65_536];
            loop {
                let read = entry.read(&mut buffer).map_err(|error| {
                    PersistenceError::new("read archive managed file entry", error)
                })?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| PersistenceError::new("write managed file to disk", error))?;
                total_bytes += read as i64;
            }
            writer
                .sync_all()
                .map_err(|error| PersistenceError::new("sync managed file to disk", error))?;
            file_count += 1;
        }
        Ok((file_count, total_bytes))
    }

    /// 恢复前自动创建一份安全备份，写入 workspace/backups/pre-restore-<timestamp>.zip。
    /// 失败时返回 None，由调用方决定是否继续。
    fn create_safety_backup(&mut self) -> Result<PathBuf, BackupRepositoryError> {
        let backup_directory = self
            .database_path
            .parent()
            .map(|p| p.join("backups"))
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve safety backup directory",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?;
        fs::create_dir_all(&backup_directory)
            .map_err(|error| PersistenceError::new("create safety backup directory", error))?;
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format safety backup timestamp", error))?;
        let safe_timestamp = timestamp.replace(':', "-");
        let safety_path = backup_directory.join(format!("pre-restore-{safe_timestamp}.zip"));

        // 复用 create_backup 流程，但 draft 使用当前工作区的元信息。
        let workspace_name = self
            .connection
            .query_row(
                "SELECT display_name FROM workspace WHERE singleton_key = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "未命名工作空间".to_owned());
        let teacher_name = self
            .connection
            .query_row(
                "SELECT teacher.display_name FROM workspace JOIN teacher ON teacher.id = workspace.owner_teacher_id WHERE workspace.singleton_key = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "未知教师".to_owned());
        let schema_version = current_schema_version(&self.connection)?;
        let (asset_count, total_bytes) = self
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM asset_manifest WHERE deleted_at IS NULL",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap_or((0, 0));

        let draft = BackupDraft::try_new(
            workspace_name,
            teacher_name,
            schema_version,
            asset_count,
            total_bytes,
            Some("恢复前自动安全备份".to_owned()),
        )
        .map_err(|error| {
            PersistenceError::new(
                "build safety backup draft",
                std::io::Error::other(error.to_string()),
            )
        })?;

        // 直接走内部完整流程，不经过 atomic_write_archive 的闭包。
        self.run_create_backup(&safety_path, draft)?;
        Ok(safety_path)
    }

    /// 完整的创建备份流程：临时文件写入 -> rename。
    fn run_create_backup(
        &mut self,
        archive_path: &Path,
        draft: BackupDraft,
    ) -> Result<BackupSummary, BackupRepositoryError> {
        let parent = archive_path.parent().ok_or_else(|| {
            PersistenceError::new(
                "resolve backup target directory",
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "backup target path has no parent directory",
                ),
            )
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| PersistenceError::new("create backup target directory", error))?;

        let temporary_path = parent.join(format!(".backup-{}.tmp", Uuid::new_v4()));
        let created_at = self.now_rfc3339()?;
        let manifest = draft.clone().into_manifest(created_at.clone());

        let file = fs::File::create(&temporary_path)
            .map_err(|error| PersistenceError::new("create temporary backup archive", error))?;
        let mut writer = ZipWriter::new(file);

        self.write_manifest_entry(&mut writer, &manifest)?;
        self.write_database_snapshot(&mut writer)?;
        let (managed_file_count, managed_total_bytes) = self.write_managed_files(&mut writer)?;

        writer
            .finish()
            .map_err(|error| PersistenceError::new("finalize backup archive", error))?;

        // 确保父目录刷盘。
        if let Some(parent_dir) = temporary_path.parent() {
            let _ = fs::File::open(parent_dir).and_then(|f| f.sync_all());
        }

        if archive_path.exists() {
            let _ = fs::remove_file(archive_path);
        }
        fs::rename(&temporary_path, archive_path)
            .map_err(|error| PersistenceError::new("publish backup archive", error))?;

        let archive_size = fs::metadata(archive_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        Ok(BackupSummary {
            archive_path: archive_path.to_string_lossy().into_owned(),
            archive_size,
            managed_file_count,
            managed_total_bytes,
            created_at,
        })
    }
}

impl BackupRepository for SqliteBackupRepository {
    fn create_backup(
        &mut self,
        archive_path: &Path,
        draft: BackupDraft,
    ) -> Result<BackupSummary, BackupRepositoryError> {
        self.run_create_backup(archive_path, draft)
    }

    fn preview_restore(
        &mut self,
        archive_path: &Path,
    ) -> Result<RestorePreview, BackupRepositoryError> {
        let (mut archive, manifest) = self.read_archive_manifest(archive_path)?;

        // 统计受管文件数与字节数，但不解压。
        let mut managed_file_count: i64 = 0;
        let mut managed_total_bytes: i64 = 0;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| PersistenceError::new("read archive entry for preview", error))?;
            let name = entry.name().to_owned();
            if !name.starts_with(MANAGED_FILES_PREFIX) || entry.is_dir() {
                continue;
            }
            managed_file_count += 1;
            managed_total_bytes += entry.size() as i64;
        }

        Ok(RestorePreview {
            archive_path: archive_path.to_string_lossy().into_owned(),
            manifest,
            managed_file_count,
            managed_total_bytes,
        })
    }

    fn restore_backup(
        &mut self,
        archive_path: &Path,
    ) -> Result<RestoreSummary, BackupRepositoryError> {
        // 先尝试创建安全备份。失败时仍允许继续（用户已显式选择恢复）。
        let safety_backup_path = match self.create_safety_backup() {
            Ok(path) => Some(path),
            Err(error) => {
                eprintln!("failed to create safety backup before restore: {error}");
                None
            }
        };

        // 重新读取归档（前面 create_safety_backup 可能改变了 connection 状态）。
        let (mut archive, manifest) = self.read_archive_manifest(archive_path)?;

        // 把 SQLite 快照提取到临时文件，然后用 connection.restore 加载到当前连接。
        let snapshot_path = self
            .database_path
            .parent()
            .map(|p| p.join("restore-snapshot.sqlite3"))
            .ok_or_else(|| {
                PersistenceError::new(
                    "resolve restore snapshot path",
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "database path has no parent directory",
                    ),
                )
            })?;
        if snapshot_path.exists() {
            let _ = fs::remove_file(&snapshot_path);
        }
        self.extract_database_snapshot(&mut archive, &snapshot_path)?;

        // 使用 rusqlite 的 restore API 把快照加载到当前连接，避免在 Windows 上替换正在使用的 .sqlite3 文件。
        self.connection
            .restore(MAIN_DB, &snapshot_path, None::<fn(Progress)>)
            .map_err(|error| PersistenceError::new("restore database from snapshot", error))?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| PersistenceError::new("checkpoint restored database", error))?;

        // 校验恢复后的数据库。
        verify_database(&self.connection)?;
        let actual_version = current_schema_version(&self.connection)?;
        if actual_version != manifest.schema_version {
            let _ = fs::remove_file(&snapshot_path);
            return Err(BackupRepositoryError::InvalidArchive(format!(
                "schema version mismatch after restore: expected {}, found {}",
                manifest.schema_version, actual_version
            )));
        }

        let _ = fs::remove_file(&snapshot_path);

        // 恢复受管文件。
        let managed_root = self.managed_root();
        let (restored_file_count, restored_total_bytes) =
            self.extract_managed_files(&mut archive, &managed_root)?;

        let restored_at = self.now_rfc3339()?;
        Ok(RestoreSummary {
            archive_path: archive_path.to_string_lossy().into_owned(),
            safety_backup_path: safety_backup_path.map(|p| p.to_string_lossy().into_owned()),
            restored_file_count,
            restored_total_bytes,
            restored_at,
            requires_restart: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::backup_repository::BackupRepository;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed_workspace() -> (tempfile::TempDir, PathBuf) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("春季课程", "王老师").unwrap())
            .unwrap();
        drop(workspace);
        (directory, path)
    }

    fn write_sample_asset(directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let assets_dir = directory.join("managed-files").join("assets");
        fs::create_dir_all(&assets_dir).unwrap();
        let file_path = assets_dir.join(name);
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(bytes).unwrap();
        file_path
    }

    #[test]
    fn creates_and_reads_back_a_backup_archive() {
        let (directory, database_path) = seed_workspace();
        write_sample_asset(directory.path(), "preview.png", b"sample-bytes");

        let mut repository = SqliteBackupRepository::open(&database_path).unwrap();
        let archive_path = directory.path().join("backup.zip");

        let draft = BackupDraft::try_new(
            "春季课程".to_owned(),
            "王老师".to_owned(),
            4,
            0,
            0,
            Some("单元测试备份".to_owned()),
        )
        .unwrap();
        let summary = repository.create_backup(&archive_path, draft).unwrap();
        assert!(archive_path.exists());
        assert_eq!(summary.archive_path, archive_path.to_string_lossy());
        assert!(summary.managed_file_count >= 1);
        assert!(summary.managed_total_bytes >= b"sample-bytes".len() as i64);

        let preview = repository.preview_restore(&archive_path).unwrap();
        assert_eq!(preview.manifest.workspace_name, "春季课程");
        assert_eq!(preview.manifest.teacher_name, "王老师");
        assert!(preview.managed_file_count >= 1);
    }

    #[test]
    fn rejects_preview_for_nonexistent_archive() {
        let (_directory, database_path) = seed_workspace();
        let mut repository = SqliteBackupRepository::open(&database_path).unwrap();
        let missing = std::env::temp_dir().join("does-not-exist.zip");
        let error = repository.preview_restore(&missing).unwrap_err();
        assert!(matches!(error, BackupRepositoryError::Persistence(_)));
    }

    #[test]
    fn rejects_archive_with_unsupported_format_version() {
        let (directory, database_path) = seed_workspace();
        let mut repository = SqliteBackupRepository::open(&database_path).unwrap();
        let archive_path = directory.path().join("bad.zip");

        // 写一个格式版本错误的 manifest。
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer.start_file(MANIFEST_ENTRY, options).unwrap();
        let manifest = serde_json::json!({
            "formatVersion": 999,
            "workspaceName": "x",
            "teacherName": "y",
            "schemaVersion": 1,
            "assetCount": 0,
            "totalBytes": 0,
            "createdAt": "2026-07-16T00:00:00Z",
            "note": null,
        });
        writer.write_all(manifest.to_string().as_bytes()).unwrap();
        // finish 会消费 writer 并返回内部文件句柄。
        let _ = writer.finish().unwrap();

        let error = repository.preview_restore(&archive_path).unwrap_err();
        assert!(matches!(error, BackupRepositoryError::InvalidArchive(_)));
    }

    #[test]
    fn restores_database_and_managed_files() {
        let (directory, database_path) = seed_workspace();
        let assets_dir = directory.path().join("managed-files").join("assets");
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(assets_dir.join("a.png"), b"alpha").unwrap();
        fs::write(assets_dir.join("b.png"), b"beta").unwrap();

        let mut repository = SqliteBackupRepository::open(&database_path).unwrap();
        let archive_path = directory.path().join("restore-source.zip");
        let schema_version =
            current_schema_version(&open_database(&database_path).unwrap()).unwrap();
        let draft = BackupDraft::try_new(
            "春季课程".to_owned(),
            "王老师".to_owned(),
            schema_version,
            0,
            0,
            None,
        )
        .unwrap();
        repository.create_backup(&archive_path, draft).unwrap();

        // 修改本地状态：删除受管文件并插入一条新的资产。
        fs::remove_file(assets_dir.join("a.png")).unwrap();
        fs::write(assets_dir.join("c.png"), b"gamma").unwrap();

        // 恢复。
        let summary = repository.restore_backup(&archive_path).unwrap();
        assert!(summary.restored_file_count >= 2);
        assert!(summary.safety_backup_path.is_some());
        assert!(summary.requires_restart);

        // 恢复后受管文件应该回到备份时刻状态。
        assert!(assets_dir.join("a.png").exists());
        assert!(assets_dir.join("b.png").exists());
        assert!(!assets_dir.join("c.png").exists());
    }
}
