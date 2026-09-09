use std::{path::PathBuf, sync::Arc};

use tauri::{AppHandle, Manager};

use crate::{
    application::{
        account_scheduler::AccountScheduler, agent_service::AgentService,
        asset_service::AssetService, backup_service::BackupService,
        creative_memory_service::CreativeMemoryService, credential_service::CredentialService,
        critic_service::CriticService, edit_understanding_service::EditUnderstandingService,
        error::AppError, generation_engine::register_configured_generation_providers,
        generation_pipeline::GenerationPipeline, generation_service::GenerationService,
        generation_submit_service::GenerationSubmitService, poll_worker::PollWorker,
        provider_registry::ProviderRegistry, provider_service::ProviderService,
        resource_service::ResourceService, workspace_service::WorkspaceService,
    },
    ports::reloadable::Reloadable,
};

/// 批量恢复后调用 reload_all 让所有服务重新打开数据库连接。
pub struct ServiceReloader {
    database_path: PathBuf,
}

impl ServiceReloader {
    pub fn new(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn reload_all(&self, app: &AppHandle) -> Result<(), AppError> {
        let path = &self.database_path;
        app.state::<WorkspaceService>().reload(path)?;
        app.state::<AssetService>().reload(path)?;
        app.state::<ResourceService>().reload(path)?;
        app.state::<BackupService>().reload(path)?;
        app.state::<GenerationService>().reload(path)?;
        app.state::<CredentialService>().reload(path)?;
        app.state::<ProviderService>().reload(path)?;
        let provider_registry = app.state::<Arc<ProviderRegistry>>();
        let account_scheduler = app.state::<Arc<AccountScheduler>>();
        register_configured_generation_providers(
            provider_registry.inner().as_ref(),
            path,
            account_scheduler.inner().as_ref(),
        )?;
        app.state::<Arc<GenerationPipeline>>()
            .inner()
            .as_ref()
            .reload(path)?;
        app.state::<GenerationSubmitService>().reload(path)?;
        app.state::<PollWorker>().reload(path)?;
        app.state::<AgentService>().reload(path)?;
        // AI Critic & Edit Understanding 热重载
        app.state::<CriticService>().reload(path)?;
        app.state::<EditUnderstandingService>().reload(path)?;
        // Creative Memory 热重载
        app.state::<CreativeMemoryService>().reload(path)?;
        Ok(())
    }
}
