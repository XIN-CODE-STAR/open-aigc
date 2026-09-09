//! Creative Plan：纯声明式创作计划。
//!
//! 描述"要做什么"（分镜、风格、约束），不包含执行信息（provider、status、retry）。
//! 执行态由 Executor 层的 ExecutionTask 管理。
//!
//! 职责链：Director（意图）→ Planner（规划）→ PlanBuilder（转换）→ Executor（执行）

use serde::{Deserialize, Serialize};

use crate::application::creative_director_service::CreativeBrief;

/// 资产类型（声明式：只说"需要什么类型"，不指定 provider）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Image,
    Video,
    Audio,
    Text,
}

impl std::fmt::Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetType::Image => write!(f, "image"),
            AssetType::Video => write!(f, "video"),
            AssetType::Audio => write!(f, "audio"),
            AssetType::Text => write!(f, "text"),
        }
    }
}

/// 单个镜头计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotPlan {
    /// 镜头序号（从 1 开始）。
    pub index: u8,
    /// 镜头目标（Opening / Environment / Emotion / Action / Ending 等）。
    pub goal: String,
    /// 镜头内容描述。
    pub description: String,
    /// 时长（秒）。
    pub duration_secs: f32,
    /// 需要的资产类型。
    pub asset_type: AssetType,
    /// 风格描述（可覆盖全局风格）。
    #[serde(default)]
    pub style: String,
    /// 参考素材描述（角色、场景、情绪板等）。
    #[serde(default)]
    pub references: Vec<String>,
    /// 转场方式（cut / fade / dissolve / wipe 等）。
    #[serde(default)]
    pub transition: String,
}

/// 规划元数据（调试 + 未来 A/B Test）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningMetadata {
    /// Planner 版本。
    pub planner_version: String,
    /// 使用的 LLM 模型标识。
    #[serde(default)]
    pub llm_model: String,
    /// 规划置信度（0.0 ~ 1.0）。
    #[serde(default)]
    pub confidence: f64,
}

/// 完整创作计划（纯声明式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreativePlan {
    /// 计划唯一 ID。
    pub id: String,
    /// Schema 版本（用于未来迁移）。
    #[serde(default = "default_plan_version")]
    pub version: u8,
    /// 所属工作区。
    pub workspace_id: String,
    /// 来源 Creative Brief（意图快照）。
    pub brief: CreativeBrief,
    /// 全局风格约束。
    #[serde(default)]
    pub global_style: String,
    /// 全局约束列表（如"不使用真人面孔"、"时长不超过30秒"）。
    #[serde(default)]
    pub constraints: Vec<String>,
    /// 分镜列表。
    pub shots: Vec<ShotPlan>,
    /// 规划元数据。
    pub metadata: PlanningMetadata,
    /// 创建时间（RFC 3339）。
    pub created_at: String,
}

fn default_plan_version() -> u8 {
    1
}

impl CreativePlan {
    /// 总时长（秒）。
    pub fn total_duration_secs(&self) -> f32 {
        self.shots.iter().map(|s| s.duration_secs).sum()
    }

    /// 镜头数量。
    pub fn shot_count(&self) -> usize {
        self.shots.len()
    }
}
