#![allow(dead_code)]
//! Execution Runtime：执行运行时数据模型。
//!
//! 纯 Struct 定义，不含任何执行逻辑（Router / Skill / Provider 在 Step 3B 接入）。
//! 回答三个问题：当前正在执行什么？已经产生了什么？下一步可以使用什么？
//!
//! 职责链位置：
//! ExecutionPlan（静态）→ ExecutionContext（运行时）→ ExecutionResult（最终结果）

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::domain::creative_plan::AssetType;

// ─── ArtifactOrigin ───

/// 资产来源（领域概念：描述资产从何而来）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOrigin {
    /// 远端 AI 生成（已提交，尚未下载）。
    Generated,
    /// 已下载到本地。
    Downloaded,
    /// 多素材本地合成（ffmpeg 等工具产出，非 AI 生成）。
    Composed,
    /// 用户导入。
    Imported,
    /// 测试用 Mock。
    Mock,
}

impl Default for ArtifactOrigin {
    fn default() -> Self {
        Self::Generated
    }
}

// ─── Provenance ───

/// 资产生产链路追踪。
///
/// 记录一个 Artifact 是由哪个 Task、哪个 Provider、哪个 Skill、在哪一步产出的。
/// 从 Artifact 沿 provenance 回溯，可以重建完整的生产链路。
///
/// 设计原则：
/// - 所有字段 Optional（旧数据/导入数据可能缺少部分信息）
/// - Serialize 以便持久化到 metadata 或独立表
/// - Clone 以便在 ExecutionContext 中传递
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// 产出此资产的 Task ID（对应 TaskRecord.task_id）。
    pub task_id: Option<String>,
    /// AI Provider 标识（如 "jimeng", "openai", "replicate"）。
    pub provider_id: Option<String>,
    /// Router 选中的模型名（如 "jimeng-2.1", "dall-e-3"）。
    pub model_name: Option<String>,
    /// 执行此步骤的 Skill 名称（如 "TaskRuntimeImage", "CompositeSkill"）。
    pub skill_name: Option<String>,
    /// 执行步骤索引（对应 ExecutionPlan.steps 下标，0-based）。
    pub step_index: Option<u8>,
    /// 关联的会话 ID（Agent 对话中产出时）。
    pub conversation_id: Option<String>,
    /// 关联的 Submission ID（Creative Runtime Facade 提交时）。
    pub submission_id: Option<String>,
    /// Task 提交时间（RFC 3339）。
    pub submitted_at: Option<String>,
    /// Task 完成时间（RFC 3339）。
    pub completed_at: Option<String>,
    /// 执行耗时（毫秒）。
    pub duration_ms: Option<u64>,
}

impl Provenance {
    /// 从 TaskRuntime 返回的 TaskHandle 创建 Provenance。
    pub fn from_task_handle(
        task_id: &str,
        provider_id: Option<&str>,
        model_name: Option<&str>,
        skill_name: &str,
    ) -> Self {
        Self {
            task_id: Some(task_id.to_owned()),
            provider_id: provider_id.map(|s| s.to_owned()),
            model_name: model_name.map(|s| s.to_owned()),
            skill_name: Some(skill_name.to_owned()),
            ..Default::default()
        }
    }

    /// 快速创建：仅标注 skill。
    pub fn skill_only(skill_name: &str) -> Self {
        Self {
            skill_name: Some(skill_name.to_owned()),
            ..Default::default()
        }
    }

    /// 是否有完整 Provider 信息。
    pub fn has_provider_info(&self) -> bool {
        self.provider_id.is_some() && self.model_name.is_some()
    }

    /// 是否有完整时间链路。
    pub fn has_timing(&self) -> bool {
        self.submitted_at.is_some() && self.completed_at.is_some()
    }
}

// ─── Artifact ───

/// 执行过程中产生的资产（图片、视频、音频、文案等）。
///
/// 与 CreativePlan 的声明式 ShotPlan 对应，但包含运行态信息（来源步骤、存储位置）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    /// 资产唯一 ID。
    pub id: String,
    /// 产生该资产的执行步骤索引。
    pub source_step: u8,
    /// 关联的镜头索引（对应 ShotPlan.index）。
    pub shot_index: u8,
    /// 资产类型。
    pub asset_type: AssetType,
    /// 存储位置（本地路径或 URL）。
    pub location: String,
    /// 资产来源（Generated / Downloaded / Composed / Imported / Mock）。
    #[serde(default)]
    pub origin: ArtifactOrigin,
    /// 附加元数据（尺寸、时长、SHA256 等）。
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// 生产链路追踪（哪个 Task / Provider / Skill / Step 产出此资产）。
    ///
    /// Option 因为旧数据和导入数据可能没有 provenance。
    /// 回溯路径：Artifact → Provenance.task_id → TaskRecord → Provider → Model。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

