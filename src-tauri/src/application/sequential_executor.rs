#![allow(dead_code)]
//! Sequential Executor：顺序执行引擎 + Skill Registry。
//!
//! 读取 ExecutionPlan 的 steps，通过 SkillRegistry 按 PlanStepKind 分发到对应 Skill，
//! 通过 ExecutionRuntime 报告结果。
//!
//! 职责边界：
//! - Executor 只负责：读取 Step → Registry.dispatch → 告诉 Runtime 结果
//! - Registry 负责：按 capability() 找到对应 Skill
//! - Skill 负责：执行具体能力（通过 GenerationFacade，不直接碰 Router/Provider）
//! - Runtime 负责：更新 Context / ArtifactStore / 推进 current_step

use std::sync::{Arc, Mutex};

use crate::{
    application::{
        creative_plan_builder::ExecutionPlan,
        execution_runtime::ExecutionRuntime,
        generation_facade::{GenerationFacade, GenerationRequest},
    },
    domain::{
        agent::PlanStepKind,
        creative_plan::AssetType,
        execution::{Artifact, ArtifactOrigin, ExecutionResult, Provenance},
        execution_persistence::{ExecutionStepDraft, WorkflowRunDraft},
    },
    ports::workflow_execution_repository::{
        ExecutionPersistenceEvent, WorkflowExecutionRepository,
    },
};

// ─── ExecutionSkill Trait ───

/// Skill 能力接口：执行单个步骤。
///
/// 每个 Skill 声明自己的 capability（PlanStepKind），Registry 据此自动映射。
/// Skill 内部通过 GenerationFacade 完成生成，不直接依赖 Router/Provider。
pub trait ExecutionSkill {
    /// Skill 名称（日志 + 调试用）。
    fn name(&self) -> &str;

    /// 该 Skill 的能力类型（Registry 据此注册和分发）。
    fn capability(&self) -> PlanStepKind;

    /// 是否支持该步骤类型（默认实现：匹配 capability）。
    fn supports(&self, kind: &PlanStepKind) -> bool {
        *kind == self.capability()
    }

    /// 执行步骤，返回产生的资产（或失败原因）。
    fn execute(
        &self,
        step_index: u8,
        step_description: &str,
        runtime: &ExecutionRuntime,
    ) -> Result<Artifact, String>;
}

// ─── Skill Registry ───

/// Skill 注册表：按 capability() 自动映射 PlanStepKind → Skill。
pub struct ExecutionSkillRegistry {
    skills: Vec<Box<dyn ExecutionSkill>>,
}

impl ExecutionSkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    /// 注册一个 Skill（自动按其 capability 映射）。
    pub fn register(&mut self, skill: impl ExecutionSkill + 'static) {
        self.skills.push(Box::new(skill));
    }

    /// 按 kind 查找支持的 Skill。
    pub fn dispatch(&self, kind: &PlanStepKind) -> Option<&dyn ExecutionSkill> {
        self.skills
            .iter()
            .find(|s| s.supports(kind))
            .map(|s| s.as_ref())
    }

    /// 创建包含所有 Mock Skill 的 Registry（测试用）。
    pub fn with_mocks() -> Self {
        let mut registry = Self::new();
        registry.register(MockImageSkill);
        registry.register(MockVideoSkill);
        registry.register(MockCompositeSkill);
        registry.register(MockStyleSkill);
        registry
    }

    /// 创建包含真实 Skill 的 Registry（使用指定 Facade）。
    ///
    /// Composite 通过 `detect_ffmpeg()` 条件注册：
    /// - ffmpeg 可用 → 注册真实 CompositeSkill（FfmpegCompositionFacade）
    /// - ffmpeg 不可用 → 保持 MockCompositeSkill，输出明确日志
    pub fn with_facade(facade: Arc<dyn GenerationFacade>) -> Self {
        let mut registry = Self::new();
        registry.register(ImageGenerationSkill::new(Arc::clone(&facade)));
        registry.register(VideoGenerationSkill::new(Arc::clone(&facade)));

        // Composite: 条件注册
        if let Some(ffmpeg_path) = super::composite_skill::detect_ffmpeg() {
            eprintln!("[SkillRegistry] CompositeSkill: engine=ffmpeg, path={ffmpeg_path}");
            // workspace_root 在真实环境中应从配置注入；当前使用默认路径
            let workspace_root =
                std::env::var("AIGC_WORKSPACE_ROOT").unwrap_or_else(|_| "managed-files".to_owned());
            let ffmpeg_facade: Arc<dyn super::composition_facade::CompositionFacade> = Arc::new(
                super::composition_facade::FfmpegCompositionFacade::new(ffmpeg_path),
            );
            registry.register(super::composite_skill::CompositeSkill::new(
                ffmpeg_facade,
                workspace_root,
            ));
        } else {
            eprintln!(
                "[SkillRegistry] CompositeSkill unavailable: \
                 reason=ffmpeg_not_found, fallback=MockCompositeSkill"
            );
            registry.register(MockCompositeSkill);
        }

        registry.register(MockStyleSkill);
        registry
    }
}

