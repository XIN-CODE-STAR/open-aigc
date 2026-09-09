use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    adapters::sqlite::{
        asset_repository::SqliteAssetRepository, generation_repository::SqliteGenerationRepository,
        resource_repository::SqliteResourceRepository,
    },
    application::error::AppError,
    domain::{
        generation::{
            GenerationFilter, GenerationResultRecord, GenerationStatus, GenerationTaskDraft,
            GenerationTaskRecord, GenerationValidationError,
        },
        resources::AssociationDraft,
    },
    ports::{
        asset_repository::{AssetRepository, ImportOptions},
        generation_repository::GenerationRepository,
        resource_repository::ResourceRepository,
    },
};

/// 生成历史服务：协调生成任务、受管文件导入和资源关联。
///
/// 关键约束：AI 生成结果必须先通过 AssetRepository::import 进入受管文件流程，
/// 再通过 GenerationRepository::add_result 记录到生成历史。
/// 这确保了所有 AI 输出都经过统一的 staging → 校验 → manifest 流程。
pub struct GenerationService {
    generation_repository: Mutex<Box<dyn GenerationRepository>>,
    asset_repository: Mutex<Box<dyn AssetRepository>>,
    resource_repository: Mutex<Box<dyn ResourceRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl GenerationService {
    pub fn new(
        generation_repository: impl GenerationRepository + 'static,
        asset_repository: impl AssetRepository + 'static,
        resource_repository: impl ResourceRepository + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            generation_repository: Mutex::new(Box::new(generation_repository)),
            asset_repository: Mutex::new(Box::new(asset_repository)),
            resource_repository: Mutex::new(Box::new(resource_repository)),
            database_path,
        }
    }

    /// 提交一个生成任务，初始状态为 pending。
    pub fn submit_task(
        &self,
        workspace_id: String,
        provider_name: String,
        model_name: String,
        prompt_text: String,
    ) -> Result<GenerationTaskRecord, AppError> {
        let draft =
            GenerationTaskDraft::try_new(workspace_id, provider_name, model_name, prompt_text)?;
        self.with_generation_repository(|repo| repo.create_task(draft).map_err(Into::into))
    }

    /// 记录生成输出：把 AI 产出的文件导入受管存储，再写入生成历史和资源关联。
    ///
    /// 流程：标记 running → 导入受管文件（namespace=generation）→ 写 generation_results
    /// → 写 resource_association（context_kind=generation-output, role=result）→ 标记 succeeded。
    /// 任一步骤失败都会标记任务为 failed 并返回错误。
    pub fn record_output(
        &self,
        task_id: &str,
        source_path: String,
    ) -> Result<GenerationResultRecord, AppError> {
        validate_id(task_id, "taskId")?;
        let trimmed_path = source_path.trim();
        if trimmed_path.is_empty() {
            return Err(AppError::GenerationValidation(
                GenerationValidationError::Required {
                    field: "sourcePath",
                },
            ));
        }

        // 1. 标记任务为 running。
        self.with_generation_repository(|repo| {
            repo.update_status(task_id, GenerationStatus::Running, None)
                .map_err(Into::into)
        })?;

        // 2. 通过受管文件流程导入 AI 输出。
        let import_result = self.with_asset_repository(|repo| {
            repo.import(
                vec![trimmed_path.to_owned()],
                "generation",
                ImportOptions::default(),
            )
            .map_err(AppError::from)
        });

        let summary = match import_result {
            Ok(summary) => summary,
            Err(error) => {
                self.mark_failed_internal(task_id, &error.to_string());
                return Err(error);
            }
        };

        if summary.imported.is_empty() {
            let error_message = if summary.failures.is_empty() {
                "没有可导入的生成结果文件。".to_owned()
            } else {
                format!(
                    "生成结果文件导入失败：{}",
                    summary
                        .failures
                        .first()
                        .map(|f| f.reason.as_str())
                        .unwrap_or("未知原因")
                )
            };
            self.mark_failed_internal(task_id, &error_message);
            return Err(AppError::GenerationValidation(
                GenerationValidationError::Required {
                    field: "sourcePath",
                },
            ));
        }

        let asset_id = summary.imported[0].asset.id.clone();

        // 3. 写入 generation_results，关联任务与受管资产。
        let result = self.with_generation_repository(|repo| {
            repo.add_result(task_id, &asset_id).map_err(AppError::from)
        });
        let result = match result {
            Ok(record) => record,
            Err(error) => {
                self.mark_failed_internal(task_id, &error.to_string());
                return Err(error);
            }
        };

        // 4. 创建资源关联：context_kind=generation-output, role=result。
        let _ = self.with_resource_repository(|repo| {
            let draft = AssociationDraft::try_new(
                asset_id.clone(),
                "generation-output".to_owned(),
                format!("generation:{task_id}"),
                "result".to_owned(),
                None,
            )?;
            repo.create(draft).map_err(AppError::from)
        });

        // 5. 标记任务为 succeeded。
        self.with_generation_repository(|repo| {
            repo.update_status(task_id, GenerationStatus::Succeeded, None)
                .map_err(Into::into)
        })?;

        Ok(result)
    }

    /// 手动标记任务为失败。
    pub fn mark_failed(
        &self,
        task_id: &str,
        error_message: String,
    ) -> Result<GenerationTaskRecord, AppError> {
        validate_id(task_id, "taskId")?;
        self.with_generation_repository(|repo| {
            repo.update_status(task_id, GenerationStatus::Failed, Some(error_message))
                .map_err(Into::into)
        })
    }

