//! Canvas 领域模型 — 画布即工作空间。
#![allow(dead_code)]
//!
//! 将画布从 Memory 中解耦，成为独立的创作工作空间。
//! CanvasNode / CanvasEdge 是画布的基本单元，通过 typed refs 关联到
//! Memory、Artifact、Task、Shot、Project、Agent 等其他领域对象。
//!
//! 设计原则：
//! - Canvas = Workspace（创作空间），Memory = Context（上下文）
//! - 强类型枚举替代 stringly-typed node_type / edge_type
//! - refs 字段将画布节点与领域对象关联（可查询、可反查）
//! - SemanticRelation 为空间感知提供 LLM 可读的语义关系

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ──────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────

/// Canvas 领域验证错误。
#[derive(Debug, Error)]
pub enum CanvasError {
    #[error("summary is required")]
    SummaryRequired,

    #[error("summary exceeds {max} characters")]
    SummaryTooLong { max: usize },

    #[error("description exceeds {max} characters")]
    DescriptionTooLong { max: usize },

    #[error("prompt exceeds {max} characters")]
    PromptTooLong { max: usize },

    #[error("label exceeds {max} characters")]
    LabelTooLong { max: usize },

    #[error("invalid node kind: {0}")]
    InvalidNodeKind(String),

    #[error("invalid edge kind: {0}")]
    InvalidEdgeKind(String),

    #[error("invalid node status: {0}")]
    InvalidNodeStatus(String),

    #[error("source and target node must be different")]
    SelfLoop,

    #[error("source node {0} not found")]
    SourceNotFound(String),

    #[error("target node {0} not found")]
    TargetNotFound(String),

    #[error("duplicate edge between {source_node} and {target_node} with kind {kind}")]
    DuplicateEdge {
        source_node: String,
        target_node: String,
        kind: String,
    },
}

// ──────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────

const SUMMARY_MAX: usize = 500;
const DESCRIPTION_MAX: usize = 8000;
const PROMPT_MAX: usize = 8000;
const LABEL_MAX: usize = 200;

// ──────────────────────────────────────────────────────────────────
// CanvasNodeKind — 替代 stringly-typed node_type
// ──────────────────────────────────────────────────────────────────

/// 画布节点类型枚举。
///
/// 替代原来的 free-form String `node_type`，提供类型安全和语义清晰度。
/// 注意：AgentState 不属于 CanvasNodeKind — runtime state ≠ workspace element。
/// Agent 状态通过 Task 生命周期管理，Canvas 如需展示则通过前端 overlay。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasNodeKind {
    /// 场景容器 — 对应 SceneRecord。
    Scene,
    /// 单个镜头 — 对应 ShotRecord。
    Shot,
    /// 生成或上传的图片 — 对应 Artifact(Image)。
    Image,
    /// 生成或上传的视频 — 对应 Artifact(Video)。
    Video,
    /// 用户上传的参考素材。
    Upload,
    /// 文本注释 / 便利贴。
    Note,
    /// 知识事实节点。
    Fact,
    /// 文档 / 剧本。
    Document,
    /// 角色参考卡片 — 对应 CharacterProfileRecord。
    Character,
    /// Prompt 模板 / 片段。
    Prompt,
}