impl Default for ExecutionSkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Real Skills（通过 GenerationFacade）───

/// 图片生成 Skill：通过 GenerationFacade 生成图片。
pub struct ImageGenerationSkill {
    facade: Arc<dyn GenerationFacade>,
}

impl ImageGenerationSkill {
    pub fn new(facade: Arc<dyn GenerationFacade>) -> Self {
        Self { facade }
    }
}

impl ExecutionSkill for ImageGenerationSkill {
    fn name(&self) -> &str {
        "ImageGeneration"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::ImageGeneration
    }

    fn execute(
        &self,
        step_index: u8,
        step_description: &str,
        _runtime: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        let request = GenerationRequest {
            asset_type: AssetType::Image,
            prompt: step_description.to_owned(),
            style: None,
            reference_url: None,
            duration_secs: None,
            extra: std::collections::HashMap::new(),
        };

        let output = self.facade.submit(&request)?;

        let mut metadata = output.metadata;
        if let Some(sub_id) = &output.submission_id {
            metadata.insert(
                "submission_id".to_owned(),
                serde_json::Value::String(sub_id.clone()),
            );
        }
        metadata.insert(
            "provider".to_owned(),
            serde_json::Value::String(output.provider.clone()),
        );
        metadata.insert(
            "model".to_owned(),
            serde_json::Value::String(output.model.clone()),
        );
        metadata.insert(
            "state".to_owned(),
            serde_json::to_value(&output.state).unwrap_or_default(),
        );

        Ok(Artifact {
            id: format!(
                "img-{step_index:03}-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0")
            ),
            source_step: step_index,
            shot_index: step_index,
            asset_type: AssetType::Image,
            location: output.location,
            origin: output.origin,
            metadata,
            provenance: Some({
                let mut p = Provenance::skill_only("ImageGeneration");
                p.provider_id = Some(output.provider.clone());
                p.model_name = Some(output.model.clone());
                p.submission_id = output.submission_id.clone();
                p.step_index = Some(step_index);
                p
            }),
        })
    }
}

/// 视频生成 Skill：通过 GenerationFacade 生成视频。
pub struct VideoGenerationSkill {
    facade: Arc<dyn GenerationFacade>,
}

impl VideoGenerationSkill {
    pub fn new(facade: Arc<dyn GenerationFacade>) -> Self {
        Self { facade }
    }
}

impl ExecutionSkill for VideoGenerationSkill {
    fn name(&self) -> &str {
        "VideoGeneration"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::VideoGeneration
    }

