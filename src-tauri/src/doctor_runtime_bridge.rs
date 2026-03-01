use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::runtime::types::RuntimeEvent;

pub fn map_runtime_event_name(event: &RuntimeEvent) -> &'static str {
    match event {
        RuntimeEvent::ChatDelta { .. } => "doctor:chat-delta",
        RuntimeEvent::ChatFinal { .. } => "doctor:chat-final",
        RuntimeEvent::Invoke { .. } => "doctor:invoke",
        RuntimeEvent::Error { .. } => "doctor:error",
        RuntimeEvent::Status { .. } => "doctor:status",
    }
}

pub fn emit_runtime_event(app: &AppHandle, event: RuntimeEvent) {
    let name = map_runtime_event_name(&event);
    match event {
        RuntimeEvent::ChatDelta { text } => {
            let _ = app.emit(name, json!({ "text": text }));
        }
        RuntimeEvent::ChatFinal { text } => {
            let _ = app.emit(name, json!({ "text": text }));
        }
        RuntimeEvent::Invoke { payload } => {
            let _ = app.emit(name, payload);
        }
        RuntimeEvent::Error { error } => {
            let _ = app.emit(
                name,
                json!({
                    "code": error.code.as_str(),
                    "message": error.message,
                    "actionHint": error.action_hint,
                }),
            );
        }
        RuntimeEvent::Status { text } => {
            let _ = app.emit(name, json!({ "text": text }));
        }
    }
}