impl CanvasNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Shot => "shot",
            Self::Image => "image",
            Self::Video => "video",
            Self::Upload => "upload",
            Self::Note => "note",
            Self::Fact => "fact",
            Self::Document => "document",
            Self::Character => "character",
            Self::Prompt => "prompt",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CanvasError> {
        match s {
            "scene" => Ok(Self::Scene),
            "shot" => Ok(Self::Shot),
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "upload" => Ok(Self::Upload),
            "note" => Ok(Self::Note),
            "fact" => Ok(Self::Fact),
            "document" => Ok(Self::Document),
            "character" => Ok(Self::Character),
            "prompt" => Ok(Self::Prompt),
            _ => Err(CanvasError::InvalidNodeKind(s.to_owned())),
        }
    }

    /// 返回所有合法值（用于错误提示）。
    pub fn all_variants() -> &'static [&'static str] {
        &[
            "scene",
            "shot",
            "image",
            "video",
            "upload",
            "note",
            "fact",
            "document",
            "character",
            "prompt",
        ]
    }

    /// 是否为媒体类型（有资产内容的节点）。
    pub fn is_media(self) -> bool {
        matches!(self, Self::Image | Self::Video | Self::Upload)
    }

    /// 是否为结构类型（对应 MangaProject 层级的节点）。
    pub fn is_structural(self) -> bool {
        matches!(self, Self::Scene | Self::Shot | Self::Character)
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasEdgeKind — 替代 stringly-typed edge_type
// ──────────────────────────────────────────────────────────────────

/// 画布边类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasEdgeKind {
    /// A 引用 B（如 shot 使用 character ref）。
    Reference,
    /// A 依赖 B（如 shot 需要前一个 shot 的结果）。
    Dependency,
    /// A → B 时序关系（镜头排列顺序）。
    Sequence,
    /// A 是 B 的组成部分（scene 包含 shots）。
    Composition,
    /// A 与 B 相似（搜索结果关联）。
    Similarity,
    /// Agent 基于 B 创建/修改了 A。
    AgentLink,
}

