#![allow(dead_code)]
//! Runtime Event Emitter Port：运行时事件监听器接口。
//!
//! 定义 RuntimeEventBus 的下游消费者契约。
//! 实现示例：TauriRuntimeEmitter（前端推送）、SqliteRuntimeListener（持久化）、Logger。

use crate::domain::runtime_event::RuntimeEvent;

/// 运行时事件监听器。
///
/// 每个实现决定如何处理收到的事件（emit / persist / log）。
/// emit() 失败不应阻断流水线，实现方自行处理错误。
pub trait RuntimeEventListener: Send + Sync {
    /// 接收一个运行时事件。
    ///
    /// 实现方应自行处理错误（log + 吞掉），不向上游传播。
    fn on_event(&self, event: &RuntimeEvent);
}

/// 空监听器（无操作），用作测试占位或禁用事件时的默认值。
pub struct NoopRuntimeListener;

impl RuntimeEventListener for NoopRuntimeListener {
    fn on_event(&self, _event: &RuntimeEvent) {
        // 无操作
    }
}

/// 日志监听器：将事件打印到 stderr（开发调试用）。
pub struct LoggingRuntimeListener;

impl RuntimeEventListener for LoggingRuntimeListener {
    fn on_event(&self, event: &RuntimeEvent) {
        eprintln!(
            "[RuntimeEvent] type={}, run_id={}",
            event.event_type(),
            event.run_id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::runtime_event::{RuntimeEvent, RuntimePhase};

    #[test]
    fn test_noop_listener_does_not_panic() {
        let listener = NoopRuntimeListener;
        let event = RuntimeEvent::RunStarted {
            run_id: "test-001".to_owned(),
            total_shots: 3,
        };
        listener.on_event(&event);
    }

    #[test]
    fn test_logging_listener_does_not_panic() {
        let listener = LoggingRuntimeListener;
        let event = RuntimeEvent::PhaseChanged {
            run_id: "test-002".to_owned(),
            phase: RuntimePhase::Submitting,
            message: "test".to_owned(),
        };
        listener.on_event(&event);
    }
}
