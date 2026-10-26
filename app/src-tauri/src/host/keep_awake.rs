//! Keep the computer awake while a flow runs. Never fails a flow.

use std::any::Any;
use std::sync::Arc;

/// The guard is created and dropped on the flow thread.
pub trait KeepAwake: Send + Sync {
    fn hold(&self, why: &str) -> Box<dyn Any>;
}

pub fn platform_keep_awake() -> Arc<dyn KeepAwake> {
    Arc::new(PlatformAwake)
}

struct PlatformAwake;

/// Kills and reaps a helper child on drop.
#[cfg(unix)]
struct ChildGuard(std::process::Child);

#[cfg(unix)]
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
fn spawn_guard(mut cmd: std::process::Command) -> Box<dyn Any> {
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(child) => Box::new(ChildGuard(child)),
        Err(_) => Box::new(()),
    }
}

#[cfg(target_os = "macos")]
impl KeepAwake for PlatformAwake {
    fn hold(&self, _why: &str) -> Box<dyn Any> {
        let mut cmd = std::process::Command::new("caffeinate");
        cmd.arg("-i").arg("-w").arg(std::process::id().to_string());
        spawn_guard(cmd)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl KeepAwake for PlatformAwake {
    fn hold(&self, why: &str) -> Box<dyn Any> {
        let mut cmd = std::process::Command::new("systemd-inhibit");
        cmd.arg("--what=sleep:idle")
            .arg("--who=Inkroot")
            .arg(format!("--why={why}"))
            .arg("sleep")
            .arg("infinity");
        // Not on PATH -> spawn fails -> no-op guard.
        spawn_guard(cmd)
    }
}

#[cfg(windows)]
struct WinGuard;

#[cfg(windows)]
impl Drop for WinGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Power::{ES_CONTINUOUS, SetThreadExecutionState};
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

#[cfg(windows)]
impl KeepAwake for PlatformAwake {
    fn hold(&self, _why: &str) -> Box<dyn Any> {
        use windows_sys::Win32::System::Power::{
            ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
        };
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
        Box::new(WinGuard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_hold_returns_droppable_guard() {
        let k = platform_keep_awake();
        let g = k.hold("test");
        drop(g);
    }
}
