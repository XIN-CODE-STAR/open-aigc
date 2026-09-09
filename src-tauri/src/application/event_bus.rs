#![allow(dead_code)]
//! 统一 EventBus：跨域事件广播基础设施。
//!
//! 将 3 套独立事件系统统一为单一总线：
//! - AgentEvent（Agent 对话事件）
//! - RuntimeEvent（创作流水线事件）
//! - TaskEvent（任务生命周期事件）
//! - WorkflowEvent（工作流阶段事件）
//!
//! 设计原则：
//! - 单一 EventBus 实例，所有 subsystem 通过 Arc<EventBus> 共享
//! - Listener 按需注册（Tauri Emitter、SQLite 持久化、日志、未来：WebSocket）
//! - 单个 Listener 失败不阻塞其他 Listener
//! - 同步广播（当前阶段），未来可扩展为异步

use std::sync::RwLock;

use serde::Serialize;
use tauri::Emitter;

use crate::domain::runtime_event::RuntimeEvent;
use crate::domain::task::TaskEvent;

// ─── DomainEvent ───

/// 统一领域事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum DomainEvent {
    /// Agent 对话事件。
    Agent(AgentEventWrapper),
    /// 创作流水线事件。
    Runtime(RuntimeEvent),
    /// 任务生命周期事件。
    Task(TaskEvent),
}

// ─── AgentEventWrapper ───

/// Agent 事件包装（保持与现有 AgentEvent 的兼容性）。
///
/// AgentEvent 定义在 agent_service.rs 中，此处用 serde_json::Value 包装
/// 避免循环依赖。EventBus 透传 JSON，由 Tauri Emitter 直接发送原始 AgentEvent。
#[derive(Debug, Clone, Serialize)]
pub struct AgentEventWrapper {
    /// 原始 AgentEvent JSON。
    pub inner: serde_json::Value,
}

// ─── EventListener ───

/// 事件监听器 trait。
///
/// 实现方负责自行处理错误。on_event() 不应 panic。
pub trait EventListener: Send + Sync {
    /// 接收事件通知。
    fn on_event(&self, event: &DomainEvent);

    /// 监听器名称（日志/调试用）。
    fn name(&self) -> &str {
        "unnamed"
    }
}

// ─── EventBus ───

/// 统一事件总线。
///
/// 聚合多个 EventListener，统一广播。
/// 线程安全：通过 RwLock 保护 Listener 列表。
pub struct EventBus {
    listeners: RwLock<Vec<Box<dyn EventListener>>>,
}

impl EventBus {
    /// 创建空事件总线。
    pub fn new() -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// 注册一个事件监听器。
    pub fn register(&self, listener: Box<dyn EventListener>) {
        let mut listeners = self.listeners.write().unwrap_or_else(|e| e.into_inner());
        listeners.push(listener);
    }

    /// 广播领域事件给所有已注册的监听器。
    ///
    /// 单个 listener 失败不会影响其他 listener。
    pub fn emit(&self, event: DomainEvent) {
        let listeners = self.listeners.read().unwrap_or_else(|e| e.into_inner());
        for listener in listeners.iter() {
            listener.on_event(&event);
        }
    }

    /// 便捷方法：发射 RuntimeEvent。
    pub fn emit_runtime(&self, event: RuntimeEvent) {
        self.emit(DomainEvent::Runtime(event));
    }

    /// 便捷方法：发射 TaskEvent。
    pub fn emit_task(&self, event: TaskEvent) {
        self.emit(DomainEvent::Task(event));
    }

    /// 便捷方法：发射 Agent 事件（接受已序列化的 AgentEvent JSON）。
    pub fn emit_agent_json(&self, agent_event_json: serde_json::Value) {
        self.emit(DomainEvent::Agent(AgentEventWrapper {
            inner: agent_event_json,
        }));
    }

