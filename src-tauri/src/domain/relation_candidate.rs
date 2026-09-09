//! 关系候选领域模型 — RAG → Agent → Canvas 的中间层。
#![allow(dead_code)]
//!
//! RAG 不直接 INSERT canvas_edges，而是产生 RelationCandidate。
//! Agent 读取候选关系后判断：approve → CanvasCommand::AddEdge，reject → 更新 status。
//!
//! 宪法第 3 条：RAG 只产生检索结果和关系候选，不直接修改 Canvas。
//! 宪法第 6 条：Canvas Edge 保存"已确认关系"，RelationCandidate 保存"AI 推测关系"。

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ──────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RelationCandidateError {
    #[error("source node id is required")]
    SourceNodeIdRequired,

    #[error("target node id is required")]
    TargetNodeIdRequired,

    #[error("source and target node must be different")]
    SelfLoop,

    #[error("relation type is required")]
    RelationTypeRequired,

    #[error("invalid confidence value: {0} (must be 0.0-1.0)")]
    InvalidConfidence(f32),

    #[error("candidate is not in pending status")]
    NotPending,
}

// ──────────────────────────────────────────────────────────────────
// RelationSource — 关系来源
// ──────────────────────────────────────────────────────────────────

/// 关系候选的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationSource {
    /// RAG 语义检索发现。
    Rag,
    /// Agent 推理发现。
    Agent,
    /// 用户手动创建。
    User,
    /// 系统规则（如 ApplyScriptPlan 自动创建）。
    System,
}

impl RelationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rag => "rag",
            Self::Agent => "agent",
            Self::User => "user",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "rag" => Ok(Self::Rag),
            "agent" => Ok(Self::Agent),
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            _ => Err(format!("unknown relation source: {s}")),
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// CandidateStatus — 候选状态
// ──────────────────────────────────────────────────────────────────

/// 关系候选的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateStatus {
    /// 等待 Agent/用户判断。
    Pending,
    /// 已确认 → 转为 CanvasEdge。
    Accepted,
    /// 已拒绝。
    Rejected,
}

impl CandidateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("unknown candidate status: {s}")),
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// RelationCandidate — 关系候选实体
// ──────────────────────────────────────────────────────────────────

/// 关系候选 — RAG 发现的关系，等待 Agent/用户确认。
///
/// 数据流：
/// RAG 检索 → 发现两个节点有潜在关系 → 写入 RelationCandidate
///     → Agent 读取 → 判断 accept/reject
///     → accept 时通过 CanvasCommand::AddEdge 转为 CanvasEdge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationCandidate {
    pub id: String,

    /// 关系的起始节点。
    pub source_node_id: String,
    /// 关系的目标节点。
    pub target_node_id: String,

    /// 关系类型（如 "same_character", "same_style", "similar_visual", "temporal_next"）。
    pub relation_type: String,
    /// 置信度 0.0 ~ 1.0。
    pub confidence: f32,
    /// 支持证据（如 ["white hair", "red jacket", "face similarity"]）。
    pub evidence_json: Option<String>,

    /// 候选来源。
    pub source: RelationSource,
    /// 当前状态。
    pub status: CandidateStatus,

    pub created_at: String,
    pub reviewed_at: Option<String>,
}

impl RelationCandidate {
    /// 验证字段。
    pub fn validate(&self) -> Result<(), RelationCandidateError> {
        if self.source_node_id.is_empty() {
            return Err(RelationCandidateError::SourceNodeIdRequired);
        }
        if self.target_node_id.is_empty() {
            return Err(RelationCandidateError::TargetNodeIdRequired);
        }
        if self.source_node_id == self.target_node_id {
            return Err(RelationCandidateError::SelfLoop);
        }
        if self.relation_type.is_empty() {
            return Err(RelationCandidateError::RelationTypeRequired);
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(RelationCandidateError::InvalidConfidence(self.confidence));
        }
        Ok(())
    }

    /// 是否处于待处理状态。
    pub fn is_pending(&self) -> bool {
        self.status == CandidateStatus::Pending
    }
}

// ──────────────────────────────────────────────────────────────────
// RelationCandidateDraft — 构造输入
// ──────────────────────────────────────────────────────────────────

