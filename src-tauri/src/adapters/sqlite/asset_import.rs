use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    adapters::sqlite::{
        asset_repository::{
            begin_import_tx, delete_manifest_by_id, insert_asset_in_tx, now_rfc3339,
        },
        managed_storage::{assets_root, staging_root},
    },
    domain::assets::{
        AssetDraft, AssetImportFailure, AssetImportOutcome, AssetImportSkipped, AssetImportSummary,
        AssetKind, AssetRecord, IntegrityStatus, StorageNamespace,
    },
    ports::{
        asset_repository::{AssetRepositoryError, ImportOptions, StagingError},
        persistence::PersistenceError,
    },
};

const READ_BUFFER_SIZE: usize = 65_536;
const EXTENSION_MAX_LENGTH: usize = 16;

pub(super) fn run_import(
    connection: &mut Connection,
    workspace_directory: &Path,
    source_paths: Vec<String>,
    namespace: &str,
    options: ImportOptions,
    device_id: Option<&str>,
) -> Result<AssetImportSummary, AssetRepositoryError> {
    let storage_namespace = match StorageNamespace::parse(namespace.trim()) {
        Ok(value) => value,
        Err(_) => {
            // 未知 namespace 不接触数据库，整批标记失败。
            let failures = source_paths
                .into_iter()
                .map(|path| AssetImportFailure {
                    source_path: path,
                    reason: "未知的存储命名空间。".to_owned(),
                })
                .collect();
            return Ok(AssetImportSummary {
                imported: Vec::new(),
                skipped: Vec::new(),
                failures,
            });
        }
    };

    let staging_directory = staging_root(workspace_directory);
    let assets_directory = assets_root(workspace_directory);
    fs::create_dir_all(&staging_directory).map_err(|error| {
        AssetRepositoryError::Staging(StagingError::Io {
            operation: "create staging directory",
            source: error,
        })
    })?;
    fs::create_dir_all(&assets_directory).map_err(|error| {
        AssetRepositoryError::Staging(StagingError::Io {
            operation: "create assets directory",
            source: error,
        })
    })?;

    let mut imported: Vec<AssetImportOutcome> = Vec::new();
    let mut skipped: Vec<AssetImportSkipped> = Vec::new();
    let mut failures: Vec<AssetImportFailure> = Vec::new();

    for source_path in source_paths {
        let outcome = process_one(
            connection,
            &staging_directory,
            &assets_directory,
            &source_path,
            storage_namespace,
            options,
            device_id,
        );
        match outcome {
            Ok(ImportResult::Imported(record)) => imported.push(AssetImportOutcome {
                asset: record,
                source_path,
            }),
            Ok(ImportResult::Skipped { reason, existing }) => skipped.push(AssetImportSkipped {
                source_path,
                reason,
                existing_asset_id: existing.map(|record| record.id),
            }),
            Ok(ImportResult::Failed(reason)) => failures.push(AssetImportFailure {
                source_path,
                reason,
            }),
            Err(error) => return Err(error),
        }
    }

    Ok(AssetImportSummary {
        imported,
        skipped,
        failures,
    })
}

enum ImportResult {
    Imported(AssetRecord),
    Skipped {
        reason: String,
        existing: Option<AssetRecord>,
    },
    Failed(String),
}

