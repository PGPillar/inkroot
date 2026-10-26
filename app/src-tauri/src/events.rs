//! Event names and the emitter abstraction shared by the backend and the UI bridge.

use crate::ipc::{DeviceStatus, FlowState, LogLine, Progress};

pub const EVT_DEVICE: &str = "device://status";
pub const EVT_FLOW_STATE: &str = "flow://state";
pub const EVT_PROGRESS: &str = "flow://progress";
pub const EVT_LOG: &str = "flow://log";

/// Sink for backend events. The Tauri implementation lives elsewhere; tests use fakes.
pub trait Emitter: Send + Sync {
    fn device(&self, s: &DeviceStatus);
    fn flow_state(&self, s: &FlowState);
    fn progress(&self, p: &Progress);
    fn log(&self, l: &LogLine);
}

/// Forwards backend events to the webview. Emit errors are ignored (no window yet, or
/// closing).
pub struct TauriEmitter<R: tauri::Runtime = tauri::Wry>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> Emitter for TauriEmitter<R> {
    fn device(&self, s: &DeviceStatus) {
        let _ = tauri::Emitter::emit(&self.0, EVT_DEVICE, s);
    }
    fn flow_state(&self, s: &FlowState) {
        let _ = tauri::Emitter::emit(&self.0, EVT_FLOW_STATE, s);
    }
    fn progress(&self, p: &Progress) {
        let _ = tauri::Emitter::emit(&self.0, EVT_PROGRESS, p);
    }
    fn log(&self, l: &LogLine) {
        let _ = tauri::Emitter::emit(&self.0, EVT_LOG, l);
    }
}
