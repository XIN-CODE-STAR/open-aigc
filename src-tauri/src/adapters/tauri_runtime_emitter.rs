#![allow(dead_code)]
//! Tauri Runtime Emitter：将 RuntimeEvent 推送到前端 runtime://event 频道。
//!
//! 实现 RuntimeEventListener trait，持有 AppHandle clone，
//! 在 on_event() 中序列化事件并通过 Tauri emit 发送。
//!
//! 频道：runtime://event（与 agent://event 分离，不混用）

use tauri::{AppHandle, Emitter};

use crate::domain::runtime_event::RuntimeEvent;
use crate::ports::runtime_event_emitter::RuntimeEventListener;

/// Tauri 运行时事件适配器。
///
/// 将 RuntimeEvent 序列化后通过 `runtime://event` 频道推送到前端。
/// emit 失败仅 log，不向上传播。
pub struct TauriRuntimeEmitter {
    app_handle: AppHandle,
}

impl TauriRuntimeEmitter {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

impl RuntimeEventListener for TauriRuntimeEmitter {
    fn on_event(&self, event: &RuntimeEvent) {
        let event_type = event.event_type();
        let run_id = event.run_id();

        match self.app_handle.emit("runtime://event", event) {
            Ok(()) => {
                eprintln!("[TauriRuntimeEmitter] emitted: type={event_type}, run_id={run_id}");
            }
            Err(e) => {
                eprintln!(
                    "[TauriRuntimeEmitter] emit failed: type={event_type}, run_id={run_id}, error={e}"
                );
            }
        }
    }
}
