#![allow(dead_code)]
//! Generation Facade：生成能力的统一入口。
//!
//! Skill 只通过 Facade 表达"我要生成 X 类型的资产"，
//! Facade 内部负责：Router 选模型 → Provider 提交 → （未来）Poll → Download。
//! Skill 永远不知道 Router / Provider / PollWorker 的存在。
//!
//! 职责链位置：
//! Skill → GenerationFacade → Router → Provider → (未来) PollWorker → Download
//!
//! Step 3C-1：真实 Submission Runtime
//! - submit()：Router 选模型 → Provider.submit() → 返回 submission_id
//! - poll() / download()：预定义接口，Step 3C-2/3C-3 实现

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::creative_plan::AssetType;
use crate::domain::execution::ArtifactOrigin;

// ─── Generation Lifecycle Enums ───

/// 生成任务生命周期状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationState {
    /// 已提交到远端 Provider。
    Submitted,
    /// 远端处理中。
    Running,
    /// 完成，可下载。
    Completed,
    /// 失败。
    Failed,
}

// ─── Request / Output ───

/// 生成请求（Skill → Facade 的通信协议）。
#[derive(Debug, Clone)]
pub struct GenerationRequest {
    /// 资产类型。
    pub asset_type: AssetType,
    /// 生成提示词。
    pub prompt: String,
    /// 风格约束（可选）。
    pub style: Option<String>,
    /// 参考图 URL（可选，用于 image-to-video 等）。
    pub reference_url: Option<String>,
    /// 时长（秒，视频/音频用）。
    pub duration_secs: Option<f32>,
    /// 附加参数。
    pub extra: HashMap<String, serde_json::Value>,
}

/// 生成结果（Facade → Skill 的返回）。
#[derive(Debug, Clone)]
pub struct GenerationOutput {
    /// 当前生命周期状态。
    pub state: GenerationState,
    /// 资产来源。
    pub origin: ArtifactOrigin,
    /// 资产存储位置。
    /// - Submitted 时为 "pending://submission/{submission_id}"
    /// - Completed/Downloaded 时为本地路径
    pub location: String,
    /// 提交 ID（异步生成时用于追踪）。
    pub submission_id: Option<String>,
    /// 使用的 provider 标识。
    pub provider: String,
    /// Router 选中的模型名。
    pub model: String,
    /// 附加元数据。
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── GenerationFacade Trait ───

/// 生成 Facade trait。
///
/// Skill 通过此接口与生成基础设施交互。
/// - submit()：异步提交（主入口），返回 submission_id
/// - poll()：查询状态（Step 3C-2）
/// - download()：下载到本地（Step 3C-3）
pub trait GenerationFacade: Send + Sync {
    /// 异步提交生成请求。返回 submission_id，state = Submitted。
    ///
    /// 内部流程：Router 选模型 → Provider.submit() → 返回远端 job ID。
    fn submit(&self, request: &GenerationRequest) -> Result<GenerationOutput, String>;

    /// 查询提交状态（Step 3C-2 实现）。
    fn poll(&self, submission_id: &str) -> Result<GenerationState, String> {
        let _ = submission_id;
        Err("poll not implemented (Step 3C-2)".to_owned())
    }

    /// 下载已完成的任务到本地（Step 3C-3 实现）。
    fn download(&self, submission_id: &str, target_dir: &str) -> Result<String, String> {
        let _ = (submission_id, target_dir);
        Err("download not implemented (Step 3C-3)".to_owned())
    }
}

// ─── Mock Facade ───

/// Mock Facade：立即返回假提交结果，用于验证调用链。
pub struct MockGenerationFacade;

impl GenerationFacade for MockGenerationFacade {
    fn submit(&self, request: &GenerationRequest) -> Result<GenerationOutput, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let submission_id = format!("sub-{}", &id[..8]);
        let location = format!("pending://submission/{submission_id}");

        eprintln!(
            "[Facade] mock submit: {:?}, prompt={}...",
            request.asset_type,
            request.prompt.chars().take(30).collect::<String>()
        );

        Ok(GenerationOutput {
            state: GenerationState::Submitted,
            origin: ArtifactOrigin::Mock,
            location,
            submission_id: Some(submission_id),
            provider: "mock-provider".to_owned(),
            model: "mock-model".to_owned(),
            metadata: HashMap::new(),
        })
    }
}

// ─── Real Facade ───

/// 真实 Generation Facade：Router 选模型 → Provider 提交。
///
/// 依赖：
/// - ModelRouterService：按任务类型 + 策略选择最优模型
/// - ProviderRegistry：按 provider_id 解析 Provider 适配器
///
/// Step 3C-1 只实现 submit()；poll() / download() 在后续步骤实现。
pub struct RealGenerationFacade {
    router: Arc<crate::application::model_router_service::ModelRouterService>,
    provider_registry: Arc<crate::application::provider_registry::ProviderRegistry>,
}

impl RealGenerationFacade {
    pub fn new(
        router: Arc<crate::application::model_router_service::ModelRouterService>,
        provider_registry: Arc<crate::application::provider_registry::ProviderRegistry>,
    ) -> Self {
        Self {
            router,
            provider_registry,
        }
    }

