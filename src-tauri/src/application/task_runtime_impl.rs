#![allow(dead_code)]
//! Generation Task Runtime：TaskRuntime 的生成任务实现。
//!
//! 包装 ProviderRegistry + GenerationRepository + ModelRouterService，
//! 将生成提交统一到 TaskRuntime trait 下。
//!
//! 这是 "Phase 2: 生成系统 Task 化" 的核心实现。
//! 与 BuiltinToolExecutor 中 execute_generation() 的逻辑对齐，
//! 但抽离为可复用的 TaskRuntime。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::application::model_router_service::ModelRouterService;
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::generation::{GenerationStatus, GenerationTaskDraft};
use crate::domain::model_router::{RoutingRequest, RoutingStrategy, RoutingTaskType};
use crate::domain::task::{TaskKind, TaskStatus};
use crate::ports::generation_repository::GenerationRepository;
use crate::ports::task_runtime::{TaskError, TaskHandle, TaskRequest, TaskRuntime};
use crate::ports::unified_provider::{CapabilityKind, UnifiedRequest};

/// 生成任务运行时：TaskRuntime 的生成任务实现。
///
/// 将 TaskRequest 映射到 ProviderRegistry → Provider.submit() → GenerationRepository。
/// 共享 BuiltinToolExecutor 的 ProviderRegistry 实例。
pub struct GenerationTaskRuntime {
    provider_registry: Arc<ProviderRegistry>,
    generation_repository: Arc<Mutex<Box<dyn GenerationRepository>>>,
    model_router: Option<ModelRouterService>,
}

impl GenerationTaskRuntime {
    pub fn new(
        provider_registry: Arc<ProviderRegistry>,
        generation_repository: impl GenerationRepository + 'static,
    ) -> Self {
        Self {
            provider_registry,
            generation_repository: Arc::new(Mutex::new(Box::new(generation_repository))),
            model_router: None,
        }
    }

    /// 注入模型路由器。
    pub fn with_model_router(mut self, router: ModelRouterService) -> Self {
        self.model_router = Some(router);
        self
    }

    /// 将 TaskKind 映射为 RoutingTaskType。
    fn routing_task_type(kind: TaskKind) -> RoutingTaskType {
        match kind {
            TaskKind::ImageGeneration => RoutingTaskType::ImageGeneration,
            TaskKind::VideoGeneration => RoutingTaskType::VideoGeneration,
            TaskKind::AudioGeneration => RoutingTaskType::TextToSpeech,
            _ => RoutingTaskType::ImageGeneration,
        }
    }

    /// 将 TaskKind 映射为 CapabilityKind。
    fn capability_kind(kind: TaskKind, has_reference: bool) -> CapabilityKind {
        match kind {
            TaskKind::ImageGeneration => {
                if has_reference {
                    CapabilityKind::ImageToImage
                } else {
                    CapabilityKind::TextToImage
                }
            }
            TaskKind::VideoGeneration => {
                if has_reference {
                    CapabilityKind::ImageToVideo
                } else {
                    CapabilityKind::TextToVideo
                }
            }
            TaskKind::AudioGeneration => CapabilityKind::TextToSpeech,
            _ => CapabilityKind::TextToImage,
        }
    }

    /// 将 GenerationStatus 映射为 TaskStatus。
    fn map_status(status: GenerationStatus) -> TaskStatus {
        match status {
            GenerationStatus::Pending => TaskStatus::Pending,
            GenerationStatus::Running => TaskStatus::Running,
            GenerationStatus::Succeeded => TaskStatus::Completed,
            GenerationStatus::Failed => TaskStatus::Failed,
        }
    }