    fn execute(
        &self,
        step_index: u8,
        step_description: &str,
        _runtime: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        let request = GenerationRequest {
            asset_type: AssetType::Video,
            prompt: step_description.to_owned(),
            style: None,
            reference_url: None,
            duration_secs: Some(5.0),
            extra: std::collections::HashMap::new(),
        };

        let output = self.facade.submit(&request)?;

        let mut metadata = output.metadata;
        if let Some(sub_id) = &output.submission_id {
            metadata.insert(
                "submission_id".to_owned(),
                serde_json::Value::String(sub_id.clone()),
            );
        }
        metadata.insert(
            "provider".to_owned(),
            serde_json::Value::String(output.provider.clone()),
        );
        metadata.insert(
            "model".to_owned(),
            serde_json::Value::String(output.model.clone()),
        );
        metadata.insert(
            "state".to_owned(),
            serde_json::to_value(&output.state).unwrap_or_default(),
        );

        Ok(Artifact {
            id: format!(
                "vid-{step_index:03}-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0")
            ),
            source_step: step_index,
            shot_index: step_index,
            asset_type: AssetType::Video,
            location: output.location,
            origin: output.origin,
            metadata,
            provenance: Some({
                let mut p = Provenance::skill_only("VideoGeneration");
                p.provider_id = Some(output.provider.clone());
                p.model_name = Some(output.model.clone());
                p.submission_id = output.submission_id.clone();
                p.step_index = Some(step_index);
                p
            }),
        })
    }
}

// ─── Mock Skills ───

/// Mock 图片生成 Skill（不经过 Facade，直接返回）。
pub struct MockImageSkill;

impl ExecutionSkill for MockImageSkill {
    fn name(&self) -> &str {
        "MockImageGeneration"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::ImageGeneration
    }

    fn execute(
        &self,
        step_index: u8,
        _desc: &str,
        _rt: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        Ok(Artifact {
            id: format!("mock-img-{step_index:03}"),
            source_step: step_index,
            shot_index: step_index,
            asset_type: AssetType::Image,
            location: format!("artifact://mock/image_{step_index:03}.png"),
            origin: ArtifactOrigin::Mock,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        })
    }
}

/// Mock 视频生成 Skill。
pub struct MockVideoSkill;

impl ExecutionSkill for MockVideoSkill {
    fn name(&self) -> &str {
        "MockVideoGeneration"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::VideoGeneration
    }

    fn execute(
        &self,
        step_index: u8,
        _desc: &str,
        _rt: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        Ok(Artifact {
            id: format!("mock-vid-{step_index:03}"),
            source_step: step_index,
            shot_index: step_index,
            asset_type: AssetType::Video,
            location: format!("artifact://mock/video_{step_index:03}.mp4"),
            origin: ArtifactOrigin::Mock,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        })
    }
}

/// Mock 合成 Skill。
pub struct MockCompositeSkill;

impl ExecutionSkill for MockCompositeSkill {
    fn name(&self) -> &str {
        "MockComposite"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::Composite
    }

    fn execute(
        &self,
        step_index: u8,
        _desc: &str,
        _rt: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        Ok(Artifact {
            id: format!("mock-final-{step_index:03}"),
            source_step: step_index,
            shot_index: 0,
            asset_type: AssetType::Video,
            location: "artifact://mock/final_output.mp4".to_owned(),
            origin: ArtifactOrigin::Mock,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        })
    }
}

/// Mock 风格设定 Skill。
pub struct MockStyleSkill;

impl ExecutionSkill for MockStyleSkill {
    fn name(&self) -> &str {
        "MockStyleSetup"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::StyleSetup
    }

    fn execute(
        &self,
        step_index: u8,
        _desc: &str,
        _rt: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        Ok(Artifact {
            id: format!("mock-style-{step_index:03}"),
            source_step: step_index,
            shot_index: 0,
            asset_type: AssetType::Text,
            location: "artifact://mock/style_config.json".to_owned(),
            origin: ArtifactOrigin::Mock,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        })
    }
}

// ─── Sequential Executor ───

/// 顺序执行器：通过 Registry 分发步骤到对应 Skill。
///
/// 可选持久化：通过 `with_persistence()` 注入 Repository，每步发射事件。
/// 持久化失败不阻断执行（"增强失败不影响主流程"）。
pub struct SequentialExecutor {
    registry: ExecutionSkillRegistry,
    persistence: Option<Arc<Mutex<dyn WorkflowExecutionRepository>>>,
}

impl SequentialExecutor {
    /// 创建执行器（注入 Registry）。
    pub fn new(registry: ExecutionSkillRegistry) -> Self {
        Self {
            registry,
            persistence: None,
        }
    }

