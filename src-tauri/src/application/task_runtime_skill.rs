#![allow(dead_code)]
//! TaskRuntimeSkill：将 TaskRuntime 适配为 ExecutionSkill。
//!
//! 实现 Skill → Task 统一：Skill 产生 TaskRequest，通过 TaskRuntime 提交执行。
//! 这是 Phase 4 "Skill 统一" 的核心桥接。

use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    application::sequential_executor::ExecutionSkill,
    domain::{
        agent::PlanStepKind,
        creative_plan::AssetType,
        execution::{Artifact, ArtifactOrigin, Provenance},
        task::TaskKind,
    },
    ports::task_runtime::{TaskRequest, TaskRuntime},
};

/// 将 TaskRuntime 适配为 ExecutionSkill 的包装器。
///
/// 用于统一生成路径：Skill 层产生 TaskRequest → TaskRuntime 提交 → Provider 执行。
/// 与现有的 ImageGenerationSkill/VideoGenerationSkill 并行，作为统一路径的入口。
pub struct TaskRuntimeSkill {
    task_runtime: Arc<dyn TaskRuntime>,
    kind: PlanStepKind,
    task_kind: TaskKind,
    asset_type: AssetType,
    name: String,
}

impl TaskRuntimeSkill {
    /// 创建图片生成 TaskRuntimeSkill。
    pub fn image(task_runtime: Arc<dyn TaskRuntime>) -> Self {
        Self {
            task_runtime,
            kind: PlanStepKind::ImageGeneration,
            task_kind: TaskKind::ImageGeneration,
            asset_type: AssetType::Image,
            name: "TaskRuntimeImage".to_owned(),
        }
    }

    /// 创建视频生成 TaskRuntimeSkill。
    pub fn video(task_runtime: Arc<dyn TaskRuntime>) -> Self {
        Self {
            task_runtime,
            kind: PlanStepKind::VideoGeneration,
            task_kind: TaskKind::VideoGeneration,
            asset_type: AssetType::Video,
            name: "TaskRuntimeVideo".to_owned(),
        }
    }

    /// 创建自定义 TaskRuntimeSkill。
    pub fn custom(
        task_runtime: Arc<dyn TaskRuntime>,
        kind: PlanStepKind,
        task_kind: TaskKind,
        asset_type: AssetType,
        name: String,
    ) -> Self {
        Self {
            task_runtime,
            kind,
            task_kind,
            asset_type,
            name,
        }
    }
}

impl ExecutionSkill for TaskRuntimeSkill {
    fn name(&self) -> &str {
        &self.name
    }

    fn capability(&self) -> PlanStepKind {
        self.kind.clone()
    }

    fn execute(
        &self,
        step_index: u8,
        step_description: &str,
        _runtime: &crate::application::execution_runtime::ExecutionRuntime,
    ) -> Result<Artifact, String> {
        let request = TaskRequest {
            kind: self.task_kind.clone(),
            prompt: step_description.to_owned(),
            priority: crate::domain::task::TaskPriority::Normal,
            workspace_id: String::new(), // 由调用方填充
            conversation_id: None,
            parameters: HashMap::new(),
            max_retries: 2,
            timeout_secs: Some(300),
            sandbox: None,
        };

        let handle = self
            .task_runtime
            .submit(&request)
            .map_err(|e| format!("TaskRuntime submit failed: {e}"))?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "task_id".to_owned(),
            serde_json::Value::String(handle.task_id.clone()),
        );
        if let Some(provider) = &handle.provider_id {
            metadata.insert(
                "provider".to_owned(),
                serde_json::Value::String(provider.clone()),
            );
        }
        if let Some(model) = &handle.model_name {
            metadata.insert("model".to_owned(), serde_json::Value::String(model.clone()));
        }

        Ok(Artifact {
            id: format!(
                "task-{step_index:03}-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0")
            ),
            source_step: step_index,
            shot_index: step_index,
            asset_type: self.asset_type.clone(),
            location: format!("pending://task/{}", handle.task_id),
            origin: ArtifactOrigin::Generated,
            metadata,
            provenance: Some(Provenance::from_task_handle(
                &handle.task_id,
                handle.provider_id.as_deref(),
                handle.model_name.as_deref(),
                &self.name,
            )),
        })
    }
}
