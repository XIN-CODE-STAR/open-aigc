//! 通用领域类型 — 置信度、推断来源等可复用的基础类型。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 推断来源 — 表示数据是由什么方式产出的。
///
/// 用于 ScriptParser 的推断、SemanticProfile 的标签、
/// RelationCandidate 的发现来源等所有需要区分"谁说了算"的场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InferenceSource {
    /// Rust 启发式规则推断。
    Heuristic,
    /// LLM 推断。
    Llm,
    /// 用户手动输入。
    User,
    /// 外部导入（如 SRT 解析、字幕文件）。
    Imported,
    /// 系统规则（如 ApplyScriptPlan 自动创建）。
    System,
}

impl InferenceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Heuristic => "heuristic",
            Self::Llm => "llm",
            Self::User => "user",
            Self::Imported => "imported",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "heuristic" => Ok(Self::Heuristic),
            "llm" => Ok(Self::Llm),
            "user" => Ok(Self::User),
            "imported" => Ok(Self::Imported),
            "system" => Ok(Self::System),
            _ => Err(format!("unknown inference source: {s}")),
        }
    }
}

/// 带置信度和来源的推断结果。
///
/// 用于所有需要表达"AI/规则推断的值 + 置信度 + 来源"的场景：
/// - ScriptParser 的 shot_type / camera_motion 推断
/// - SemanticProfile 的 tag / entity 识别
/// - RelationCandidate 的关系类型推断
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceResult<T> {
    /// 推断出的值。
    pub value: T,
    /// 置信度 0.0 ~ 1.0。
    pub confidence: f32,
    /// 推断来源。
    pub source: InferenceSource,
}

impl<T> InferenceResult<T> {
    /// 创建用户手动输入的高置信度结果（confidence = 1.0）。
    pub fn user(value: T) -> Self {
        Self {
            value,
            confidence: 1.0,
            source: InferenceSource::User,
        }
    }

    /// 创建启发式推断的结果。
    pub fn heuristic(value: T, confidence: f32) -> Self {
        Self {
            value,
            confidence,
            source: InferenceSource::Heuristic,
        }
    }

    /// 创建 LLM 推断的结果。
    pub fn llm(value: T, confidence: f32) -> Self {
        Self {
            value,
            confidence,
            source: InferenceSource::Llm,
        }
    }

    /// 创建系统规则的结果（confidence = 1.0）。
    pub fn system(value: T) -> Self {
        Self {
            value,
            confidence: 1.0,
            source: InferenceSource::System,
        }
    }

    /// 创建外部导入的结果。
    pub fn imported(value: T, confidence: f32) -> Self {
        Self {
            value,
            confidence,
            source: InferenceSource::Imported,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_source_roundtrip() {
        for source in [
            InferenceSource::Heuristic,
            InferenceSource::Llm,
            InferenceSource::User,
            InferenceSource::Imported,
            InferenceSource::System,
        ] {
            let s = source.as_str();
            let parsed = InferenceSource::parse(s).unwrap();
            assert_eq!(source, parsed, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn inference_source_serialization() {
        let src = InferenceSource::Heuristic;
        let json = serde_json::to_string(&src).unwrap();
        assert_eq!(json, "\"heuristic\"");
        let back: InferenceSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InferenceSource::Heuristic);
    }

    #[test]
    fn inference_result_user() {
        let result = InferenceResult::user("close-up".to_owned());
        assert_eq!(result.confidence, 1.0);
        assert_eq!(result.source, InferenceSource::User);
        assert_eq!(result.value, "close-up");
    }

    #[test]
    fn inference_result_heuristic() {
        let result = InferenceResult::heuristic("wide".to_owned(), 0.82);
        assert!((result.confidence - 0.82).abs() < f32::EPSILON);
        assert_eq!(result.source, InferenceSource::Heuristic);
    }

    #[test]
    fn inference_result_serialization() {
        let result = InferenceResult::llm("medium".to_owned(), 0.95);
        let json = serde_json::to_string(&result).unwrap();
        let back: InferenceResult<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.value, "medium");
        assert!((back.confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(back.source, InferenceSource::Llm);
    }
}