    /// 将 AssetType 映射为 Router 的 RoutingTaskType。
    fn routing_task_type(asset_type: &AssetType) -> crate::domain::model_router::RoutingTaskType {
        use crate::domain::model_router::RoutingTaskType;
        match asset_type {
            AssetType::Image => RoutingTaskType::ImageGeneration,
            AssetType::Video => RoutingTaskType::VideoGeneration,
            AssetType::Audio => RoutingTaskType::TextToSpeech,
            AssetType::Text => RoutingTaskType::TextGeneration,
        }
    }

    /// 将 AssetType 映射为 Provider 的 CapabilityKind。
    fn capability_kind(
        asset_type: &AssetType,
        has_reference: bool,
    ) -> crate::ports::unified_provider::CapabilityKind {
        use crate::ports::unified_provider::CapabilityKind;
        match asset_type {
            AssetType::Image => {
                if has_reference {
                    CapabilityKind::ImageToImage
                } else {
                    CapabilityKind::TextToImage
                }
            }
            AssetType::Video => {
                if has_reference {
                    CapabilityKind::ImageToVideo
                } else {
                    CapabilityKind::TextToVideo
                }
            }
            AssetType::Audio => CapabilityKind::TextToSpeech,
            AssetType::Text => CapabilityKind::TextToImage, // fallback, text 暂不走 Provider
        }
    }
}

impl GenerationFacade for RealGenerationFacade {
    fn submit(&self, request: &GenerationRequest) -> Result<GenerationOutput, String> {
        use crate::domain::model_router::{RoutingRequest, RoutingStrategy};
        use crate::ports::unified_provider::UnifiedRequest;

        // 1. Router 选模型
        let task_type = Self::routing_task_type(&request.asset_type);
        let routing_request = RoutingRequest {
            task_type,
            strategy: RoutingStrategy::Balanced,
            preferred_provider: None,
            preferred_model: None,
            budget_limit: None,
            requires_reference: request.reference_url.is_some(),
            context: Some(format!(
                "Creative Runtime auto-submit: {:?}",
                request.asset_type
            )),
        };

        let decision = self
            .router
            .route(&routing_request)
            .map_err(|e| format!("[Facade] Router failed: {e}"))?;

        let selected = &decision.selected;
        eprintln!(
            "[Facade] Router selected: {} ({}) for {:?}",
            selected.display_name, selected.provider_id, request.asset_type
        );

        // 2. 从 ProviderRegistry 解析 Provider + Credential
        let resolved = self
            .provider_registry
            .resolve(&selected.provider_id)
            .ok_or_else(|| {
                format!(
                    "[Facade] Provider '{}' not found in registry",
                    selected.provider_id
                )
            })?;
        let provider = &resolved.adapter;
        let credential = &resolved.credential;

        // 3. 构建 UnifiedRequest
        let capability =
            Self::capability_kind(&request.asset_type, request.reference_url.is_some());
        let mut parameters: HashMap<String, serde_json::Value> = HashMap::new();

        if let Some(duration) = request.duration_secs {
            parameters.insert(
                "duration".to_owned(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(duration as f64)
                        .unwrap_or(serde_json::Number::from(5)),
                ),
            );
        }
        if let Some(style) = &request.style {
            parameters.insert("style".to_owned(), serde_json::Value::String(style.clone()));
        }
        // 合并 extra 参数
        for (k, v) in &request.extra {
            parameters.insert(k.clone(), v.clone());
        }

        let unified_request = UnifiedRequest {
            capability,
            model: selected.model_name.clone(),
            prompt: request.prompt.clone(),
            negative_prompt: None,
            reference_image_path: None,
            reference_image_url: request.reference_url.clone(),
            parameters,
        };

        // 4. Provider.submit()
        let submit_result = provider
            .submit(&unified_request, credential)
            .map_err(|e| format!("[Facade] Provider submit failed: {e}"))?;

        let submission_id = submit_result.remote_job_id.clone();
        let location = format!("pending://submission/{submission_id}");

        eprintln!(
            "[Facade] Submitted to {} ({}): job_id={}",
            selected.provider_id, selected.model_name, submission_id
        );

        // 5. 同步 Provider 直接返回结果的情况（如 Grok 图片）
        let state = if submit_result.immediate_result_url.is_some() {
            GenerationState::Completed
        } else {
            GenerationState::Submitted
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "routing_reason".to_owned(),
            serde_json::Value::String(decision.reason.clone()),
        );
        if let Some(url) = &submit_result.immediate_result_url {
            metadata.insert(
                "immediate_result_url".to_owned(),
                serde_json::Value::String(url.clone()),
            );
        }
        if let Some(est) = submit_result.estimated_duration_secs {
            metadata.insert(
                "estimated_duration_secs".to_owned(),
                serde_json::Value::Number(serde_json::Number::from(est)),
            );
        }

        Ok(GenerationOutput {
            state,
            origin: ArtifactOrigin::Generated,
            location,
            submission_id: Some(submission_id),
            provider: selected.provider_id.clone(),
            model: selected.model_name.clone(),
            metadata,
        })
    }
}
