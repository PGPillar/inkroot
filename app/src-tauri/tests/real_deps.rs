//! Real dependency wiring. Kept in its own test binary because it mutates env vars: the only
//! test here, so no other thread reads the environment concurrently.

use inkroot_lib::commands::{build_state, inner, real_deps};
use inkroot_lib::events::Emitter;
use inkroot_lib::ipc::{
    DeviceStatus, ErrorCode, FlowKind, FlowOptions, FlowState, LogLine, Progress,
};
use inkroot_lib::tools::ToolPaths;
use std::sync::Arc;
use std::time::Duration;

struct Quiet;
impl Emitter for Quiet {
    fn device(&self, _: &DeviceStatus) {}
    fn flow_state(&self, _: &FlowState) {}
    fn progress(&self, _: &Progress) {}
    fn log(&self, _: &LogLine) {}
}

#[test]
fn real_deps_carry_pinned_magisk_sha_and_tolerate_missing_tools() {
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: the only test in this binary; nothing else touches the environment.
    unsafe {
        std::env::set_var("INKROOT_BACKUP_DIR", tmp.path().join("backups"));
        std::env::set_var("INKROOT_CONFIG_DIR", tmp.path().join("config"));
    }
    let tools = ToolPaths {
        root: tmp.path().join("no-tools-here"),
    };
    let deps = real_deps(tools, Arc::new(Quiet));
    unsafe {
        std::env::remove_var("INKROOT_BACKUP_DIR");
        std::env::remove_var("INKROOT_CONFIG_DIR");
    }
    assert_eq!(
        deps.magisk_apk_sha256,
        inkroot_lib::magisk::MAGISK_APK_SHA256
    );
    assert_eq!(deps.library.root, tmp.path().join("backups"));
    assert!(!deps.profiles.is_empty());
    assert_eq!(deps.timeouts.poll, Duration::from_millis(1500));

    // Missing tools don't crash anything; a flow reports toolMissing.
    let state = build_state(deps, false);
    assert!(!state.info.demo);
    let err = inner::start_flow(&state, FlowKind::Backup, FlowOptions::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::ToolMissing);
}