/// 创建关系候选的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationCandidateDraft {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub confidence: f32,
    pub evidence_json: Option<String>,
    pub source: RelationSource,
}

impl RelationCandidateDraft {
    pub fn validate(&self) -> Result<(), RelationCandidateError> {
        if self.source_node_id.is_empty() {
            return Err(RelationCandidateError::SourceNodeIdRequired);
        }
        if self.target_node_id.is_empty() {
            return Err(RelationCandidateError::TargetNodeIdRequired);
        }
        if self.source_node_id == self.target_node_id {
            return Err(RelationCandidateError::SelfLoop);
        }
        if self.relation_type.is_empty() {
            return Err(RelationCandidateError::RelationTypeRequired);
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(RelationCandidateError::InvalidConfidence(self.confidence));
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate() -> RelationCandidate {
        RelationCandidate {
            id: "rc-1".to_owned(),
            source_node_id: "node-a".to_owned(),
            target_node_id: "node-b".to_owned(),
            relation_type: "same_character".to_owned(),
            confidence: 0.94,
            evidence_json: Some(r#"["white hair", "red jacket", "face similarity"]"#.to_owned()),
            source: RelationSource::Rag,
            status: CandidateStatus::Pending,
            created_at: "2026-08-17T00:00:00Z".to_owned(),
            reviewed_at: None,
        }
    }

    #[test]
    fn valid_candidate() {
        let candidate = make_candidate();
        assert!(candidate.validate().is_ok());
    }

    #[test]
    fn self_loop() {
        let mut candidate = make_candidate();
        candidate.target_node_id = candidate.source_node_id.clone();
        assert!(matches!(
            candidate.validate(),
            Err(RelationCandidateError::SelfLoop)
        ));
    }

    #[test]
    fn empty_relation_type() {
        let mut candidate = make_candidate();
        candidate.relation_type = String::new();
        assert!(matches!(
            candidate.validate(),
            Err(RelationCandidateError::RelationTypeRequired)
        ));
    }

    #[test]
    fn invalid_confidence() {
        let mut candidate = make_candidate();
        candidate.confidence = 1.5;
        assert!(matches!(
            candidate.validate(),
            Err(RelationCandidateError::InvalidConfidence(1.5))
        ));
    }

    #[test]
    fn is_pending() {
        let mut candidate = make_candidate();
        assert!(candidate.is_pending());

        candidate.status = CandidateStatus::Accepted;
        assert!(!candidate.is_pending());
    }

    #[test]
    fn relation_source_roundtrip() {
        for source in [
            RelationSource::Rag,
            RelationSource::Agent,
            RelationSource::User,
            RelationSource::System,
        ] {
            let s = source.as_str();
            let parsed = RelationSource::parse(s).unwrap();
            assert_eq!(source, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn candidate_status_roundtrip() {
        for status in [
            CandidateStatus::Pending,
            CandidateStatus::Accepted,
            CandidateStatus::Rejected,
        ] {
            let s = status.as_str();
            let parsed = CandidateStatus::parse(s).unwrap();
            assert_eq!(status, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn serialization_roundtrip() {
        let candidate = make_candidate();
        let json = serde_json::to_string(&candidate).unwrap();
        let back: RelationCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "rc-1");
        assert_eq!(back.relation_type, "same_character");
        assert_eq!(back.source, RelationSource::Rag);
        assert_eq!(back.status, CandidateStatus::Pending);
    }

    #[test]
    fn draft_validation() {
        let draft = RelationCandidateDraft {
            source_node_id: "n1".to_owned(),
            target_node_id: "n2".to_owned(),
            relation_type: "same_style".to_owned(),
            confidence: 0.85,
            evidence_json: None,
            source: RelationSource::Rag,
        };
        assert!(draft.validate().is_ok());
    }

    #[test]
    fn draft_self_loop() {
        let draft = RelationCandidateDraft {
            source_node_id: "n1".to_owned(),
            target_node_id: "n1".to_owned(),
            relation_type: "same_style".to_owned(),
            confidence: 0.85,
            evidence_json: None,
            source: RelationSource::Rag,
        };
        assert!(matches!(
            draft.validate(),
            Err(RelationCandidateError::SelfLoop)
        ));
    }
}
