use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::workspace_repository::SqliteWorkspaceRepository,
    application::error::AppError,
    domain::workspace::{
        NewWorkspace, UpdateWorkspaceNameResult, WorkspaceProfile, WorkspaceStatus,
    },
    ports::workspace_repository::WorkspaceRepository,
};

pub struct WorkspaceService {
    repository: Mutex<Box<dyn WorkspaceRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl WorkspaceService {
    pub fn new(repository: impl WorkspaceRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    pub fn get_status(&self) -> Result<WorkspaceStatus, AppError> {
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        repository.get_status().map_err(AppError::from)
    }

    pub fn initialize(
        &self,
        workspace_name: String,
        teacher_name: String,
    ) -> Result<WorkspaceProfile, AppError> {
        let workspace = NewWorkspace::try_new(workspace_name, teacher_name)?;
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        if repository.get_status()?.initialized {
            return Err(AppError::AlreadyInitialized);
        }

        repository.initialize(&workspace).map_err(AppError::from)
    }

    /// 重命名当前工作空间。教师自助修改，无需 Admin 权限。
    /// 名称复用与 `initialize` 相同的校验规则（非空 / 长度 / 控制字符）。
    /// 未初始化时返回 `AppError::AlreadyInitialized`（与 initialize 失败语义一致）。
    pub fn rename_workspace(&self, workspace_name: String) -> Result<WorkspaceProfile, AppError> {
        // 直接复用 NewWorkspace::try_new 的校验逻辑，构造一个临时实例。
        let new_workspace = NewWorkspace::try_new(workspace_name, "placeholder")?;
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;

        match repository
            .update_workspace_name(&new_workspace.workspace_name)
            .map_err(AppError::from)?
        {
            UpdateWorkspaceNameResult::Updated(profile) => Ok(profile),
            UpdateWorkspaceNameResult::NotInitialized => Err(AppError::AlreadyInitialized),
        }
    }
}

impl crate::ports::reloadable::Reloadable for WorkspaceService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_repository = SqliteWorkspaceRepository::open(database_path)?;
        let mut repository = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repository = Box::new(new_repository);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::WorkspaceService;
    use crate::{
        adapters::sqlite::workspace_repository::SqliteWorkspaceRepository,
        application::error::AppError,
    };

    #[test]
    fn rejects_repeated_initialization() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();
        let service = WorkspaceService::new(repository, path);

        service
            .initialize("春季课程".to_owned(), "王老师".to_owned())
            .unwrap();
        let error = service
            .initialize("第二工作空间".to_owned(), "李老师".to_owned())
            .unwrap_err();

        assert!(matches!(error, AppError::AlreadyInitialized));
    }

    #[test]
    fn validation_failure_does_not_initialize_workspace() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let repository = SqliteWorkspaceRepository::open(&path).unwrap();
        let service = WorkspaceService::new(repository, path);

        assert!(service
            .initialize("   ".to_owned(), "王老师".to_owned())
            .is_err());
        assert!(!service.get_status().unwrap().initialized);
    }
}