// ─── ArtifactStore ───

/// 资产存储（当前为内存 Vec，未来可替换为 SQLite / 文件系统索引）。
///
/// 提供按 shot / asset_type 查询能力，避免 Executor 直接操作 Vec。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStore {
    artifacts: Vec<Artifact>,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self {
            artifacts: Vec::new(),
        }
    }

    /// 添加一个资产。
    pub fn push(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }

    /// 按镜头索引查询。
    pub fn by_shot(&self, shot_index: u8) -> Vec<&Artifact> {
        self.artifacts
            .iter()
            .filter(|a| a.shot_index == shot_index)
            .collect()
    }

    /// 按资产类型查询。
    pub fn by_type(&self, asset_type: &AssetType) -> Vec<&Artifact> {
        self.artifacts
            .iter()
            .filter(|a| &a.asset_type == asset_type)
            .collect()
    }

    /// 获取指定镜头的最新资产（最后一个）。
    pub fn latest_for_shot(&self, shot_index: u8) -> Option<&Artifact> {
        self.artifacts
            .iter()
            .rev()
            .find(|a| a.shot_index == shot_index)
    }

    /// 替换指定镜头的最新资产（Critic 重新生成时使用）。
    ///
    /// 如果该镜头有资产，替换最后一个；否则追加。
    pub fn replace_latest(&mut self, shot_index: u8, artifact: Artifact) {
        if let Some(pos) = self
            .artifacts
            .iter()
            .rposition(|a| a.shot_index == shot_index)
        {
            self.artifacts[pos] = artifact;
        } else {
            self.artifacts.push(artifact);
        }
    }

    /// 所有资产。
    pub fn all(&self) -> &[Artifact] {
        &self.artifacts
    }

    /// 资产总数。
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}

// ─── ExecutionContext ───

/// 执行运行时上下文。
///
/// 贯穿整个执行生命周期，记录当前进度、已产生资产和共享变量。
/// Executor 每完成一步都更新此 Context。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionContext {
    /// 关联的执行计划 ID（对应 ExecutionPlan 来源的 PlanRecord.id）。
    pub plan_id: String,
    /// 所属工作区。
    pub workspace_id: String,
    /// 当前正在执行的步骤索引（0-based，对应 ExecutionPlan.steps 下标）。
    pub current_step: usize,
    /// 已产生的资产。
    pub artifact_store: ArtifactStore,
    /// 共享变量（跨步骤传递数据：角色参考图 URL、风格参数、评分等）。
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
    /// 修订迭代次数（用户反馈后重新执行的轮次）。
    #[serde(default)]
    pub revision_iteration: u8,
    /// 运行时元数据（开始时间、执行器版本等）。
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// 执行开始时间（RFC 3339）。
    pub started_at: String,
    /// 最后更新时间（RFC 3339）。
    pub last_updated_at: String,
}

impl ExecutionContext {
    /// 创建新的执行上下文。
    pub fn new(plan_id: String, workspace_id: String) -> Self {
        let now = now_rfc3339();
        Self {
            plan_id,
            workspace_id,
            current_step: 0,
            artifact_store: ArtifactStore::new(),
            variables: HashMap::new(),
            revision_iteration: 0,
            metadata: HashMap::new(),
            started_at: now.clone(),
            last_updated_at: now,
        }
    }

    /// 设置共享变量。
    pub fn set_variable(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.variables.insert(key.into(), value);
    }

    /// 读取共享变量。
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }

    /// 推进到下一步。
    pub fn advance_step(&mut self) {
        self.current_step += 1;
        self.last_updated_at = now_rfc3339();
    }
}

// ─── ExecutionStatus / ExecutionResult ───

/// 执行最终状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// 全部步骤成功完成。
    Completed,
    /// 部分步骤成功，部分失败。
    Partial,
    /// 全部失败或关键步骤失败。
    Failed,
    /// 用户主动取消。
    Cancelled,
}

/// 执行最终结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    /// 关联的计划 ID。
    pub plan_id: String,
    /// 最终状态。
    pub status: ExecutionStatus,
    /// 成功完成的镜头索引列表。
    pub completed_shots: Vec<u8>,
    /// 失败的镜头索引列表。
    pub failed_shots: Vec<u8>,
    /// 最终输出资产（合成后的成品）。
    pub output_artifacts: Vec<Artifact>,
    /// 总执行耗时（秒）。
    pub duration_secs: f64,
}

// ─── Helpers ───

pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
