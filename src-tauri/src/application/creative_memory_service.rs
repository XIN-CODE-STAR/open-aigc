use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    adapters::sqlite::creative_memory_repository::SqliteCreativeMemoryRepository,
    application::error::AppError,
    domain::creative_memory::{
        CreativeMemoryDraft, CreativeMemoryEventRecord, CreativeMemoryRecord, MemoryEventType,
        MemoryFilter, MemoryStatus,
    },
    ports::creative_memory_repository::CreativeMemoryRepository,
};

pub struct CreativeMemoryService {
    repository: Mutex<Box<dyn CreativeMemoryRepository>>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl CreativeMemoryService {
    pub fn new(
        repository: impl CreativeMemoryRepository + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    /// 保存新的创意记忆。
    pub fn save_memory(
        &self,
        draft: CreativeMemoryDraft,
    ) -> Result<CreativeMemoryRecord, AppError> {
        self.with_repository(|repo| repo.insert(&draft).map_err(Into::into))
    }

    /// 获取单条记忆。
    pub fn get_memory(&self, id: &str) -> Result<Option<CreativeMemoryRecord>, AppError> {
        self.with_repository(|repo| repo.get(id).map_err(Into::into))
    }

    /// 按筛选条件列出记忆。
    pub fn list_memories(
        &self,
        filter: MemoryFilter,
    ) -> Result<Vec<CreativeMemoryRecord>, AppError> {
        self.with_repository(|repo| repo.list(&filter).map_err(Into::into))
    }

    /// 列出用户的所有活跃记忆。
    pub fn list_active_memories(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<CreativeMemoryRecord>, AppError> {
        let filter = MemoryFilter {
            scope: None,
            scope_ref_id: None,
            memory_type: None,
            status: Some(MemoryStatus::Active),
            limit,
        };
        let all = self.with_repository(|repo| repo.list(&filter).map_err(Into::into))?;
        // 仅返回该用户创建的记忆
        Ok(all
            .into_iter()
            .filter(|m| m.created_by == user_id)
            .collect())
    }

    /// 更新记忆状态（暂停/恢复/归档）。
    pub fn update_memory_status(
        &self,
        id: &str,
        status: MemoryStatus,
    ) -> Result<CreativeMemoryRecord, AppError> {
        self.with_repository(|repo| {
            let record = repo.update_status(id, status)?;
            let _ = repo.insert_event(
                id,
                match status {
                    MemoryStatus::Active => MemoryEventType::Resumed,
                    MemoryStatus::Paused => MemoryEventType::Paused,
                    MemoryStatus::Archived => MemoryEventType::Archived,
                },
                None,
                &record.created_by,
            );
            Ok(record)
        })
    }

    /// 更新记忆内容（用户编辑偏好）。
    pub fn update_memory_content(
        &self,
        id: &str,
        content_json: &str,
        summary: &str,
    ) -> Result<CreativeMemoryRecord, AppError> {
        self.with_repository(|repo| {
            let record = repo.update_content(id, content_json, summary)?;
            let _ = repo.insert_event(
                id,
                MemoryEventType::Updated,
                Some("用户编辑记忆内容"),
                &record.created_by,
            );
            Ok(record)
        })
    }

    /// 确认记忆（增加确认次数和置信度）。
    pub fn confirm_memory(&self, id: &str) -> Result<CreativeMemoryRecord, AppError> {
        self.with_repository(|repo| {
            let record = repo.confirm(id)?;
            let _ = repo.insert_event(
                id,
                MemoryEventType::Confirmed,
                Some("用户确认记忆"),
                &record.created_by,
            );
            Ok(record)
        })
    }

    /// 删除记忆（软删除）。
    pub fn delete_memory(&self, id: &str) -> Result<(), AppError> {
        self.with_repository(|repo| {
            let record = repo.get(id)?.ok_or_else(|| {
                AppError::new(
                    "memory not found",
                    std::io::Error::other(format!("memory {id} not found")),
                )
            })?;
            repo.archive(id)?;
            let _ = repo.insert_event(
                id,
                crate::domain::creative_memory::MemoryEventType::Deleted,
                Some("用户删除记忆"),
                &record.created_by,
            );
            Ok(())
        })
    }

    /// 获取记忆的事件历史。
    pub fn list_memory_events(
        &self,
        memory_id: &str,
    ) -> Result<Vec<CreativeMemoryEventRecord>, AppError> {
        self.with_repository(|repo| repo.list_events(memory_id).map_err(Into::into))
    }

    fn with_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn CreativeMemoryRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repo.as_mut())
    }
}

impl crate::ports::reloadable::Reloadable for CreativeMemoryService {
    fn reload(&self, database_path: &std::path::Path) -> Result<(), AppError> {
        let new_repo = SqliteCreativeMemoryRepository::open(database_path)?;
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *repo = Box::new(new_repo);
        Ok(())
    }
}
