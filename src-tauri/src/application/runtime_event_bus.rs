#![allow(dead_code)]
//! Runtime Event Bus：运行时事件总线。
//!
//! 职责：将 RuntimeEvent 广播给所有注册的 RuntimeEventListener。
//! 设计：
//! - 内部持有 Vec<Box<dyn RuntimeEventListener>>
//! - emit() 逐个通知，单个 listener 失败不影响其他
//! - 线程安全：通过 Arc<RuntimeEventBus> 在多线程间共享

use crate::domain::runtime_event::RuntimeEvent;
use crate::ports::runtime_event_emitter::RuntimeEventListener;

/// 运行时事件总线。
///
/// 聚合多个 RuntimeEventListener，统一广播。
pub struct RuntimeEventBus {
    listeners: Vec<Box<dyn RuntimeEventListener>>,
}

impl RuntimeEventBus {
    /// 创建空事件总线。
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    /// 注册一个事件监听器。
    pub fn register(&mut self, listener: Box<dyn RuntimeEventListener>) {
        self.listeners.push(listener);
    }

    /// 广播事件给所有已注册的监听器。
    ///
    /// 单个 listener panic 不会传播（Listener 契约要求自行处理错误），
    /// 但如果某个 listener 真的 panic，此处不做 catch_unwind（由实现方保证）。
    pub fn emit(&self, event: RuntimeEvent) {
        for listener in &self.listeners {
            listener.on_event(&event);
        }
    }

    /// 已注册监听器数量。
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

impl Default for RuntimeEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::runtime_event::{RuntimeEvent, RuntimePhase, ShotStatus};
    use crate::ports::runtime_event_emitter::{LoggingRuntimeListener, NoopRuntimeListener};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// 计数监听器（测试用）。
    struct CountingListener {
        count: Arc<AtomicU32>,
    }

    impl RuntimeEventListener for CountingListener {
        fn on_event(&self, _event: &RuntimeEvent) {
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn test_bus_emits_to_all_listeners() {
        let counter = Arc::new(AtomicU32::new(0));
        let mut bus = RuntimeEventBus::new();
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));

        bus.emit(RuntimeEvent::RunStarted {
            run_id: "run-1".to_owned(),
            total_shots: 3,
        });

        // 2 listeners × 1 event = 2
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_bus_emits_multiple_events() {
        let counter = Arc::new(AtomicU32::new(0));
        let mut bus = RuntimeEventBus::new();
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));

        bus.emit(RuntimeEvent::RunStarted {
            run_id: "run-1".to_owned(),
            total_shots: 3,
        });
        bus.emit(RuntimeEvent::PhaseChanged {
            run_id: "run-1".to_owned(),
            phase: RuntimePhase::Submitting,
            message: "Submitting...".to_owned(),
        });
        bus.emit(RuntimeEvent::ShotUpdated {
            run_id: "run-1".to_owned(),
            shot_index: 1,
            status: ShotStatus::Generating,
            artifact_id: None,
            message: "Generating shot 1".to_owned(),
        });

        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_bus_empty_listeners() {
        let bus = RuntimeEventBus::new();
        assert_eq!(bus.listener_count(), 0);

        // Should not panic with no listeners
        bus.emit(RuntimeEvent::RunStarted {
            run_id: "run-1".to_owned(),
            total_shots: 1,
        });
    }

    #[test]
    fn test_bus_with_builtin_listeners() {
        let mut bus = RuntimeEventBus::new();
        bus.register(Box::new(NoopRuntimeListener));
        bus.register(Box::new(LoggingRuntimeListener));
        assert_eq!(bus.listener_count(), 2);

        // Both listeners should handle events without panic
        bus.emit(RuntimeEvent::RunCompleted {
            run_id: "run-1".to_owned(),
            status: "completed".to_owned(),
            output_asset_id: Some("asset-1".to_owned()),
            duration_secs: 10.5,
            shot_success_count: 3,
            shot_total_count: 3,
        });
    }
}
