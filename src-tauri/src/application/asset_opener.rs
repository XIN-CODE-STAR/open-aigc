use std::path::{Path, PathBuf};

use tauri_plugin_opener::OpenerExt;

use crate::{
    adapters::sqlite::managed_storage::managed_storage_root,
    application::error::AppError,
    domain::assets::AssetRecord,
    ports::{asset_repository::AssetRepositoryError, persistence::PersistenceError},
};

/// 受控的资产打开错误。任意路径输入都被拒绝；只能通过 manifest 中已记录的 asset_id 打开。
#[derive(Debug, thiserror::Error)]
pub enum AssetOpenError {
    /// 资产不存在或已软删。
    #[error("asset not found: {0}")]
    NotFound(String),
    /// 资产的 relative_path 不安全（绝对路径、`..` 跳转等）。
    #[error("asset relative path is unsafe")]
    UnsafePath,
    /// 受管文件在磁盘上不存在（可能被外部删除）。
    #[error("managed file does not exist: {0}")]
    FileMissing(String),
    /// 系统调用打开失败。
    #[error("opener failed: {0}")]
    Opener(String),
}

impl From<AssetOpenError> for AppError {
    fn from(error: AssetOpenError) -> Self {
        // 复用 AssetRepository 的错误通道；opener 的系统失败归入 Persistence 以触发 ipc 层的通用错误。
        match error {
            AssetOpenError::NotFound(id) => AppError::Persistence(PersistenceError::new(
                "open asset",
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("asset {id} not found"),
                ),
            )),
            AssetOpenError::UnsafePath => AppError::Persistence(PersistenceError::new(
                "open asset",
                std::io::Error::new(std::io::ErrorKind::InvalidData, "unsafe asset path"),
            )),
            AssetOpenError::FileMissing(path) => AppError::Persistence(PersistenceError::new(
                "open asset",
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("managed file {path} missing"),
                ),
            )),
            AssetOpenError::Opener(message) => {
                AppError::AssetRepository(AssetRepositoryError::Persistence(PersistenceError::new(
                    "open asset with system opener",
                    std::io::Error::other(message),
                )))
            }
        }
    }
}

/// 把 manifest 的 relative_path（形如 `assets/<name>`）解析为受管目录下的绝对路径。
/// 任何试图逃逸受管目录的输入（绝对路径、`..`、反斜杠、空段）都会被拒绝。
pub fn resolve_managed_asset_path(
    workspace_directory: &Path,
    relative_path: &str,
) -> Result<PathBuf, AssetOpenError> {
    if relative_path.is_empty() || relative_path.starts_with('/') || relative_path.contains('\\') {
        return Err(AssetOpenError::UnsafePath);
    }
    let managed_root = managed_storage_root(workspace_directory);
    let mut full = managed_root.clone();
    for segment in relative_path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(AssetOpenError::UnsafePath);
        }
        full.push(segment);
    }
    // 二次校验：规范化后仍必须在受管根目录下。
    let canonical_root = managed_root
        .canonicalize()
        .map_err(|_| AssetOpenError::FileMissing(managed_root.to_string_lossy().into()))?;
    let canonical_full = full
        .canonicalize()
        .map_err(|_| AssetOpenError::FileMissing(full.to_string_lossy().into()))?;
    if !canonical_full.starts_with(&canonical_root) {
        return Err(AssetOpenError::UnsafePath);
    }
    Ok(canonical_full)
}

/// 通过系统默认程序打开资产对应的受管文件。
pub fn open_asset_file(
    app: &tauri::AppHandle,
    workspace_directory: &Path,
    asset: &AssetRecord,
) -> Result<(), AssetOpenError> {
    let path = resolve_managed_asset_path(workspace_directory, &asset.relative_path)?;
    if !path.exists() {
        return Err(AssetOpenError::FileMissing(path.to_string_lossy().into()));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| AssetOpenError::Opener(error.to_string()))
}

/// 在文件资源管理器中打开资产所在目录并选中该文件。
pub fn open_asset_containing_folder(
    app: &tauri::AppHandle,
    workspace_directory: &Path,
    asset: &AssetRecord,
) -> Result<(), AssetOpenError> {
    let path = resolve_managed_asset_path(workspace_directory, &asset.relative_path)?;
    if !path.exists() {
        return Err(AssetOpenError::FileMissing(path.to_string_lossy().into()));
    }
    if path.parent().is_none() {
        return Err(AssetOpenError::UnsafePath);
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|error| AssetOpenError::Opener(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    use super::resolve_managed_asset_path;

    fn setup_managed_root(root: &Path) -> PathBuf {
        let managed = root.join("managed-files");
        fs::create_dir_all(&managed).unwrap();
        managed
    }

    #[test]
    fn rejects_traversal_paths() {
        let root = tempdir().unwrap();
        setup_managed_root(root.path());
        let err = resolve_managed_asset_path(root.path(), "assets/../secret").unwrap_err();
        assert!(matches!(err, super::AssetOpenError::UnsafePath));
    }

    #[test]
    fn rejects_absolute_paths() {
        let root = tempdir().unwrap();
        setup_managed_root(root.path());
        let err = resolve_managed_asset_path(root.path(), "/etc/passwd").unwrap_err();
        assert!(matches!(err, super::AssetOpenError::UnsafePath));
    }

    #[test]
    fn resolves_valid_asset_path() {
        let root = tempdir().unwrap();
        let _managed = setup_managed_root(root.path());
        // relative_path 形如 "assets/<name>"，对应 managed-files/assets/<name>。
        let file = root
            .path()
            .join("managed-files")
            .join("assets")
            .join("ok.png");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"x").unwrap();
        let resolved = resolve_managed_asset_path(root.path(), "assets/ok.png").unwrap();
        assert_eq!(resolved, file.canonicalize().unwrap());
    }
}
