// 路线图上的模块（视觉推理、嵌入、子代理、布局引擎、提示词编译等）接口先行、
// 尚未接入运行时；在正式接线前允许存在未使用项。
#![allow(dead_code)]

mod adapters;
mod application;
mod connectors;
mod domain;
mod ipc;
mod ports;
#[cfg(test)]
mod tests;

use adapters::agent::BuiltinToolExecutor;
use adapters::providers::openai_compatible::OpenAiCompatibleAdapter;
use adapters::providers::vision_adapter::{ClaudeVisionAdapter, GptVisionAdapter};
use adapters::sqlite::asset_repository::SqliteAssetRepository;
use adapters::sqlite::backup_repository::SqliteBackupRepository;
use adapters::sqlite::creative_memory_repository::SqliteCreativeMemoryRepository;
use adapters::sqlite::credential_repository::SqliteCredentialRepository;
use adapters::sqlite::edit_repository::SqliteEditRepository;
use adapters::sqlite::generation_attempt_repository::SqliteGenerationAttemptRepository;
use adapters::sqlite::generation_repository::SqliteGenerationRepository;
use adapters::sqlite::manga_repository::SqliteMangaRepository;
use adapters::sqlite::memory_canvas_repository::SqliteMemoryCanvasRepository;
use adapters::sqlite::resource_account_repository::SqliteResourceAccountRepository;
use adapters::sqlite::resource_repository::SqliteResourceRepository;
use adapters::sqlite::review_repository::SqliteReviewRepository;
use adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
use adapters::sqlite::SqliteAgentRepository;
use adapters::sqlite::SqlitePlanRepository;
use application::{
    agent_service::AgentService, asset_service::AssetService, backup_service::BackupService,
    creative_memory_service::CreativeMemoryService, credential_service::CredentialService,
    critic_service::CriticService, edit_understanding_service::EditUnderstandingService,
    generation_service::GenerationService, manga_service::MangaService,
    memory_canvas_service::MemoryCanvasService, provider_service::ProviderService,
    resource_account_service::ResourceAccountService, resource_service::ResourceService,
    service_reloader::ServiceReloader, workspace_service::WorkspaceService,
};
use base64::Engine;
use ipc::agent::{
    agent_v1_analyze_asset, agent_v1_analyze_assets_batch, agent_v1_create_conversation,
    agent_v1_debug_log, agent_v1_delete_conversation, agent_v1_get_conversation,
    agent_v1_list_conversations, agent_v1_list_invocations, agent_v1_list_messages,
    agent_v1_rename_conversation, agent_v1_search_assets_semantic, agent_v1_send_message,
    agent_v1_set_output_directory,
};
use ipc::assets::{
    asset_v1_delete, asset_v1_get, asset_v1_import, asset_v1_list, asset_v1_open_containing_folder,
    asset_v1_open_file, asset_v1_reverify,
};
use ipc::backup::{backup_v1_create, backup_v1_preview_restore, backup_v1_restore};
use ipc::canvas::{
    canvas_v1_add_edge, canvas_v1_add_node, canvas_v1_create, canvas_v1_delete,
    canvas_v1_delete_edge, canvas_v1_delete_node, canvas_v1_get, canvas_v1_list,
    canvas_v1_list_edges, canvas_v1_list_nodes, canvas_v1_update_node,
};
use ipc::creative_memory::{
    creative_memory_v1_confirm, creative_memory_v1_delete, creative_memory_v1_get,
    creative_memory_v1_list, creative_memory_v1_list_events, creative_memory_v1_save,
    creative_memory_v1_update_content, creative_memory_v1_update_status,
};
use ipc::creative_state::{
    creative_state_v1_add_character, creative_state_v1_add_reference, creative_state_v1_add_scene,
    creative_state_v1_get, creative_state_v1_get_prompt_context,
    creative_state_v1_get_style_tokens, creative_state_v1_save_decision,
    creative_state_v1_update_style, creative_state_v1_upsert,
};
use ipc::credentials::{
    credential_v1_create, credential_v1_delete, credential_v1_list, credential_v1_update,
};
use ipc::edit::{
    edit_v1_apply_plan, edit_v1_get_plan_by_request, edit_v1_get_request,
    edit_v1_get_understanding_result, edit_v1_list_requests, edit_v1_skip_request,
    edit_v1_submit_feedback,
};
use ipc::generations::{
    generation_v1_get_task, generation_v1_list_results, generation_v1_list_tasks,
    generation_v1_mark_failed, generation_v1_record_output, generation_v1_submit,
};
use ipc::image_analyzer::image_v1_analyze;
use ipc::manga::{
    manga_v1_create_character, manga_v1_create_project, manga_v1_create_scene,
    manga_v1_create_shot, manga_v1_delete_project, manga_v1_get_project, manga_v1_list_characters,
    manga_v1_list_projects, manga_v1_list_scenes, manga_v1_list_shots, manga_v1_upsert_story_bible,
};
use ipc::memory::{memory_v1_search, memory_v1_status};
use ipc::memory_canvas::{
    memory_canvas_v1_create, memory_canvas_v1_delete, memory_canvas_v1_get, memory_canvas_v1_list,
    memory_edge_v1_add, memory_edge_v1_delete, memory_edge_v1_list, memory_node_v1_add,
    memory_node_v1_delete, memory_node_v1_list, memory_node_v1_update, memory_viewport_v1_get,
    memory_viewport_v1_save,
};
use ipc::model_router::{
    model_router_v1_list_models, model_router_v1_record_outcome, model_router_v1_route,
};
use ipc::queue::{
    queue_v1_cancel_attempt, queue_v1_get_attempt, queue_v1_list_active, queue_v1_list_attempts,
    queue_v1_retry_attempt, queue_v1_submit_attempt,
};
use ipc::resource_accounts::{
    resource_account_v1_create, resource_account_v1_delete, resource_account_v1_list,
    resource_account_v1_set_enabled, resource_account_v1_update,
    resource_account_v1_update_session,
};
use ipc::resources::{resource_v1_create, resource_v1_delete, resource_v1_get, resource_v1_list};
use ipc::review::{
    asset_v1_get_license, asset_v1_list_versions, content_guard_v1_get_report,
    review_v1_get_report, review_v1_list_dimensions, review_v1_list_reports,
};
use ipc::vision::{review_v1_evaluate_with_vision, review_v1_list_vision_adapters};
use ipc::workflow::{
    workflow_v1_advance, workflow_v1_fail, workflow_v1_get, workflow_v1_list,
    workflow_v1_list_waiting, workflow_v1_mark_plan_created, workflow_v1_pause, workflow_v1_resume,
    workflow_v1_start,
};
use ipc::workspace::{
    workspace_v1_get_root_path, workspace_v1_get_status, workspace_v1_initialize,
    workspace_v1_rename,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let workspace_directory = app.path().app_local_data_dir()?.join("workspace");
            let database_path = workspace_directory.join("aigc-studio.sqlite3");
            let workspace_repository = SqliteWorkspaceRepository::open(&database_path)?;
            let asset_repository = SqliteAssetRepository::open(&database_path)?;
            let resource_repository = SqliteResourceRepository::open(&database_path)?;
            let backup_repository = SqliteBackupRepository::open(&database_path)?;
            let generation_repository = SqliteGenerationRepository::open(&database_path)?;
            let credential_repository = SqliteCredentialRepository::open(&database_path)?;

            if !app.manage(WorkspaceService::new(
                workspace_repository,
                database_path.clone(),
            )) {
                return Err("workspace service was already initialized".into());
            }
            if !app.manage(AssetService::new(
                asset_repository,
                workspace_directory.clone(),
                database_path.clone(),
            )) {
                return Err("asset service was already initialized".into());
            }
            if !app.manage(ResourceService::new(
                resource_repository,
                database_path.clone(),
            )) {
                return Err("resource service was already initialized".into());
            }
            // EverOS 记忆根目录（在 workspace_directory 移动前计算）。
            let everos_root = workspace_directory.join("everos-memory");
            let download_dir = workspace_directory.join("downloads");
            let creative_runtime_workspace = workspace_directory.to_string_lossy().to_string();

            // SandboxPolicy：工具执行沙箱策略（默认限制系统目录 + 已知网络）。
            // 在 workspace_directory 移动前创建。
            let sandbox_policy = std::sync::Arc::new(
                crate::domain::sandbox::SandboxPolicy::default()
                    .with_workspace(workspace_directory.clone()),
            );
            app.manage(std::sync::Arc::clone(&sandbox_policy));

            // EventBus：统一事件总线（AgentEvent / RuntimeEvent / TaskEvent / WorkflowEvent）。
            let event_bus = std::sync::Arc::new(
                crate::application::event_bus::EventBus::new(),
            );
            event_bus.register(Box::new(
                crate::application::event_bus::LoggingEventListener,
            ));
            event_bus.register(Box::new(
                crate::application::event_bus::TauriEventEmitter::new(app.handle().clone()),
            ));
            app.manage(std::sync::Arc::clone(&event_bus));

            if !app.manage(BackupService::new(
                backup_repository,
                workspace_directory,
                database_path.clone(),
            )) {
                return Err("backup service was already initialized".into());
            }
            // GenerationService 需要独立的 asset/resource 仓库实例来导入 AI 输出并创建资源关联。
            let generation_asset_repository = SqliteAssetRepository::open(&database_path)?;
            let generation_resource_repository = SqliteResourceRepository::open(&database_path)?;
            if !app.manage(GenerationService::new(
                generation_repository,
                generation_asset_repository,
                generation_resource_repository,
                database_path.clone(),
            )) {
                return Err("generation service was already initialized".into());
            }
            // CredentialService 管理 Provider 凭据元信息，密钥通过 OS keychain 存储。
            if !app.manage(CredentialService::new(
                credential_repository,
                database_path.clone(),
            )) {
                return Err("credential service was already initialized".into());
            }
            // ResourceAccountService 管理用户 AI 账号（session/cookie 类资源）。
            let resource_account_repository =
                SqliteResourceAccountRepository::open(&database_path)?;
            if !app.manage(ResourceAccountService::new(
                resource_account_repository,
                database_path.clone(),
            )) {
                return Err("resource account service was already initialized".into());
            }
            // ProviderService 使用独立的凭据仓库实例读取 API 密钥。
            let provider_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            if !app.manage(ProviderService::new(
                provider_credential_repository,
                OpenAiCompatibleAdapter::new(),
                database_path.clone(),
            )) {
                return Err("provider service was already initialized".into());
            }

            // Shared generation engine infrastructure.
            // Agent tools, queue submission, and PollWorker must use the same instances.
            let provider_registry = std::sync::Arc::new(
                crate::application::provider_registry::ProviderRegistry::new(),
            );

            // 注入 CredentialManager，使 ProviderRegistry 能解析 resource_accounts 的 session。
            let credential_svc_for_mgr = CredentialService::new(
                SqliteCredentialRepository::open(&database_path)?,
                database_path.clone(),
            );
            let credential_manager = std::sync::Arc::new(
                crate::application::credential_manager::CredentialManager::new(
                    credential_svc_for_mgr,
                ),
            );
            provider_registry.set_credential_manager(credential_manager);

            // 多账号调度器（Priority 策略：始终选最高优先级可用账号）。
            let account_scheduler = std::sync::Arc::new(
                crate::application::account_scheduler::AccountScheduler::new(
                    crate::application::account_scheduler::ScheduleStrategy::Priority,
                ),
            );

            // Register configured generation providers from stored credentials.
            let registered_providers =
                crate::application::generation_engine::register_configured_generation_providers(
                    &provider_registry,
                    &database_path,
                    &account_scheduler,
                )?;
            eprintln!("[Engine] Registered configured providers: {registered_providers}");

            // GenerationTaskRuntime：统一生成任务运行时（TaskRuntime trait 实现）。
            let task_generation_repo = SqliteGenerationRepository::open(&database_path)?;
            let task_runtime: std::sync::Arc<dyn crate::ports::task_runtime::TaskRuntime> =
                std::sync::Arc::new(
                    crate::application::task_runtime_impl::GenerationTaskRuntime::new(
                        std::sync::Arc::clone(&provider_registry),
                        task_generation_repo,
                    ),
                );
            let sub_agent_task_runtime = std::sync::Arc::clone(&task_runtime);
            app.manage(task_runtime);

            std::fs::create_dir_all(&download_dir).ok();
            let pipeline_review_repo = SqliteReviewRepository::open(&database_path)?;
            let pipeline_asset_repo = SqliteAssetRepository::open(&database_path)?;
            let pipeline = std::sync::Arc::new(
                crate::application::generation_pipeline::GenerationPipeline::new(
                    download_dir,
                    pipeline_review_repo,
                    pipeline_asset_repo,
                ),
            );

            // AgentService 协调 Plan-and-Execute 循环：规划 + 工具调用 + 持久化。
            let agent_repository = SqliteAgentRepository::open(&database_path)?;
            let agent_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            let tool_generation_repository = SqliteGenerationRepository::open(&database_path)?;
            let tool_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            let tool_router_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            let tool_router_credential_service = CredentialService::new(
                tool_router_credential_repository,
                database_path.clone(),
            );
            let tool_model_router = crate::application::model_router_service::ModelRouterService::new(
                tool_router_credential_service,
                database_path.clone(),
            );
            let tool_attempt_repository = SqliteGenerationAttemptRepository::open(&database_path)?;
            let tool_submitter =
                crate::application::generation_submit_service::GenerationSubmitService::new(
                    tool_attempt_repository,
                    std::sync::Arc::clone(&provider_registry),
                    std::sync::Arc::clone(&pipeline),
                    database_path.clone(),
                )?;

            // SemanticRepository + SemanticRetrievalService — 需要在 tool_executor 创建前初始化。
            let semantic_repo: std::sync::Arc<dyn crate::ports::semantic_repository::SemanticRepository> = std::sync::Arc::new(
                crate::adapters::sqlite::semantic_repository::SqliteSemanticRepository::open(
                    rusqlite::Connection::open(&database_path)?,
                ),
            );
            let semantic_retrieval_service = std::sync::Arc::new(
                crate::application::semantic::retrieval_service::SemanticRetrievalService::new(
                    std::sync::Arc::clone(&semantic_repo),
                ),
            );

            // SemanticPipelineService — 图片语义分析管线（为 tool_executor 创建一份）。
            let semantic_pipeline_for_tools = {
                use crate::ports::captioning_port::CaptioningPort;
                let mut captioners: Vec<std::sync::Arc<dyn CaptioningPort>> = Vec::new();

                // DashScope Qwen-VL（云端）— 从凭据库获取 API Key
                let dashscope_key = {
                    let cred_service = app.state::<CredentialService>();
                    let creds = cred_service.list().unwrap_or_default();
                    creds.iter()
                        .find(|c| {
                            c.provider_name.to_lowercase().contains("dashscope")
                                || c.base_url.contains("dashscope")
                        })
                        .and_then(|c| cred_service.get_secret(&c.credential_key).ok())
                        .unwrap_or_default()
                };
                if !dashscope_key.is_empty() {
                    captioners.push(std::sync::Arc::new(
                        crate::adapters::providers::dashscope_captioner::DashScopeCaptioner::new(dashscope_key),
                    ));
                }

                // Florence-2（本地）— 检查本地服务是否运行
                let florence = crate::adapters::providers::florence_captioner::FlorenceCaptioner::new();
                if florence.is_ready() {
                    captioners.push(std::sync::Arc::new(florence));
                }

                eprintln!("[SemanticPipeline] initialized with {} captioners", captioners.len());
                std::sync::Arc::new(
                    crate::application::semantic::pipeline_service::SemanticPipelineService::new(
                        captioners,
                        std::sync::Arc::clone(&semantic_repo),
                    ),
                )
            };

            let tool_executor = BuiltinToolExecutor::new(
                tool_generation_repository,
                tool_credential_repository,
            )
            .with_model_router(tool_model_router)
            .with_generation_submitter(tool_submitter)
            .with_vision_adapter(ClaudeVisionAdapter::new())
            .with_canvas_context_service({
                let canvas_repo = std::sync::Arc::new(std::sync::Mutex::new(
                    crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                ));
                std::sync::Arc::new(
                    crate::application::canvas_context_service::CanvasContextService::new(canvas_repo),
                )
            })
            .with_apply_script_plan_service({
                let canvas_repo = std::sync::Arc::new(std::sync::Mutex::new(
                    crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                ));
                std::sync::Arc::new(
                    crate::application::apply_script_plan_service::ApplyScriptPlanService::new(canvas_repo),
                )
            })
            .with_semantic_pipeline_service(std::sync::Arc::clone(&semantic_pipeline_for_tools))
            .with_semantic_retrieval_service(std::sync::Arc::clone(&semantic_retrieval_service))
            .with_provider_registry(std::sync::Arc::clone(&provider_registry));
            let plan_repository = SqlitePlanRepository::open(&database_path)?;

            // MemoryService：EverOS 长期记忆（默认禁用，用户在设置中启用）。
            let memory_config = crate::application::memory_service::MemoryServiceConfig {
                enabled: everos_root.exists(),
                port: 18000,
                root_path: everos_root.to_string_lossy().to_string(),
                search_method: "hybrid".to_owned(),
            };
            let memory_service = crate::application::memory_service::MemoryServiceImpl::new(
                memory_config,
            );
            // Clone for AgentService before managing.
            let memory_for_agent = memory_service.clone();
            // Clone for MemoryRuntime (used outside the desktop cfg block).
            let memory_for_runtime = memory_for_agent.clone();
            app.manage(memory_service);

            // MangaService：AI 漫剧项目管理。
            let manga_repository = SqliteMangaRepository::open(&database_path)?;
            app.manage(MangaService::new(manga_repository));

            // MemoryCanvasService：记忆画布（节点/连线/视口持久化）。
            let memory_canvas_repository = SqliteMemoryCanvasRepository::open(&database_path)?;
            app.manage(MemoryCanvasService::new(memory_canvas_repository));

            // CanvasDomainService：新画布架构（CanvasNode/Edge 领域模型）。
            let canvas_domain_repo = std::sync::Arc::new(std::sync::Mutex::new(
                crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
            ));
            app.manage(crate::application::canvas_domain_service::CanvasDomainService::new(canvas_domain_repo));

            // RelationDiscoveryService + ContextOrchestrator。
            let _relation_discovery_service = std::sync::Arc::new(
                crate::application::semantic::relation_discovery_service::RelationDiscoveryService::new(
                    std::sync::Arc::clone(&semantic_retrieval_service),
                    std::sync::Arc::clone(&semantic_repo),
                    std::sync::Arc::new(
                        crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                    ) as std::sync::Arc<dyn crate::ports::canvas_repository::CanvasRepository>,
                ),
            );

            let context_orchestrator = std::sync::Arc::new(
                crate::application::context_orchestrator::ContextOrchestrator::new(
                    // Reuse the canvas context service created for the first tool executor
                    {
                        let canvas_repo = std::sync::Arc::new(std::sync::Mutex::new(
                            crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                        ));
                        std::sync::Arc::new(
                            crate::application::canvas_context_service::CanvasContextService::new(canvas_repo),
                        )
                    },
                    std::sync::Arc::clone(&semantic_retrieval_service),
                ),
            );
            app.manage(context_orchestrator);

            // Semantic Pipeline + Retrieval — 图片语义分析与 RAG 检索。
            app.manage(std::sync::Arc::clone(&semantic_pipeline_for_tools));
            app.manage(std::sync::Arc::clone(&semantic_retrieval_service));

            // AnalysisJobService — 异步分析任务生命周期管理。
            let analysis_job_repository = std::sync::Arc::new(
                crate::adapters::sqlite::SqliteAnalysisJobRepository::open(
                    rusqlite::Connection::open(&database_path)?,
                ),
            );
            let analysis_job_service = std::sync::Arc::new(
                crate::application::analysis::job_service::AnalysisJobService::new(
                    analysis_job_repository,
                ),
            );
            app.manage(std::sync::Arc::clone(&analysis_job_service));

            // AnalysisCacheService — 分析结果缓存。
            let analysis_cache = std::sync::Arc::new(
                crate::application::analysis::cache_service::AnalysisCacheService::new(1000),
            );
            app.manage(std::sync::Arc::clone(&analysis_cache));

            // VisionRouter — 视觉任务复杂度路由器。
            let vision_router = std::sync::Arc::new(
                crate::application::vision_router::VisionRouter::new(
                    None, // simple_parser
                    None, // medium_parser
                    None, // complex_reasoner
                ),
            );
            app.manage(std::sync::Arc::clone(&vision_router));

            // AnalysisPipeline — 解析 + 分析 + 持久化管线。
            // 使用 semantic_pipeline_for_tools 作为内置解析器。
            let analysis_pipeline = std::sync::Arc::new(
                crate::application::analysis::pipeline::AnalysisPipeline::new(
                    vec![],
                    std::sync::Arc::clone(&semantic_repo),
                ),
            );
            app.manage(analysis_pipeline);

            // GenerationQueueService：持久化生成任务队列。
            let attempt_repository = SqliteGenerationAttemptRepository::open(&database_path)?;
            app.manage(crate::application::generation_queue_service::GenerationQueueService::new(
                attempt_repository,
                database_path.clone(),
            ));

            // CreativeStateService（Arc 共享给 AgentService）。
            let creative_state_repo_for_agent =
                adapters::sqlite::creative_state_repository::SqliteCreativeStateRepository::open(
                    &database_path,
                )?;
            let creative_state_arc = std::sync::Arc::new(
                crate::application::creative_state_service::CreativeStateService::new(
                    creative_state_repo_for_agent,
                    database_path.clone(),
                ),
            );

            // CreativeDirectorService（创作决策层，注入 AgentService 规划阶段）。
            let creative_director_arc = std::sync::Arc::new(
                crate::application::creative_director_service::CreativeDirectorService::new(
                    std::sync::Arc::clone(&creative_state_arc),
                ),
            );

            // CreativeRuntimeService：四阶段创作流水线编排器。
            let runtime_router_credential_repo = SqliteCredentialRepository::open(&database_path)?;
            let runtime_router_credential_svc = CredentialService::new(
                runtime_router_credential_repo,
                database_path.clone(),
            );
            let runtime_model_router = std::sync::Arc::new(
                crate::application::model_router_service::ModelRouterService::new(
                    runtime_router_credential_svc,
                    database_path.clone(),
                ),
            );
            let runtime_gen_facade: std::sync::Arc<
                dyn crate::application::generation_facade::GenerationFacade,
            > = std::sync::Arc::new(
                crate::application::generation_facade::RealGenerationFacade::new(
                    runtime_model_router,
                    std::sync::Arc::clone(&provider_registry),
                ),
            );
            let runtime_asset_repo = SqliteAssetRepository::open(&database_path)?;
            let runtime_asset_svc = std::sync::Arc::new(AssetService::new(
                runtime_asset_repo,
                std::path::PathBuf::from(&creative_runtime_workspace),
                database_path.clone(),
            ));
            let runtime_importer = std::sync::Arc::new(
                crate::application::artifact_importer::ArtifactImporter::new(runtime_asset_svc),
            );
            let runtime_composition_facade: std::sync::Arc<
                dyn crate::application::composition_facade::CompositionFacade,
            > = if let Some(ffmpeg_path) = crate::application::composite_skill::detect_ffmpeg() {
                eprintln!("[Setup] CreativeRuntime: engine=ffmpeg, path={ffmpeg_path}");
                std::sync::Arc::new(
                    crate::application::composition_facade::FfmpegCompositionFacade::new(
                        ffmpeg_path,
                    ),
                )
            } else {
                eprintln!("[Setup] CreativeRuntime: ffmpeg not found, using mock composition");
                std::sync::Arc::new(
                    crate::application::composition_facade::MockCompositionFacade,
                )
            };
            // RuntimeEventBus：创作流水线事件广播。
            let mut runtime_event_bus = crate::application::runtime_event_bus::RuntimeEventBus::new();
            runtime_event_bus.register(Box::new(
                crate::adapters::tauri_runtime_emitter::TauriRuntimeEmitter::new(app.handle().clone()),
            ));
            runtime_event_bus.register(Box::new(
                crate::ports::runtime_event_emitter::LoggingRuntimeListener,
            ));
            let runtime_event_bus_arc = std::sync::Arc::new(runtime_event_bus);

            let creative_runtime_arc = std::sync::Arc::new(
                crate::application::creative_runtime_service::CreativeRuntimeService::new(
                    runtime_gen_facade,
                    std::sync::Arc::clone(&provider_registry),
                    runtime_importer,
                    runtime_composition_facade,
                    creative_runtime_workspace.clone(),
                )
                .with_event_bus(runtime_event_bus_arc),
            );

            let agent_service = {
                // AgentRuntime：拆分后的核心编排器。
                // 使用独立的 Repository 实例（SQLite 支持多连接）。
                let rt_agent_repo = SqliteAgentRepository::open(&database_path)?;
                let rt_credential_repo = SqliteCredentialRepository::open(&database_path)?;
                let rt_plan_repo = SqlitePlanRepository::open(&database_path)?;
                let rt_agent_repo_for_exec = SqliteAgentRepository::open(&database_path)?;
                let rt_tool_generation_repo = SqliteGenerationRepository::open(&database_path)?;
                let rt_tool_credential_repo = SqliteCredentialRepository::open(&database_path)?;
                let rt_tool_router_credential_repo = SqliteCredentialRepository::open(&database_path)?;
                let rt_tool_router_credential_service = CredentialService::new(
                    rt_tool_router_credential_repo,
                    database_path.clone(),
                );
                let rt_tool_model_router = crate::application::model_router_service::ModelRouterService::new(
                    rt_tool_router_credential_service,
                    database_path.clone(),
                );
                let rt_tool_attempt_repo = SqliteGenerationAttemptRepository::open(&database_path)?;
                let rt_tool_submitter =
                    crate::application::generation_submit_service::GenerationSubmitService::new(
                        rt_tool_attempt_repo,
                        std::sync::Arc::clone(&provider_registry),
                        std::sync::Arc::clone(&pipeline),
                        database_path.clone(),
                    )?;
                let rt_tool_executor = BuiltinToolExecutor::new(
                    rt_tool_generation_repo,
                    rt_tool_credential_repo,
                )
                .with_model_router(rt_tool_model_router)
                .with_generation_submitter(rt_tool_submitter)
                .with_vision_adapter(ClaudeVisionAdapter::new())
                .with_canvas_context_service({
                    let canvas_repo = std::sync::Arc::new(std::sync::Mutex::new(
                        crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                    ));
                    std::sync::Arc::new(
                        crate::application::canvas_context_service::CanvasContextService::new(canvas_repo),
                    )
                })
                .with_apply_script_plan_service({
                    let canvas_repo = std::sync::Arc::new(std::sync::Mutex::new(
                        crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(&database_path)?,
                    ));
                    std::sync::Arc::new(
                        crate::application::apply_script_plan_service::ApplyScriptPlanService::new(canvas_repo),
                    )
                })
                .with_semantic_pipeline_service(std::sync::Arc::clone(&semantic_pipeline_for_tools))
                .with_semantic_retrieval_service(std::sync::Arc::clone(&semantic_retrieval_service))
                .with_provider_registry(std::sync::Arc::clone(&provider_registry));

                let planning_engine = crate::application::planning_engine::PlanningEngine::new(
                    rt_plan_repo,
                    Some(std::sync::Arc::clone(&creative_state_arc)),
                )
                .with_creative_director(std::sync::Arc::clone(&creative_director_arc));

                let execution_engine = crate::application::execution_engine::ExecutionEngine::new(
                    rt_agent_repo_for_exec,
                    rt_tool_executor,
                    database_path.clone(),
                )
                .with_sandbox(std::sync::Arc::clone(&sandbox_policy));

                let memory_context_builder =
                    crate::application::memory_context_builder::MemoryContextBuilder::new(
                        Some(memory_for_agent.clone()),
                    );

                let agent_runtime = crate::application::agent_runtime::AgentRuntime::new(
                    rt_agent_repo,
                    rt_credential_repo,
                    planning_engine,
                    execution_engine,
                    memory_context_builder,
                    database_path.clone(),
                )
                .with_event_bus(std::sync::Arc::clone(&event_bus))
                .with_semantic_pipeline(std::sync::Arc::clone(&semantic_pipeline_for_tools));

                AgentService::new(
                    agent_repository,
                    agent_credential_repository,
                    tool_executor,
                    plan_repository,
                    Some(memory_for_agent),
                    Some(std::sync::Arc::clone(&creative_state_arc)),
                    database_path.clone(),
                )
                .with_generation_engine(
                    std::sync::Arc::clone(&provider_registry),
                    std::sync::Arc::clone(&pipeline),
                )
                .with_creative_director(creative_director_arc)
                .with_creative_runtime(creative_runtime_arc)
                .with_agent_runtime(agent_runtime)
            };

            if !app.manage(agent_service) {
                return Err("agent service was already initialized".into());
            }

            // SubAgentRuntime：统一子 Agent 生命周期管理。
            let sub_agent_runtime = std::sync::Arc::new(
                crate::application::sub_agent_runtime::SubAgentRuntime::new(
                    sub_agent_task_runtime,
                )
                .with_event_bus(std::sync::Arc::clone(&event_bus)),
            );
            app.manage(sub_agent_runtime);

            // MemoryRuntime：系统级记忆基础设施。
            let memory_runtime = std::sync::Arc::new(
                crate::application::memory_runtime::MemoryRuntime::new(
                    Some(memory_for_runtime),
                ),
            );
            app.manage(memory_runtime);

            // CriticService：AI Critic Agent 多维审美评价。
            let critic_repository = SqliteReviewRepository::open(&database_path)?;
            if !app.manage(CriticService::new(
                critic_repository,
                database_path.clone(),
            )) {
                return Err("critic service was already initialized".into());
            }

            // EditUnderstandingService：用户自然语言反馈解析与修改计划。
            let edit_repository = SqliteEditRepository::open(&database_path)?;
            if !app.manage(EditUnderstandingService::new(
                edit_repository,
                database_path.clone(),
            )) {
                return Err("edit understanding service was already initialized".into());
            }

            // VisionCriticService：LLM Vision API 自动评价。
            // 复用已创建的 CriticService，添加 Claude/GPT Vision 适配器。
            let vision_critic_repository = SqliteReviewRepository::open(&database_path)?;
            let vision_critic_inner = CriticService::new(
                vision_critic_repository,
                database_path.clone(),
            );
            // 创建独立的 CredentialService 实例供 Vision 使用
            let vision_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            let vision_credential_service = CredentialService::new(
                vision_credential_repository,
                database_path.clone(),
            );
            let vision_adapters: Vec<Box<dyn crate::ports::vision_adapter::VisionAdapter>> = vec![
                Box::new(ClaudeVisionAdapter::new()),
                Box::new(GptVisionAdapter::new()),
            ];
            app.manage(crate::application::vision_critic_service::VisionCriticService::new(
                vision_critic_inner,
                vision_credential_service,
                vision_adapters,
                "claude-vision".to_owned(),
                "claude-sonnet-4-20250514".to_owned(),
                None,
            ));

            // CreativeMemoryService：用户创意记忆系统。
            let creative_memory_repository = SqliteCreativeMemoryRepository::open(&database_path)?;
            app.manage(CreativeMemoryService::new(
                creative_memory_repository,
                database_path.clone(),
            ));

            // CreativeStateService：项目级创作状态空间。
            let creative_state_repository =
                adapters::sqlite::creative_state_repository::SqliteCreativeStateRepository::open(
                    &database_path,
                )?;
            app.manage(
                crate::application::creative_state_service::CreativeStateService::new(
                    creative_state_repository,
                    database_path.clone(),
                ),
            );

            // ModelRouterService：智能模型路由调度。
            let model_router_credential_repository = SqliteCredentialRepository::open(&database_path)?;
            let model_router_credential_service = CredentialService::new(
                model_router_credential_repository,
                database_path.clone(),
            );
            app.manage(crate::application::model_router_service::ModelRouterService::new(
                model_router_credential_service,
                database_path.clone(),
            ));

            // WorkflowService：创作工作流自动评价闭环。
            app.manage(crate::application::workflow_service::WorkflowService::new(
                database_path.clone(),
            ));

            // 创建 PollWorker 并启动
            let poll_attempt_repo = SqliteGenerationAttemptRepository::open(&database_path)?;
            let mut poll_worker = crate::application::poll_worker::PollWorker::new(
                std::sync::Arc::new(std::sync::Mutex::new(
                    Box::new(poll_attempt_repo) as Box<dyn crate::ports::generation_attempt_repository::GenerationAttemptRepository>
                )),
                std::sync::Arc::clone(&provider_registry),
                std::sync::Arc::clone(&pipeline),
                crate::application::poll_worker::PollWorkerConfig::default(),
            );
            poll_worker.set_app_handle(app.handle().clone());
            poll_worker.start().map_err(|e| {
                eprintln!("[Engine] Failed to start PollWorker: {e}");
                e
            })?;

            eprintln!("[Engine] Generation engine started. Providers: {}", provider_registry.count());

            // 步骤二：创建 GenerationSubmitService（提交链路：真正调用 Provider）
            let submit_attempt_repo = SqliteGenerationAttemptRepository::open(&database_path)?;
            let submit_service = crate::application::generation_submit_service::GenerationSubmitService::new(
                submit_attempt_repo,
                std::sync::Arc::clone(&provider_registry),
                std::sync::Arc::clone(&pipeline),
                database_path.clone(),
            )?;

            // 存入 Tauri State
            app.manage(provider_registry);
            app.manage(account_scheduler);
            app.manage(pipeline);
            app.manage(poll_worker);

            // AccountHealthWorker：定期检查 AI 账号 session 有效性。
            let mut health_worker =
                crate::application::account_health_worker::AccountHealthWorker::new(
                    database_path.clone(),
                    crate::application::account_health_worker::AccountHealthConfig::default(),
                );
            health_worker.set_app_handle(app.handle().clone());
            if let Err(e) = health_worker.start() {
                eprintln!("[Engine] Failed to start AccountHealthWorker: {e}");
            }
            app.manage(health_worker);

            app.manage(submit_service);

            // ServiceReloader 用于批量恢复后热重载所有服务的数据库连接。
            app.manage(ServiceReloader::new(database_path));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            file_read_as_data_url,
            open_file_with_system_viewer,
            workspace_v1_get_status,
            workspace_v1_get_root_path,
            workspace_v1_initialize,
            workspace_v1_rename,
            asset_v1_list,
            asset_v1_get,
            asset_v1_import,
            asset_v1_reverify,
            asset_v1_delete,
            asset_v1_open_file,
            asset_v1_open_containing_folder,
            resource_v1_list,
            resource_v1_get,
            resource_v1_create,
            resource_v1_delete,
            backup_v1_create,
            backup_v1_preview_restore,
            backup_v1_restore,
            generation_v1_submit,
            generation_v1_record_output,
            generation_v1_mark_failed,
            generation_v1_list_tasks,
            generation_v1_get_task,
            generation_v1_list_results,
            credential_v1_list,
            credential_v1_create,
            credential_v1_update,
            credential_v1_delete,
            resource_account_v1_list,
            resource_account_v1_create,
            resource_account_v1_update,
            resource_account_v1_update_session,
            resource_account_v1_set_enabled,
            resource_account_v1_delete,
            agent_v1_create_conversation,
            agent_v1_list_conversations,
            agent_v1_get_conversation,
            agent_v1_delete_conversation,
            agent_v1_rename_conversation,
            agent_v1_list_messages,
            agent_v1_list_invocations,
            agent_v1_send_message,
            agent_v1_set_output_directory,
            agent_v1_debug_log,
            memory_v1_status,
            memory_v1_search,
            manga_v1_create_project,
            manga_v1_list_projects,
            manga_v1_get_project,
            manga_v1_delete_project,
            manga_v1_create_scene,
            manga_v1_list_scenes,
            manga_v1_create_shot,
            manga_v1_list_shots,
            manga_v1_create_character,
            manga_v1_list_characters,
            manga_v1_upsert_story_bible,
            queue_v1_submit_attempt,
            queue_v1_get_attempt,
            queue_v1_list_attempts,
            queue_v1_list_active,
            queue_v1_cancel_attempt,
            queue_v1_retry_attempt,
            // AI Critic Agent
            review_v1_list_reports,
            review_v1_get_report,
            review_v1_list_dimensions,
            // Asset Version & License
            asset_v1_list_versions,
            asset_v1_get_license,
            // Content Guard
            content_guard_v1_get_report,
            // Edit Understanding Agent
            edit_v1_submit_feedback,
            edit_v1_list_requests,
            edit_v1_get_request,
            edit_v1_get_plan_by_request,
            edit_v1_apply_plan,
            edit_v1_skip_request,
            edit_v1_get_understanding_result,
            // LLM Vision API
            review_v1_evaluate_with_vision,
            review_v1_list_vision_adapters,
            // Creative Memory
            creative_memory_v1_save,
            creative_memory_v1_get,
            creative_memory_v1_list,
            creative_memory_v1_update_status,
            creative_memory_v1_update_content,
            creative_memory_v1_confirm,
            creative_memory_v1_delete,
            creative_memory_v1_list_events,
            // Memory Canvas
            memory_canvas_v1_create,
            memory_canvas_v1_list,
            memory_canvas_v1_get,
            memory_canvas_v1_delete,
            memory_node_v1_add,
            memory_node_v1_update,
            memory_node_v1_delete,
            memory_node_v1_list,
            memory_edge_v1_add,
            memory_edge_v1_delete,
            memory_edge_v1_list,
            memory_viewport_v1_save,
            memory_viewport_v1_get,
            // Canvas (new architecture)
            canvas_v1_create,
            canvas_v1_list,
            canvas_v1_get,
            canvas_v1_delete,
            canvas_v1_list_nodes,
            canvas_v1_add_node,
            canvas_v1_update_node,
            canvas_v1_delete_node,
            canvas_v1_list_edges,
            canvas_v1_add_edge,
            canvas_v1_delete_edge,
            // Creative State
            creative_state_v1_get,
            creative_state_v1_upsert,
            creative_state_v1_update_style,
            creative_state_v1_add_character,
            creative_state_v1_add_scene,
            creative_state_v1_add_reference,
            creative_state_v1_save_decision,
            creative_state_v1_get_style_tokens,
            creative_state_v1_get_prompt_context,
            // Model Router
            model_router_v1_route,
            model_router_v1_list_models,
            model_router_v1_record_outcome,
            // Workflow
            workflow_v1_start,
            workflow_v1_advance,
            workflow_v1_pause,
            workflow_v1_resume,
            workflow_v1_mark_plan_created,
            workflow_v1_fail,
            workflow_v1_get,
            workflow_v1_list,
            workflow_v1_list_waiting,
            // Image Analyzer
            image_v1_analyze,
            // Semantic Pipeline — Image Captioning & RAG
            agent_v1_analyze_asset,
            agent_v1_analyze_assets_batch,
            agent_v1_search_assets_semantic
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Read a file from disk and return it as a `data:<mime>;base64,...` URL.
/// Used by the frontend drag-drop handler to receive files dropped from the OS.
#[tauri::command]
fn file_read_as_data_url(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;
    let mime = mime_from_path(&path);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Open a file or URL with the system default application.
/// Bypasses the opener plugin's scope restrictions.
#[tauri::command]
fn open_file_with_system_viewer(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to open: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open: {e}"))?;
    }
    Ok(())
}

fn mime_from_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}