    /// 注入持久化 Repository（可选）。
    pub fn with_persistence(mut self, repo: Arc<Mutex<dyn WorkflowExecutionRepository>>) -> Self {
        self.persistence = Some(repo);
        self
    }

    /// 创建使用全部 Mock Skill 的执行器（测试用）。
    pub fn with_mocks() -> Self {
        Self {
            registry: ExecutionSkillRegistry::with_mocks(),
            persistence: None,
        }
    }

    /// 创建使用真实 Skill + 指定 Facade 的执行器。
    pub fn with_facade(facade: Arc<dyn GenerationFacade>) -> Self {
        Self {
            registry: ExecutionSkillRegistry::with_facade(facade),
            persistence: None,
        }
    }

    /// 发射持久化事件。失败时仅 log，不阻断执行。
    fn emit_event(&self, event: ExecutionPersistenceEvent) {
        if let Some(repo) = &self.persistence {
            match repo.lock() {
                Ok(mut guard) => {
                    guard.consume_event(&event);
                }
                Err(e) => {
                    eprintln!("[Executor] persist lock failed: {e}");
                }
            }
        }
    }

    /// 执行整个计划。
    pub fn execute(
        &self,
        plan: &ExecutionPlan,
        plan_id: String,
        workspace_id: String,
    ) -> ExecutionResult {
        let mut runtime = ExecutionRuntime::new(plan_id.clone(), workspace_id.clone());

        // ── Run Started ──
        self.emit_event(ExecutionPersistenceEvent::RunStarted {
            draft: WorkflowRunDraft {
                workspace_id,
                plan_id: plan_id.clone(),
                total_steps: plan.steps.len() as u32,
            },
        });

        for step in &plan.steps {
            let step_index = step.index;

            runtime.begin_step(step_index);
            eprintln!(
                "[Executor] step {step_index} [{:?}]: {}",
                step.kind,
                truncate(&step.description, 60)
            );

            // ── Step Started ──
            self.emit_event(ExecutionPersistenceEvent::StepStarted {
                draft: ExecutionStepDraft {
                    run_id: plan_id.clone(),
                    step_index: step_index as u32,
                    kind: format!("{:?}", step.kind),
                    description: step.description.clone(),
                },
            });

            let skill = self.registry.dispatch(&step.kind);

            match skill {
                Some(skill) => {
                    eprintln!("[Executor] dispatched to: {}", skill.name());
                    match skill.execute(step_index, &step.description, &runtime) {
                        Ok(artifact) => {
                            eprintln!(
                                "[Executor] step {step_index} success: {}",
                                artifact.location
                            );
                            let artifact_id = artifact.id.clone();
                            runtime.complete_step(step_index, artifact);

                            // ── Step Completed ──
                            self.emit_event(ExecutionPersistenceEvent::StepCompleted {
                                step_id: format!("{step_index}"),
                                output_artifact_id: Some(artifact_id),
                            });
                        }
                        Err(reason) => {
                            eprintln!("[Executor] step {step_index} failed: {reason}");
                            runtime.fail_step(step_index, reason.clone());

                            // ── Step Failed ──
                            self.emit_event(ExecutionPersistenceEvent::StepFailed {
                                step_id: format!("{step_index}"),
                                error: reason,
                            });
                        }
                    }
                }
                None => {
                    eprintln!(
                        "[Executor] step {step_index}: no skill for {:?}, skipping",
                        step.kind
                    );
                    runtime.skip_step(step_index);

                    // ── Step Skipped ──
                    self.emit_event(ExecutionPersistenceEvent::StepSkipped {
                        step_id: format!("{step_index}"),
                    });
                }
            }
        }

        let result = runtime.finish();

        // ── Run Completed ──
        let status_str = match result.status {
            crate::domain::execution::ExecutionStatus::Completed => "completed",
            crate::domain::execution::ExecutionStatus::Partial => "partial",
            crate::domain::execution::ExecutionStatus::Failed => "failed",
            crate::domain::execution::ExecutionStatus::Cancelled => "cancelled",
        };
        self.emit_event(ExecutionPersistenceEvent::RunCompleted {
            run_id: plan_id,
            status: status_str.to_owned(),
            completed_steps: result.completed_shots.len() as u32,
            failed_steps: result.failed_shots.len() as u32,
        });

        eprintln!(
            "[Executor] done: {:?}, {} completed, {} failed, {:.2}s",
            result.status,
            result.completed_shots.len(),
            result.failed_shots.len(),
            result.duration_secs
        );
        result
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::generation_facade::MockGenerationFacade;
    use crate::domain::agent::{PlanStep, PlanStepKind, PlanStepStatus};

    fn sample_execution_plan() -> ExecutionPlan {
        ExecutionPlan {
            steps: vec![
                PlanStep {
                    index: 1,
                    description: "确定创作风格基调：赛博朋克".to_owned(),
                    status: PlanStepStatus::Pending,
                    kind: PlanStepKind::StyleSetup,
                },
                PlanStep {
                    index: 2,
                    description: "镜头1 [Opening]：生成视频（5秒）— 城市夜景".to_owned(),
                    status: PlanStepStatus::Pending,
                    kind: PlanStepKind::VideoGeneration,
                },
                PlanStep {
                    index: 3,
                    description: "镜头2 [Emotion]：生成图片（8秒）— 主角特写".to_owned(),
                    status: PlanStepStatus::Pending,
                    kind: PlanStepKind::ImageGeneration,
                },
                PlanStep {
                    index: 4,
                    description: "将2个视觉素材按顺序合成为完整作品".to_owned(),
                    status: PlanStepStatus::Pending,
                    kind: PlanStepKind::Composite,
                },
            ],
        }
    }

    #[test]
    fn test_registry_dispatch_by_capability() {
        let registry = ExecutionSkillRegistry::with_mocks();

        assert!(registry.dispatch(&PlanStepKind::ImageGeneration).is_some());
        assert!(registry.dispatch(&PlanStepKind::VideoGeneration).is_some());
        assert!(registry.dispatch(&PlanStepKind::Composite).is_some());
        assert!(registry.dispatch(&PlanStepKind::StyleSetup).is_some());
        assert!(registry.dispatch(&PlanStepKind::Task).is_none());
    }

    #[test]
    fn test_sequential_executor_mock_all_success() {
        let executor = SequentialExecutor::with_mocks();
        let plan = sample_execution_plan();

        let result = executor.execute(&plan, "plan-001".to_owned(), "ws-1".to_owned());

        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );
        assert_eq!(result.completed_shots.len(), 4);
        assert_eq!(result.failed_shots.len(), 0);
        assert_eq!(result.output_artifacts.len(), 4);
    }

