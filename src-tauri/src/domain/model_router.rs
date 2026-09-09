use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// 模型路由任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingTaskType {
    /// 文本生成（剧本、Prompt、脚本）
    TextGeneration,
    /// 图片生成
    ImageGeneration,
    /// 视频生成
    VideoGeneration,
    /// 语音合成
    TextToSpeech,
    /// 视觉评价（AI Critic）
    VisionEvaluation,
    /// 内容安全检查
    ContentGuard,
    /// 用户反馈解析
    FeedbackParsing,
    /// 角色一致性检查
    CharacterConsistency,
    /// 风格一致性检查
    StyleConsistency,
}

impl RoutingTaskType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextGeneration => "text_generation",
            Self::ImageGeneration => "image_generation",
            Self::VideoGeneration => "video_generation",
            Self::TextToSpeech => "text_to_speech",
            Self::VisionEvaluation => "vision_evaluation",
            Self::ContentGuard => "content_guard",
            Self::FeedbackParsing => "feedback_parsing",
            Self::CharacterConsistency => "character_consistency",
            Self::StyleConsistency => "style_consistency",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RouterError> {
        match value {
            "text_generation" => Ok(Self::TextGeneration),
            "image_generation" => Ok(Self::ImageGeneration),
            "video_generation" => Ok(Self::VideoGeneration),
            "text_to_speech" => Ok(Self::TextToSpeech),
            "vision_evaluation" => Ok(Self::VisionEvaluation),
            "content_guard" => Ok(Self::ContentGuard),
            "feedback_parsing" => Ok(Self::FeedbackParsing),
            "character_consistency" => Ok(Self::CharacterConsistency),
            "style_consistency" => Ok(Self::StyleConsistency),
            _ => Err(RouterError::InvalidTaskType(value.to_owned())),
        }
    }

    /// 任务是否为高成本操作（需要更严格的成本控制）。
    pub fn is_high_cost(&self) -> bool {
        matches!(self, Self::VideoGeneration | Self::ImageGeneration)
    }

    /// 任务是否为分析型操作（不需要生成结果）。
    pub fn is_analysis(&self) -> bool {
        matches!(
            self,
            Self::VisionEvaluation
                | Self::ContentGuard
                | Self::FeedbackParsing
                | Self::CharacterConsistency
                | Self::StyleConsistency
        )
    }
}

/// 模型路由策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// 最低成本优先
    CostOptimized,
    /// 最高质量优先
    QualityFirst,
    /// 最快速度优先
    SpeedFirst,
    /// 平衡模式（默认）
    Balanced,
    /// 用户指定
    UserSpecified,
}

impl RoutingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CostOptimized => "cost_optimized",
            Self::QualityFirst => "quality_first",
            Self::SpeedFirst => "speed_first",
            Self::Balanced => "balanced",
            Self::UserSpecified => "user_specified",
        }
    }
}

/// 高成本或关键动作的审批策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// 低成本、可逆动作自动执行。
    Auto,
    /// 高成本生成动作需要展示成本/耗时并等待确认。
    ConfirmCost,
    /// 最终成品输出前确认。
    ConfirmFinal,
}

impl ApprovalPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ConfirmCost => "confirm_cost",
            Self::ConfirmFinal => "confirm_final",
        }
    }
}

/// SQLite Capability Registry 中的模型能力记录。
#[derive(Debug, Clone)]
pub struct ModelCapabilityRecord {
    pub id: String,
    pub provider_id: String,
    pub model_name: String,
    pub display_name: String,
    pub task_type: RoutingTaskType,
    /// 能力向量，维度固定在 Phase 1 的 8 个核心维度内。
    pub capabilities: BTreeMap<String, f64>,
    pub prerequisites: Vec<String>,
    pub constraints: ModelCapabilityConstraints,
    pub failure_log: BTreeMap<String, CapabilityFailureStats>,
    pub cost_score: f64,
    pub speed_score: f64,
    pub quality_score: f64,
    pub enabled: bool,
}

/// 模型硬限制。Phase 1 只使用 supports_reference / max_duration_seconds。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilityConstraints {
    pub max_duration_seconds: Option<f64>,
    pub max_resolution: Option<String>,
    pub supported_ratios: Vec<String>,
    pub supports_reference: bool,
}

/// 运行时失败学习条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityFailureStats {
    pub attempts: u64,
    pub failures: u64,
    pub score_delta: f64,
}

/// 模型候选。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCandidate {
    /// Provider ID
    pub provider_id: String,
    /// 模型名称
    pub model_name: String,
    /// 显示名称
    pub display_name: String,
    /// 能力匹配度（0.0-1.0）
    pub capability_score: f64,
    /// 成本评分（越低越好，0.0-1.0）
    pub cost_score: f64,
    /// 速度评分（越高越快，0.0-1.0）
    pub speed_score: f64,
    /// 质量评分（越高越好，0.0-1.0）
    pub quality_score: f64,
    /// 综合评分（根据策略加权计算）
    pub overall_score: f64,
    /// 是否可用
    pub available: bool,
    /// 剩余配额（-1 表示无限制）
    pub remaining_quota: i64,
    /// 最近一次健康检查状态
    pub health_status: String,
}

/// 路由决策结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDecision {
    /// 选中的模型候选
    pub selected: ModelCandidate,
    /// 备选模型列表
    pub alternatives: Vec<ModelCandidate>,
    /// 决策原因
    pub reason: String,
    /// 使用的策略
    pub strategy: RoutingStrategy,
    /// 是否需要用户确认（高成本操作）
    pub requires_confirmation: bool,
    /// 审批策略。
    pub approval_policy: ApprovalPolicy,
    /// 预估成本（如有）
    pub estimated_cost: Option<f64>,
}

/// 路由请求。
#[derive(Debug, Clone)]
pub struct RoutingRequest {
    /// 任务类型
    pub task_type: RoutingTaskType,
    /// 路由策略
    pub strategy: RoutingStrategy,
    /// 用户指定的 Provider（可选）
    pub preferred_provider: Option<String>,
    /// 用户指定的模型（可选）
    pub preferred_model: Option<String>,
    /// 预算上限（可选）
    pub budget_limit: Option<f64>,
    /// 是否需要参考图支持
    pub requires_reference: bool,
    /// 上下文信息（用于智能路由）
    pub context: Option<String>,
}

/// 路由错误。
#[derive(Debug, Clone, Error)]
pub enum RouterError {
    #[error("invalid task type: {0}")]
    InvalidTaskType(String),
    #[error("no available model for task type {task_type}")]
    NoAvailableModel { task_type: String },
    #[error("preferred model {model} is not available")]
    PreferredModelUnavailable { model: String },
    #[error("budget limit exceeded: estimated {estimated} > limit {limit}")]
    BudgetExceeded { estimated: f64, limit: f64 },
    #[error("provider {provider_id} is not healthy: {status}")]
    ProviderUnhealthy { provider_id: String, status: String },
}