impl CanvasEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Dependency => "dependency",
            Self::Sequence => "sequence",
            Self::Composition => "composition",
            Self::Similarity => "similarity",
            Self::AgentLink => "agent-link",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CanvasError> {
        match s {
            "reference" => Ok(Self::Reference),
            "dependency" => Ok(Self::Dependency),
            "sequence" => Ok(Self::Sequence),
            "composition" => Ok(Self::Composition),
            "similarity" => Ok(Self::Similarity),
            "agent-link" => Ok(Self::AgentLink),
            _ => Err(CanvasError::InvalidEdgeKind(s.to_owned())),
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// RelationKind — 两层关系模型（Core + Semantic）
// ──────────────────────────────────────────────────────────────────

/// 关系类型 — 统一表示核心系统关系和语义关系。
///
/// 核心关系（Reference/Dependency/Sequence/Composition）是系统逻辑的一部分。
/// 语义关系（same_character/same_style/...）是 RAG 发现的，种类不可预测。
/// 两层模型让系统关系保持稳定，语义关系保持可扩展。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RelationKind {
    /// 核心系统关系 — 编译期固定。
    Core(CanvasEdgeKind),
    /// 语义关系 — 字符串标识，RAG 发现。
    Semantic(String),
}

impl RelationKind {
    /// 用于数据库 kind 列的存储格式。
    pub fn to_db_string(&self) -> String {
        match self {
            Self::Core(edge) => edge.as_str().to_owned(),
            Self::Semantic(name) => format!("semantic:{name}"),
        }
    }

    /// 从数据库 kind 列解析。
    pub fn from_db_string(s: &str) -> Result<Self, CanvasError> {
        if let Some(semantic_name) = s.strip_prefix("semantic:") {
            Ok(Self::Semantic(semantic_name.to_owned()))
        } else {
            CanvasEdgeKind::parse(s).map(Self::Core)
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasNodeStatus
// ──────────────────────────────────────────────────────────────────

/// 画布节点状态枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasNodeStatus {
    /// 等待处理。
    Pending,
    /// 正在生成中（AI 任务进行中）。
    Generating,
    /// 成功完成。
    Succeeded,
    /// 处理失败。
    Failed,
    /// 草稿状态。
    Draft,
    /// 就绪（可执行）。
    Ready,
}

impl CanvasNodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Generating => "generating",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Draft => "draft",
            Self::Ready => "ready",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CanvasError> {
        match s {
            "pending" => Ok(Self::Pending),
            "generating" => Ok(Self::Generating),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            _ => Err(CanvasError::InvalidNodeStatus(s.to_owned())),
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasNodeRefs — 领域对象引用
// ──────────────────────────────────────────────────────────────────

/// 画布节点到其他领域对象的引用集合。
///
/// 每个引用字段都是 Optional，表示该节点可能关联的领域对象。
/// 持久化时，每个非 None 的引用存为独立列（可查询、可索引）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodeRefs {
    /// → CreativeMemoryRecord（长期记忆/偏好知识）。
    pub memory_id: Option<String>,
    /// → Artifact（生成结果资产）。
    pub artifact_id: Option<String>,
    /// → TaskRecord（执行任务）。
    pub task_id: Option<String>,
    /// → ShotRecord（MangaProject 镜头）。
    pub shot_id: Option<String>,
    /// → SceneRecord（MangaProject 场景）。
    pub scene_id: Option<String>,
    /// → MangaProjectRecord（漫剧项目）。
    pub project_id: Option<String>,
    /// → Sub-agent session ID。
    pub agent_id: Option<String>,
    /// → Asset storage ID。
    pub asset_id: Option<String>,
    /// → 会话 ID（与 Agent 对话关联）。
    pub conversation_id: Option<String>,
}

impl CanvasNodeRefs {
    /// 是否有任何引用。
    pub fn is_empty(&self) -> bool {
        self.memory_id.is_none()
            && self.artifact_id.is_none()
            && self.task_id.is_none()
            && self.shot_id.is_none()
            && self.scene_id.is_none()
            && self.project_id.is_none()
            && self.agent_id.is_none()
            && self.asset_id.is_none()
            && self.conversation_id.is_none()
    }

    /// 返回所有非空引用的 (field_name, value) 对。
    pub fn non_empty_refs(&self) -> Vec<(&'static str, &str)> {
        let mut refs = Vec::new();
        if let Some(v) = &self.memory_id {
            refs.push(("memory_id", v.as_str()));
        }
        if let Some(v) = &self.artifact_id {
            refs.push(("artifact_id", v.as_str()));
        }
        if let Some(v) = &self.task_id {
            refs.push(("task_id", v.as_str()));
        }
        if let Some(v) = &self.shot_id {
            refs.push(("shot_id", v.as_str()));
        }
        if let Some(v) = &self.scene_id {
            refs.push(("scene_id", v.as_str()));
        }
        if let Some(v) = &self.project_id {
            refs.push(("project_id", v.as_str()));
        }
        if let Some(v) = &self.agent_id {
            refs.push(("agent_id", v.as_str()));
        }
        if let Some(v) = &self.asset_id {
            refs.push(("asset_id", v.as_str()));
        }
        if let Some(v) = &self.conversation_id {
            refs.push(("conversation_id", v.as_str()));
        }
        refs
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasPosition / CanvasSize
// ──────────────────────────────────────────────────────────────────

/// 画布坐标。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CanvasPosition {
    pub x: f64,
    pub y: f64,
}

impl Default for CanvasPosition {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// 画布元素尺寸。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CanvasSize {
    pub width: f64,
    pub height: f64,
}

impl Default for CanvasSize {
    fn default() -> Self {
        Self {
            width: 220.0,
            height: 160.0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Canvas — 画布实体
// ──────────────────────────────────────────────────────────────────

/// 画布实体（持久化记录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasRecord {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
    /// 关联的会话 ID（直接 ID 关联，不通过名称约定）。
    pub conversation_ref: Option<String>,
    pub node_count: i64,
    pub edge_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建画布的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDraft {
    pub workspace_id: String,
    pub name: String,
    pub description: Option<String>,
    pub conversation_ref: Option<String>,
}

// ──────────────────────────────────────────────────────────────────
// CanvasNode — 画布节点实体
// ──────────────────────────────────────────────────────────────────

/// 画布节点（持久化记录）。
///
/// 替代原来的 MemoryNode，提供强类型 kind 和 typed refs。
/// `metadata` 是自由键值对，用于扩展字段（不需要 schema 变更）。
/// `revision` 用于乐观并发控制 — UpdateNode 必须指定 expected_revision。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNode {
    pub id: String,
    pub canvas_id: String,
    pub kind: CanvasNodeKind,
    pub position: CanvasPosition,
    pub size: Option<CanvasSize>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub status: Option<CanvasNodeStatus>,
    pub refs: CanvasNodeRefs,
    pub metadata: HashMap<String, serde_json::Value>,
    /// 乐观并发控制版本号 — 每次更新 +1。
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建画布节点的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNodeDraft {
    pub canvas_id: String,
    pub kind: CanvasNodeKind,
    pub position: CanvasPosition,
    pub size: Option<CanvasSize>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub status: Option<CanvasNodeStatus>,
    pub refs: CanvasNodeRefs,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CanvasNodeDraft {
    /// 验证输入字段。
    pub fn validate(&self) -> Result<(), CanvasError> {
        if let Some(ref s) = self.summary {
            if s.len() > SUMMARY_MAX {
                return Err(CanvasError::SummaryTooLong { max: SUMMARY_MAX });
            }
        }
        if let Some(ref d) = self.description {
            if d.len() > DESCRIPTION_MAX {
                return Err(CanvasError::DescriptionTooLong {
                    max: DESCRIPTION_MAX,
                });
            }
        }
        if let Some(ref p) = self.prompt {
            if p.len() > PROMPT_MAX {
                return Err(CanvasError::PromptTooLong { max: PROMPT_MAX });
            }
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasEdge — 画布边实体
// ──────────────────────────────────────────────────────────────────

/// 画布边（持久化记录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasEdge {
    pub id: String,
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: CanvasEdgeKind,
    pub label: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: String,
}

/// 创建画布边的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdgeDraft {
    pub canvas_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: CanvasEdgeKind,
    pub label: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CanvasEdgeDraft {
    /// 验证输入字段。
    pub fn validate(&self) -> Result<(), CanvasError> {
        if self.source_node_id == self.target_node_id {
            return Err(CanvasError::SelfLoop);
        }
        if let Some(ref l) = self.label {
            if l.len() > LABEL_MAX {
                return Err(CanvasError::LabelTooLong { max: LABEL_MAX });
            }
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// NodePatch — 部分更新
// ──────────────────────────────────────────────────────────────────

/// 画布节点部分更新。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePatch {
    pub kind: Option<CanvasNodeKind>,
    pub position: Option<CanvasPosition>,
    pub size: Option<CanvasSize>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub status: Option<CanvasNodeStatus>,
    pub refs: Option<CanvasNodeRefs>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ──────────────────────────────────────────────────────────────────
// SemanticRelation — 语义空间关系
// ──────────────────────────────────────────────────────────────────

/// 语义空间关系 — 替代原始 (x, y) 坐标给 LLM 阅读。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SemanticRelation {
    /// 在…左边。
    LeftOf,
    /// 在…右边。
    RightOf,
    /// 在…上方。
    Above,
    /// 在…下方。
    Below,
    /// 通过边连接。
    ConnectedTo { edge_kind: CanvasEdgeKind },
    /// 在同一个场景组中。
    InSameScene,
    /// 最近邻。
    Nearest { distance_category: DistanceCategory },
    /// 时序上在…之前。
    TemporalBefore,
    /// 时序上在…之后。
    TemporalAfter,
}

/// 距离分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DistanceCategory {
    /// 相邻（< 100px）。
    Adjacent,
    /// 近距离（100-300px）。
    Near,
    /// 远距离（> 300px）。
    Far,
}

// ──────────────────────────────────────────────────────────────────
// ContextualNode — 带语义关系的节点（供 LLM 消费）
// ──────────────────────────────────────────────────────────────────

/// 带语义关系的节点 — CanvasContextBuilder 的输出。
///
/// 包含节点本身、与其他节点的语义关系、以及相关性评分。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextualNode {
    pub node: CanvasNode,
    /// 与其他节点的语义关系 (other_node_id, relation)。
    pub relations: Vec<(String, SemanticRelation)>,
    /// 相关性评分（0.0-1.0），用于 context window 预算控制。
    pub relevance_score: f64,
}

// ──────────────────────────────────────────────────────────────────
// ContextQuery / ContextBudget — 查询参数
// ──────────────────────────────────────────────────────────────────

/// 画布上下文查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextQuery {
    /// 以某个节点为中心的邻近搜索。
    pub center_node_id: Option<String>,
    /// 按节点类型过滤。
    pub kinds: Option<Vec<CanvasNodeKind>>,
    /// 按引用字段过滤（如 "project_id", "scene_id"）。
    pub ref_filter: Option<RefFilter>,
    /// 是否包含语义关系。
    pub include_relations: bool,
    /// 从中心节点出发的最大图遍历深度。
    pub max_depth: Option<u32>,
}

/// 引用过滤条件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefFilter {
    /// 引用字段名（如 "project_id", "scene_id"）。
    pub field: String,
    /// 引用值。
    pub value: String,
}

/// 上下文预算 — 控制返回给 LLM 的信息量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// 最大返回节点数（默认 20）。
    pub max_nodes: usize,
    /// 预估 token 预算（默认 4000）。
    pub max_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_nodes: 20,
            max_tokens: 4000,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// CanvasContext — CanvasContextService 的输出
// ──────────────────────────────────────────────────────────────────

/// 画布上下文 — CanvasContextService 返回给 Agent 工具的结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasContext {
    pub canvas_id: String,
    pub canvas_name: String,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub contextual_nodes: Option<Vec<ContextualNode>>,
    /// 格式化后的 LLM 可读描述。
    pub formatted_description: String,
    pub node_count: usize,
    pub edge_count: usize,
}

// ──────────────────────────────────────────────────────────────────
// PromptContext — PromptCompiler 使用的上下文
// ──────────────────────────────────────────────────────────────────

/// Prompt 编译上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptContext {
    pub contextual_nodes: Vec<ContextualNode>,
    pub related_artifact_prompts: Vec<String>,
    pub character_descriptions: Vec<String>,
    pub scene_description: Option<String>,
    pub estimated_tokens: usize,
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_node_kind_roundtrip() {
        for kind in [
            CanvasNodeKind::Scene,
            CanvasNodeKind::Shot,
            CanvasNodeKind::Image,
            CanvasNodeKind::Video,
            CanvasNodeKind::Upload,
            CanvasNodeKind::Note,
            CanvasNodeKind::Fact,
            CanvasNodeKind::Document,
            CanvasNodeKind::Character,
            CanvasNodeKind::Prompt,
        ] {
            let s = kind.as_str();
            let parsed = CanvasNodeKind::parse(s).unwrap();
            assert_eq!(kind, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn canvas_edge_kind_roundtrip() {
        for kind in [
            CanvasEdgeKind::Reference,
            CanvasEdgeKind::Dependency,
            CanvasEdgeKind::Sequence,
            CanvasEdgeKind::Composition,
            CanvasEdgeKind::Similarity,
            CanvasEdgeKind::AgentLink,
        ] {
            let s = kind.as_str();
            let parsed = CanvasEdgeKind::parse(s).unwrap();
            assert_eq!(kind, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn node_status_roundtrip() {
        for status in [
            CanvasNodeStatus::Pending,
            CanvasNodeStatus::Generating,
            CanvasNodeStatus::Succeeded,
            CanvasNodeStatus::Failed,
            CanvasNodeStatus::Draft,
            CanvasNodeStatus::Ready,
        ] {
            let s = status.as_str();
            let parsed = CanvasNodeStatus::parse(s).unwrap();
            assert_eq!(status, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn node_kind_is_media() {
        assert!(CanvasNodeKind::Image.is_media());
        assert!(CanvasNodeKind::Video.is_media());
        assert!(CanvasNodeKind::Upload.is_media());
        assert!(!CanvasNodeKind::Note.is_media());
        assert!(!CanvasNodeKind::Scene.is_media());
    }

    #[test]
    fn node_kind_is_structural() {
        assert!(CanvasNodeKind::Scene.is_structural());
        assert!(CanvasNodeKind::Shot.is_structural());
        assert!(CanvasNodeKind::Character.is_structural());
        assert!(!CanvasNodeKind::Image.is_structural());
    }

    #[test]
    fn invalid_node_kind() {
        assert!(CanvasNodeKind::parse("invalid").is_err());
    }

    #[test]
    fn invalid_edge_kind() {
        assert!(CanvasEdgeKind::parse("invalid").is_err());
    }

    #[test]
    fn node_refs_non_empty() {
        let refs = CanvasNodeRefs {
            shot_id: Some("shot-1".to_owned()),
            project_id: Some("proj-1".to_owned()),
            ..Default::default()
        };
        assert!(!refs.is_empty());
        let non_empty = refs.non_empty_refs();
        assert_eq!(non_empty.len(), 2);
        assert_eq!(non_empty[0], ("shot_id", "shot-1"));
        assert_eq!(non_empty[1], ("project_id", "proj-1"));
    }

    #[test]
    fn node_refs_empty() {
        let refs = CanvasNodeRefs::default();
        assert!(refs.is_empty());
        assert!(refs.non_empty_refs().is_empty());
    }

    #[test]
    fn edge_draft_self_loop() {
        let draft = CanvasEdgeDraft {
            canvas_id: "c1".to_owned(),
            source_node_id: "n1".to_owned(),
            target_node_id: "n1".to_owned(),
            kind: CanvasEdgeKind::Reference,
            label: None,
            metadata: HashMap::new(),
        };
        assert!(matches!(draft.validate(), Err(CanvasError::SelfLoop)));
    }

    #[test]
    fn node_draft_summary_too_long() {
        let draft = CanvasNodeDraft {
            canvas_id: "c1".to_owned(),
            kind: CanvasNodeKind::Note,
            position: CanvasPosition::default(),
            size: None,
            summary: Some("x".repeat(SUMMARY_MAX + 1)),
            description: None,
            prompt: None,
            status: None,
            refs: CanvasNodeRefs::default(),
            metadata: HashMap::new(),
        };
        assert!(matches!(
            draft.validate(),
            Err(CanvasError::SummaryTooLong { .. })
        ));
    }

    #[test]
    fn context_budget_default() {
        let budget = ContextBudget::default();
        assert_eq!(budget.max_nodes, 20);
        assert_eq!(budget.max_tokens, 4000);
    }

    #[test]
    fn canvas_node_kind_serialization() {
        let kind = CanvasNodeKind::Prompt;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"prompt\"");
        let back: CanvasNodeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CanvasNodeKind::Prompt);
    }

    #[test]
    fn canvas_edge_kind_serialization() {
        let kind = CanvasEdgeKind::AgentLink;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"agent-link\"");
        let back: CanvasEdgeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CanvasEdgeKind::AgentLink);
    }

    #[test]
    fn relation_kind_core() {
        let kind = RelationKind::Core(CanvasEdgeKind::Reference);
        assert_eq!(kind.to_db_string(), "reference");
        let parsed = RelationKind::from_db_string("reference").unwrap();
        assert_eq!(parsed, RelationKind::Core(CanvasEdgeKind::Reference));
    }

    #[test]
    fn relation_kind_semantic() {
        let kind = RelationKind::Semantic("same_character".to_owned());
        assert_eq!(kind.to_db_string(), "semantic:same_character");
        let parsed = RelationKind::from_db_string("semantic:same_character").unwrap();
        assert_eq!(parsed, RelationKind::Semantic("same_character".to_owned()));
    }

    #[test]
    fn agent_state_not_a_valid_node_kind() {
        assert!(CanvasNodeKind::parse("agent-state").is_err());
    }
}