    /// 通过 ModelRouter 选择最优 Provider。
    fn route_provider(&self, request: &TaskRequest) -> Option<(String, String)> {
        let router = self.model_router.as_ref()?;

        let routing_request = RoutingRequest {
            task_type: Self::routing_task_type(request.kind),
            strategy: RoutingStrategy::Balanced,
            preferred_provider: request
                .parameters
                .get("provider")
                .or_else(|| request.parameters.get("providerName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned()),
            preferred_model: request
                .parameters
                .get("model")
                .or_else(|| request.parameters.get("modelName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned()),
            budget_limit: None,
            requires_reference: request.parameters.contains_key("reference_image_url"),
            context: Some(format!("TaskRuntime: {:?}", request.kind)),
        };

        router.route(&routing_request).ok().map(|decision| {
            (
                decision.selected.provider_id.clone(),
                decision.selected.model_name.clone(),
            )
        })
    }
}

impl TaskRuntime for GenerationTaskRuntime {
    fn submit(&self, request: &TaskRequest) -> Result<TaskHandle, TaskError> {
        // 只处理生成类型任务
        match request.kind {
            TaskKind::ImageGeneration | TaskKind::VideoGeneration | TaskKind::AudioGeneration => {}
            _ => {
                return Err(TaskError::UnknownKind(format!(
                    "{:?} is not a generation task",
                    request.kind
                )));
            }
        }

        // 路由选择 Provider
        let (provider_id, model_name) = self
            .route_provider(request)
            .ok_or(TaskError::NoProvider(request.kind))?;

        // 从 Registry 解析 Provider
        let resolved = self
            .provider_registry
            .resolve(&provider_id)
            .or_else(|| self.provider_registry.resolve_by_alias(&provider_id))
            .ok_or(TaskError::NoProvider(request.kind))?;

        // 创建 GenerationTask 记录
        let task_draft = GenerationTaskDraft {
            workspace_id: request.workspace_id.clone(),
            provider_name: provider_id.clone(),
            model_name: model_name.clone(),
            prompt_text: request.prompt.clone(),
        };

        let task_record = {
            let mut repo = self
                .generation_repository
                .lock()
                .map_err(|_| TaskError::Persistence("lock failed".to_owned()))?;
            repo.create_task(task_draft)
                .map_err(|e| TaskError::Persistence(e.to_string()))?
        };

        // 构建 UnifiedRequest
        let has_reference = request.parameters.contains_key("reference_image_url");
        let capability = Self::capability_kind(request.kind, has_reference);
        let mut parameters: HashMap<String, serde_json::Value> = HashMap::new();
        for (k, v) in &request.parameters {
            if k != "provider" && k != "providerName" && k != "model" && k != "modelName" {
                parameters.insert(k.clone(), v.clone());
            }
        }

        let unified_request = UnifiedRequest {
            capability,
            model: model_name.clone(),
            prompt: request.prompt.clone(),
            negative_prompt: request
                .parameters
                .get("negative_prompt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned()),
            reference_image_path: None,
            reference_image_url: request
                .parameters
                .get("reference_image_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned()),
            parameters,
        };

        // 提交到 Provider
        let submit_result = resolved
            .adapter
            .submit(&unified_request, &resolved.credential)
            .map_err(|e| TaskError::SubmissionFailed(e.to_string()))?;

        let remote_job_id = submit_result.remote_job_id.clone();
        let status = if submit_result.immediate_result_url.is_some() {
            TaskStatus::Completed
        } else {
            TaskStatus::Running
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "generation_task_id".to_owned(),
            serde_json::Value::String(task_record.id.clone()),
        );
        if let Some(url) = &submit_result.immediate_result_url {
            metadata.insert(
                "immediate_result_url".to_owned(),
                serde_json::Value::String(url.clone()),
            );
        }

        Ok(TaskHandle {
            task_id: task_record.id,
            status,
            provider_id: Some(provider_id),
            model_name: Some(model_name),
            remote_job_id: Some(remote_job_id),
            metadata,
        })
    }

    fn status(&self, task_id: &str) -> Result<TaskStatus, TaskError> {
        let mut repo = self
            .generation_repository
            .lock()
            .map_err(|_| TaskError::Persistence("lock failed".to_owned()))?;

        let task = repo
            .get_task(task_id)
            .map_err(|e| TaskError::Persistence(e.to_string()))?
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;

        Ok(Self::map_status(task.status))
    }

    fn cancel(&self, task_id: &str) -> Result<bool, TaskError> {
        // Generation tasks don't support mid-flight cancellation natively
        // (once submitted to Provider, it runs to completion).
        // We can mark the task as failed locally.
        let mut _repo = self
            .generation_repository
            .lock()
            .map_err(|_| TaskError::Persistence("lock failed".to_owned()))?;

        // Return false to indicate the task can't be truly cancelled
        // (it may already be processing on the Provider side)
        Err(TaskError::NotCancellable {
            task_id: task_id.to_owned(),
            status: TaskStatus::Running,
        })
    }
}