    pub fn list_tasks(
        &self,
        status: Option<String>,
        provider_name: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<GenerationTaskRecord>, AppError> {
        let filter = GenerationFilter::try_new(status, provider_name, limit)?;
        self.with_generation_repository(|repo| repo.list_tasks(&filter).map_err(Into::into))
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<GenerationTaskRecord>, AppError> {
        validate_id(task_id, "taskId")?;
        self.with_generation_repository(|repo| repo.get_task(task_id).map_err(Into::into))
    }

    pub fn list_results(&self, task_id: &str) -> Result<Vec<GenerationResultRecord>, AppError> {
        validate_id(task_id, "taskId")?;
        self.with_generation_repository(|repo| repo.list_results(task_id).map_err(Into::into))
    }

    fn mark_failed_internal(&self, task_id: &str, error_message: &str) {
        let _ = self.with_generation_repository(|repo| {
            repo.update_status(
                task_id,
                GenerationStatus::Failed,
                Some(error_message.to_owned()),
            )
            .map_err(Into::into)
        });
    }

    fn with_generation_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn GenerationRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .generation_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }

    fn with_asset_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn AssetRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .asset_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }

    fn with_resource_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn ResourceRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .resource_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }
}

fn validate_id(value: &str, field: &'static str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::GenerationValidation(
            GenerationValidationError::Required { field },
        ));
    }
    if trimmed.len() != 36 || !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(AppError::GenerationValidation(
            GenerationValidationError::InvalidUuid { field },
        ));
    }
    Ok(())
}

impl crate::ports::reloadable::Reloadable for GenerationService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        let new_generation = SqliteGenerationRepository::open(database_path)?;
        let new_asset = SqliteAssetRepository::open(database_path)?;
        let new_resource = SqliteResourceRepository::open(database_path)?;
        let mut generation_repository = self
            .generation_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *generation_repository = Box::new(new_generation);
        let mut asset_repository = self
            .asset_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *asset_repository = Box::new(new_asset);
        let mut resource_repository = self
            .resource_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *resource_repository = Box::new(new_resource);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::GenerationService;
    use crate::adapters::sqlite::asset_repository::SqliteAssetRepository;
    use crate::adapters::sqlite::generation_repository::SqliteGenerationRepository;
    use crate::adapters::sqlite::resource_repository::SqliteResourceRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::application::error::AppError;
    use crate::domain::generation::GenerationStatus;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed_service() -> (tempfile::TempDir, GenerationService, String) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
            .unwrap();
        let workspace_id = workspace
            .get_status()
            .unwrap()
            .workspace
            .unwrap()
            .workspace_id
            .clone();
        drop(workspace);

        let generation_repo = SqliteGenerationRepository::open(&path).unwrap();
        let asset_repo = SqliteAssetRepository::open(&path).unwrap();
        let resource_repo = SqliteResourceRepository::open(&path).unwrap();
        let service = GenerationService::new(generation_repo, asset_repo, resource_repo, path);
        (directory, service, workspace_id)
    }

    #[test]
    fn submits_and_lists_a_pending_task() {
        let (_dir, service, workspace_id) = seed_service();
        let task = service
            .submit_task(
                workspace_id,
                "Seedance".to_owned(),
                "seedance-v2".to_owned(),
                "生成一段舞蹈视频".to_owned(),
            )
            .unwrap();
        assert_eq!(task.status, GenerationStatus::Pending);

        let tasks = service.list_tasks(None, None, None).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task.id);
    }

    #[test]
    fn records_output_through_managed_file_flow() {
        let (directory, service, workspace_id) = seed_service();
        let task = service
            .submit_task(
                workspace_id,
                "可灵".to_owned(),
                "kl-v1".to_owned(),
                "prompt".to_owned(),
            )
            .unwrap();

        let output_file = directory.path().join("generation-output.mp4");
        fs::write(&output_file, b"fake-video-content").unwrap();

        let result = service
            .record_output(&task.id, output_file.to_string_lossy().into_owned())
            .unwrap();
        assert_eq!(result.task_id, task.id);
        assert!(!result.asset_id.is_empty());

        let updated = service.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated.status, GenerationStatus::Succeeded);
        assert!(updated.completed_at.is_some());

        let results = service.list_results(&task.id).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].asset_id, result.asset_id);
    }

    #[test]
    fn marks_failed_when_source_file_does_not_exist() {
        let (_dir, service, workspace_id) = seed_service();
        let task = service
            .submit_task(
                workspace_id,
                "海螺".to_owned(),
                "hailuo-v1".to_owned(),
                "prompt".to_owned(),
            )
            .unwrap();

        let error = service
            .record_output(&task.id, "nonexistent/path/output.mp4".to_owned())
            .unwrap_err();

        let updated = service.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated.status, GenerationStatus::Failed);
        assert!(updated.error_message.is_some());
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn rejects_submit_with_empty_provider_name() {
        let (_dir, service, workspace_id) = seed_service();
        let error = service
            .submit_task(
                workspace_id,
                "  ".to_owned(),
                "model".to_owned(),
                "prompt".to_owned(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::GenerationValidation(
                crate::domain::generation::GenerationValidationError::Required {
                    field: "providerName"
                }
            )
        ));
    }

    #[test]
    fn rejects_get_task_with_invalid_id() {
        let (_dir, service, _workspace_id) = seed_service();
        let error = service.get_task("not-a-uuid").unwrap_err();
        assert!(matches!(
            error,
            AppError::GenerationValidation(
                crate::domain::generation::GenerationValidationError::InvalidUuid {
                    field: "taskId"
                }
            )
        ));
    }
}