#[allow(clippy::too_many_arguments)]
fn process_one(
    connection: &mut Connection,
    staging_directory: &Path,
    assets_directory: &Path,
    source_path: &str,
    storage_namespace: StorageNamespace,
    options: ImportOptions,
    device_id: Option<&str>,
) -> Result<ImportResult, AssetRepositoryError> {
    let source = PathBuf::from(source_path);
    let metadata = match fs::metadata(&source) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Ok(ImportResult::Failed(format!("无法读取源文件：{error}")));
        }
    };
    if !metadata.is_file() {
        return Ok(ImportResult::Failed("源路径不是文件。".to_owned()));
    }
    let size_bytes = metadata.len() as i64;
    if size_bytes > options.max_bytes {
        return Ok(ImportResult::Failed(format!(
            "文件大小超过 {} 字节上限。",
            options.max_bytes
        )));
    }

    let display_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "未命名资产".to_owned());
    let extension = sanitize_extension(&source);
    let asset_kind = AssetKind::infer(None, &display_name);

    let staging_id = Uuid::new_v4().to_string();
    let staging_file_name = format!("{staging_id}.{extension}");
    let staging_path = staging_directory.join(&staging_file_name);
    let relative_path = format!("assets/{staging_file_name}");

    // 流式拷贝到 staging 并同步计算 SHA-256。staging 写失败按整批失败处理。
    let sha256 = match stream_to_staging(&source, &staging_path) {
        Ok(hash) => hash,
        Err(error) => {
            // 即便 staging 失败也尝试清理已写入的临时文件。
            let _ = fs::remove_file(&staging_path);
            return Err(AssetRepositoryError::Staging(StagingError::Io {
                operation: "stream source to staging",
                source: error,
            }));
        }
    };

    // 事务内：先看是否已有同 sha256+size 的资产，有则跳过；否则写入 manifest。
    let transaction = begin_import_tx(connection)?;
    let existing = match find_duplicate_in_tx(&transaction, sha256.as_deref(), size_bytes) {
        Ok(Some(record)) => Some(record),
        Ok(None) => None,
        Err(error) => {
            let _ = transaction.rollback();
            let _ = fs::remove_file(&staging_path);
            return Err(AssetRepositoryError::Persistence(error));
        }
    };

    if let Some(record) = existing {
        transaction
            .rollback()
            .map_err(|error| PersistenceError::new("rollback duplicate lookup", error))?;
        let _ = fs::remove_file(&staging_path);
        return Ok(ImportResult::Skipped {
            reason: "工作空间中已存在相同内容的资产。".to_owned(),
            existing: Some(record),
        });
    }

    let created_at = now_rfc3339()?;
    let draft = AssetDraft::try_new(
        storage_namespace.as_str().to_owned(),
        asset_kind,
        display_name,
        relative_path.clone(),
        size_bytes,
        sha256.clone(),
        None,
        None,
    )
    .map_err(|error| {
        PersistenceError::new(
            "build asset draft",
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()),
        )
    })?;

    let inserted = match insert_asset_in_tx(&transaction, &draft, device_id, &created_at) {
        Ok(record) => record,
        Err(error) => {
            let _ = transaction.rollback();
            let _ = fs::remove_file(&staging_path);
            return Err(AssetRepositoryError::Persistence(error));
        }
    };

    if let Err(error) = transaction.commit() {
        let _ = fs::remove_file(&staging_path);
        return Err(AssetRepositoryError::Persistence(PersistenceError::new(
            "commit asset import",
            error,
        )));
    }

    // manifest 已 commit，把 staging 文件原子移动到 assets 目录。
    let final_path = assets_directory.join(&staging_file_name);
    if let Err(error) = fs::rename(&staging_path, &final_path) {
        // 极少发生（同盘 rename 是原子的）。回滚已写入的 manifest 并清理 staging。
        if let Err(cleanup_error) = delete_manifest_by_id(connection, &inserted.id) {
            eprintln!("failed to roll back manifest after rename failure: {cleanup_error}");
        }
        let _ = fs::remove_file(&staging_path);
        return Ok(ImportResult::Failed(format!(
            "无法将文件移动到受管目录：{error}"
        )));
    }

    Ok(ImportResult::Imported(inserted))
}

fn stream_to_staging(source: &Path, staging: &Path) -> Result<Option<String>, std::io::Error> {
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(staging)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; READ_BUFFER_SIZE];
    let mut total: u64 = 0;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
        total += read as u64;
    }
    writer.sync_all()?;
    drop(writer);
    drop(reader);

    // 空文件不写哈希值，避免和占位资产冲突；正常文件都会落到 assets 命名空间。
    let hash = if total == 0 {
        None
    } else {
        Some(format!("{:x}", hasher.finalize()))
    };
    Ok(hash)
}

fn find_duplicate_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    sha256: Option<&str>,
    size_bytes: i64,
) -> Result<Option<AssetRecord>, PersistenceError> {
    let Some(hash) = sha256 else {
        return Ok(None);
    };
    let record = transaction
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
            WHERE deleted_at IS NULL AND sha256 = ?1 AND size_bytes = ?2
            LIMIT 1
            "#,
            rusqlite::params![hash, size_bytes],
            map_asset_record,
        )
        .optional()
        .map_err(|error| PersistenceError::new("find duplicate asset in import", error))?;
    Ok(record)
}

fn map_asset_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRecord> {
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

fn sanitize_extension(path: &Path) -> String {
    let raw = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bin");
    let lower = raw.to_ascii_lowercase();
    let filtered: String = lower
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(EXTENSION_MAX_LENGTH)
        .collect();
    if filtered.is_empty() {
        "bin".to_owned()
    } else {
        filtered
    }
}