    #[test]
    fn test_sequential_executor_with_facade() {
        let facade: Arc<dyn GenerationFacade> = Arc::new(MockGenerationFacade);
        let executor = SequentialExecutor::with_facade(facade);
        let plan = sample_execution_plan();

        let result = executor.execute(&plan, "plan-facade".to_owned(), "ws-1".to_owned());

        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );
        assert_eq!(result.completed_shots.len(), 4);
        // Image 和 Video 通过 Facade submit，location 应为 pending://submission/...
        let img_artifact = result
            .output_artifacts
            .iter()
            .find(|a| a.asset_type == AssetType::Image);
        assert!(img_artifact.is_some());
        assert!(img_artifact
            .unwrap()
            .location
            .starts_with("pending://submission/"));
        // 验证 metadata 包含 provider 和 model 信息
        assert!(img_artifact.unwrap().metadata.contains_key("provider"));
        assert!(img_artifact.unwrap().metadata.contains_key("model"));
        // 验证 origin 为 Mock（MockGenerationFacade）
        assert_eq!(img_artifact.unwrap().origin, ArtifactOrigin::Mock);
    }

    /// 集成测试：CreativePlan → PlanBuilder → Executor → Facade → ExecutionResult。
    #[test]
    fn test_integration_creative_plan_to_execution() {
        use crate::application::creative_director_service::CreativeBrief;
        use crate::application::creative_plan_builder::build_execution_plan;
        use crate::domain::creative_plan::{CreativePlan, PlanningMetadata, ShotPlan};

        // 1. 构造 CreativePlan（模拟 Planner 输出）
        let creative_plan = CreativePlan {
            id: "cp-001".to_owned(),
            version: 1,
            workspace_id: "ws-integration".to_owned(),
            brief: CreativeBrief {
                project_type: "video".to_owned(),
                audience: "年轻人".to_owned(),
                platform: "douyin".to_owned(),
                visual_direction: "赛博朋克".to_owned(),
                strategy: "快节奏".to_owned(),
                suggested_style: None,
            },
            global_style: "赛博朋克，霓虹色调".to_owned(),
            constraints: vec!["30秒内".to_owned()],
            shots: vec![
                ShotPlan {
                    index: 1,
                    goal: "Opening".to_owned(),
                    description: "城市夜景俯瞰".to_owned(),
                    duration_secs: 5.0,
                    asset_type: AssetType::Video,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "fade".to_owned(),
                },
                ShotPlan {
                    index: 2,
                    goal: "Emotion".to_owned(),
                    description: "主角行走在霓虹街道".to_owned(),
                    duration_secs: 8.0,
                    asset_type: AssetType::Image,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "cut".to_owned(),
                },
            ],
            metadata: PlanningMetadata {
                planner_version: "1.0.0".to_owned(),
                llm_model: String::new(),
                confidence: 0.9,
            },
            created_at: "2026-07-23T00:00:00Z".to_owned(),
        };

        // 2. PlanBuilder → ExecutionPlan
        let exec_plan = build_execution_plan(&creative_plan);
        // style + 2 shots + composite = 4 steps
        assert_eq!(exec_plan.steps.len(), 4);

        // 3. Executor with Facade
        let facade: Arc<dyn GenerationFacade> = Arc::new(MockGenerationFacade);
        let executor = SequentialExecutor::with_facade(facade);

        // 4. Execute
        let result = executor.execute(
            &exec_plan,
            "plan-integration".to_owned(),
            "ws-integration".to_owned(),
        );

        // 5. Verify
        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );
        assert_eq!(result.completed_shots.len(), 4);
        assert_eq!(result.failed_shots.len(), 0);
        assert_eq!(result.output_artifacts.len(), 4);
        assert_eq!(result.plan_id, "plan-integration");
        assert!(result.duration_secs >= 0.0);
    }

    #[test]
    fn test_sequential_executor_unsupported_kind_skipped() {
        let executor = SequentialExecutor::with_mocks();
        let plan = ExecutionPlan {
            steps: vec![PlanStep {
                index: 1,
                description: "通用任务".to_owned(),
                status: PlanStepStatus::Pending,
                kind: PlanStepKind::Task,
            }],
        };

        let result = executor.execute(&plan, "plan-002".to_owned(), "ws-1".to_owned());
        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );
        assert_eq!(result.output_artifacts.len(), 0);
    }

    // ─── Fake Repository for persistence testing ───

    use crate::domain::execution_persistence::{
        ExecutionStepRecord, GenerationSubmissionRecord, WorkflowRunRecord,
    };
    use crate::ports::workflow_execution_repository::WorkflowExecutionRepositoryError;

    /// 内存 Fake Repository：记录事件供测试断言。
    struct FakeWorkflowExecutionRepository {
        events: Vec<ExecutionPersistenceEvent>,
    }

    impl FakeWorkflowExecutionRepository {
        fn new() -> Self {
            Self { events: Vec::new() }
        }

        fn event_count(&self) -> usize {
            self.events.len()
        }

        fn has_run_started(&self) -> bool {
            self.events
                .iter()
                .any(|e| matches!(e, ExecutionPersistenceEvent::RunStarted { .. }))
        }

        fn has_run_completed(&self) -> bool {
            self.events
                .iter()
                .any(|e| matches!(e, ExecutionPersistenceEvent::RunCompleted { .. }))
        }

        fn step_started_count(&self) -> usize {
            self.events
                .iter()
                .filter(|e| matches!(e, ExecutionPersistenceEvent::StepStarted { .. }))
                .count()
        }

        fn step_completed_count(&self) -> usize {
            self.events
                .iter()
                .filter(|e| matches!(e, ExecutionPersistenceEvent::StepCompleted { .. }))
                .count()
        }

        fn step_skipped_count(&self) -> usize {
            self.events
                .iter()
                .filter(|e| matches!(e, ExecutionPersistenceEvent::StepSkipped { .. }))
                .count()
        }
    }

    impl WorkflowExecutionRepository for FakeWorkflowExecutionRepository {
        fn create_run(
            &mut self,
            _draft: WorkflowRunDraft,
        ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::RunNotFound(
                "fake".to_owned(),
            ))
        }
        fn update_run_status(
            &mut self,
            _id: &str,
            _status: &str,
            _completed: u32,
            _failed: u32,
        ) -> Result<WorkflowRunRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::RunNotFound(
                "fake".to_owned(),
            ))
        }
        fn get_run(
            &mut self,
            _id: &str,
        ) -> Result<Option<WorkflowRunRecord>, WorkflowExecutionRepositoryError> {
            Ok(None)
        }
        fn list_runs_by_workspace(
            &mut self,
            _ws: &str,
        ) -> Result<Vec<WorkflowRunRecord>, WorkflowExecutionRepositoryError> {
            Ok(Vec::new())
        }
        fn create_step(
            &mut self,
            _draft: ExecutionStepDraft,
        ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::StepNotFound(
                "fake".to_owned(),
            ))
        }
        fn update_step_status(
            &mut self,
            _id: &str,
            _status: &str,
            _artifact: Option<&str>,
            _error: Option<&str>,
        ) -> Result<ExecutionStepRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::StepNotFound(
                "fake".to_owned(),
            ))
        }
        fn list_steps_by_run(
            &mut self,
            _run_id: &str,
        ) -> Result<Vec<ExecutionStepRecord>, WorkflowExecutionRepositoryError> {
            Ok(Vec::new())
        }
        fn create_submission(
            &mut self,
            _draft: crate::domain::execution_persistence::GenerationSubmissionDraft,
        ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::SubmissionNotFound(
                "fake".to_owned(),
            ))
        }
        fn update_submission_status(
            &mut self,
            _id: &str,
            _status: &str,
            _asset: Option<&str>,
        ) -> Result<GenerationSubmissionRecord, WorkflowExecutionRepositoryError> {
            Err(WorkflowExecutionRepositoryError::SubmissionNotFound(
                "fake".to_owned(),
            ))
        }
        fn list_submissions_by_step(
            &mut self,
            _step_id: &str,
        ) -> Result<Vec<GenerationSubmissionRecord>, WorkflowExecutionRepositoryError> {
            Ok(Vec::new())
        }

        /// Override consume_event to record events instead of calling CRUD.
        fn consume_event(&mut self, event: &ExecutionPersistenceEvent) {
            self.events.push(event.clone());
        }
    }

    #[test]
    fn test_executor_with_persistence_emits_events() {
        let fake_repo = Arc::new(Mutex::new(FakeWorkflowExecutionRepository::new()));
        let executor = SequentialExecutor::with_mocks().with_persistence(fake_repo.clone());
        let plan = sample_execution_plan();

        let result = executor.execute(&plan, "plan-persist".to_owned(), "ws-1".to_owned());

        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );

        // Verify persistence events
        let repo = fake_repo.lock().unwrap();
        assert!(repo.has_run_started(), "RunStarted event missing");
        assert!(repo.has_run_completed(), "RunCompleted event missing");
        assert_eq!(
            repo.step_started_count(),
            4,
            "Expected 4 StepStarted events"
        );
        assert_eq!(
            repo.step_completed_count(),
            4,
            "Expected 4 StepCompleted events"
        );
        assert_eq!(
            repo.step_skipped_count(),
            0,
            "Expected 0 StepSkipped events"
        );
    }

    #[test]
    fn test_executor_without_persistence_still_works() {
        // No persistence injected — should work exactly as before
        let executor = SequentialExecutor::with_mocks();
        let plan = sample_execution_plan();

        let result = executor.execute(&plan, "plan-no-persist".to_owned(), "ws-1".to_owned());

        assert_eq!(
            result.status,
            crate::domain::execution::ExecutionStatus::Completed
        );
        assert_eq!(result.completed_shots.len(), 4);
    }
}