    /// 已注册监听器数量。
    pub fn listener_count(&self) -> usize {
        let listeners = self.listeners.read().unwrap_or_else(|e| e.into_inner());
        listeners.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Built-in Listeners ───

/// 日志监听器（开发调试用）。
pub struct LoggingEventListener;

impl EventListener for LoggingEventListener {
    fn on_event(&self, event: &DomainEvent) {
        match event {
            DomainEvent::Agent(_) => {
                eprintln!("[EventBus] Agent event emitted");
            }
            DomainEvent::Runtime(rt) => {
                eprintln!("[EventBus] Runtime event: {:?}", rt.event_type());
            }
            DomainEvent::Task(task) => {
                eprintln!("[EventBus] Task event: task_id={}", task.task_id());
            }
        }
    }

    fn name(&self) -> &str {
        "logging"
    }
}

/// 无操作监听器（测试用）。
pub struct NoopEventListener;

impl EventListener for NoopEventListener {
    fn on_event(&self, _event: &DomainEvent) {}

    fn name(&self) -> &str {
        "noop"
    }
}

/// Tauri 事件发射器（通过 app.emit() 推送到前端）。
pub struct TauriEventEmitter {
    app_handle: tauri::AppHandle,
}

impl TauriEventEmitter {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl EventListener for TauriEventEmitter {
    fn on_event(&self, event: &DomainEvent) {
        match event {
            // Agent 事件：直接发射到 agent://event 频道（与现有行为兼容）
            DomainEvent::Agent(_) => {
                // AgentEvent 已经由 AgentService 直接通过 app.emit() 发射，
                // EventBus 不重复发射，避免前端收到重复事件。
            }
            // Runtime 事件：发射到 runtime://event 频道
            DomainEvent::Runtime(rt) => {
                if let Err(e) = self.app_handle.emit("runtime://event", rt) {
                    eprintln!("[EventBus] Failed to emit runtime event: {e}");
                }
            }
            // Task 事件：发射到 task://event 频道
            DomainEvent::Task(task) => {
                if let Err(e) = self.app_handle.emit("task://event", task) {
                    eprintln!("[EventBus] Failed to emit task event: {e}");
                }
            }
        }
    }

    fn name(&self) -> &str {
        "tauri_emitter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::TaskKind;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct CountingListener {
        count: Arc<AtomicU32>,
    }

    impl EventListener for CountingListener {
        fn on_event(&self, _event: &DomainEvent) {
            self.count.fetch_add(1, Ordering::Relaxed);
        }
        fn name(&self) -> &str {
            "counting"
        }
    }

    #[test]
    fn test_bus_emits_to_all_listeners() {
        let counter = Arc::new(AtomicU32::new(0));
        let bus = EventBus::new();
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));

        bus.emit_task(TaskEvent::Created {
            task_id: "task-001".to_owned(),
            kind: TaskKind::ImageGeneration,
        });

        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_bus_empty_listeners() {
        let bus = EventBus::new();
        assert_eq!(bus.listener_count(), 0);

        // Should not panic with no listeners
        bus.emit_task(TaskEvent::Created {
            task_id: "task-001".to_owned(),
            kind: TaskKind::ImageGeneration,
        });
    }

    #[test]
    fn test_bus_register_and_count() {
        let bus = EventBus::new();
        assert_eq!(bus.listener_count(), 0);

        bus.register(Box::new(NoopEventListener));
        assert_eq!(bus.listener_count(), 1);

        bus.register(Box::new(LoggingEventListener));
        assert_eq!(bus.listener_count(), 2);
    }

    #[test]
    fn test_bus_multiple_events() {
        let counter = Arc::new(AtomicU32::new(0));
        let bus = EventBus::new();
        bus.register(Box::new(CountingListener {
            count: Arc::clone(&counter),
        }));

        bus.emit_task(TaskEvent::Created {
            task_id: "task-001".to_owned(),
            kind: TaskKind::ImageGeneration,
        });
        bus.emit_task(TaskEvent::Started {
            task_id: "task-001".to_owned(),
        });
        bus.emit_task(TaskEvent::Completed {
            task_id: "task-001".to_owned(),
            artifact_ids: vec!["art-001".to_owned()],
            duration_secs: 10.0,
        });

        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }
}
