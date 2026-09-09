#![allow(dead_code)]
//! Runtime Event：创作流水线事件系统领域类型。
//!
//! 定义 CreativeRuntimeService 在四阶段编排过程中发出的所有事件。
//! 与 ExecutionPersistenceEvent 共存不合并：
//! - ExecutionPersistenceEvent = SequentialExecutor 持久化协议
//! - RuntimeEvent = CreativeRuntimeService 领域事件（前端实时展示用）
//!
//! 事件流向：
//! CreativeRuntimeService → RuntimeEventBus → Listeners (Tauri, SQLite, Logger)

use serde::{Deserialize, Serialize};

// ─── RuntimePhase ───

/// 创作运行时阶段（完整生命周期，含 Planning）。
///
/// 与 creative_runtime_service::RuntimePhase 的关系：
/// 此处多了 Planning 阶段，覆盖从意图解析到最终合成的全流程。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    /// 意图解析 + CreativePlan 生成中。
    Planning,
    /// 提交生成请求中。
    Submitting,
    /// 轮询远端状态 + 下载中。
    Polling,
    /// 导入资产库中。
    Importing,
    /// 合成最终作品中。
    Composing,
    /// 全部完成。
    Completed,
    /// 失败。
    Failed,
}

// ─── ShotStatus ───

/// 单个镜头的运行时状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShotStatus {
    /// 等待处理。
    Pending,
    /// 正在提交到远端 API。
    Submitting,
    /// 远端生成中。
    Generating,
    /// 已下载到本地。
    Downloaded,
    /// 已导入资产库。
    Imported,
    /// 已参与合成。
    Composed,
    /// 失败。
    Failed,
}

// ─── RuntimeEvent ───

/// 创作流水线事件。
///
/// 每个变体携带 `run_id` 以区分并行执行的多条流水线。
/// 事件不可变（无 &mut self），仅做状态通知。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    /// 流水线开始执行。
    RunStarted { run_id: String, total_shots: u32 },

    /// 阶段切换。
    PhaseChanged {
        run_id: String,
        phase: RuntimePhase,
        message: String,
    },

    /// 单个镜头状态更新。
    ShotUpdated {
        run_id: String,
        shot_index: u32,
        status: ShotStatus,
        artifact_id: Option<String>,
        message: String,
    },

    /// 提交状态更新（Phase 1 细粒度）。
    SubmissionUpdated {
        run_id: String,
        shot_index: u32,
        submission_id: String,
        provider: String,
        model: String,
    },

    /// 流水线执行完毕。
    RunCompleted {
        run_id: String,
        status: String,
        output_asset_id: Option<String>,
        duration_secs: f64,
        shot_success_count: u32,
        shot_total_count: u32,
    },
}

impl RuntimeEvent {
    /// 提取事件所属的 run_id。
    pub fn run_id(&self) -> &str {
        match self {
            Self::RunStarted { run_id, .. }
            | Self::PhaseChanged { run_id, .. }
            | Self::ShotUpdated { run_id, .. }
            | Self::SubmissionUpdated { run_id, .. }
            | Self::RunCompleted { run_id, .. } => run_id,
        }
    }

    /// 提取事件类型的简短标签（用于日志/调试）。
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::PhaseChanged { .. } => "phase_changed",
            Self::ShotUpdated { .. } => "shot_updated",
            Self::SubmissionUpdated { .. } => "submission_updated",
            Self::RunCompleted { .. } => "run_completed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_phase_serde() {
        let phase = RuntimePhase::Submitting;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, r#""submitting""#);

        let parsed: RuntimePhase = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RuntimePhase::Submitting);
    }

    #[test]
    fn test_shot_status_serde() {
        let status = ShotStatus::Generating;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""generating""#);
    }

    #[test]
    fn test_runtime_event_tagged_serde() {
        let event = RuntimeEvent::RunStarted {
            run_id: "run-001".to_owned(),
            total_shots: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"run_started""#));
        assert!(json.contains(r#""run_id":"run-001""#));
        assert!(json.contains(r#""total_shots":3"#));
    }

    #[test]
    fn test_runtime_event_run_id_accessor() {
        let event = RuntimeEvent::PhaseChanged {
            run_id: "run-042".to_owned(),
            phase: RuntimePhase::Polling,
            message: "Polling...".to_owned(),
        };
        assert_eq!(event.run_id(), "run-042");
        assert_eq!(event.event_type(), "phase_changed");
    }

    #[test]
    fn test_runtime_event_completed() {
        let event = RuntimeEvent::RunCompleted {
            run_id: "run-100".to_owned(),
            status: "completed".to_owned(),
            output_asset_id: Some("asset-xyz".to_owned()),
            duration_secs: 45.3,
            shot_success_count: 3,
            shot_total_count: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"run_completed""#));
        assert!(json.contains(r#""output_asset_id":"asset-xyz""#));
    }
}
