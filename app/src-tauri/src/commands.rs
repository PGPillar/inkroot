//! Tauri commands (the IPC contract's command table), app state and dependency wiring.
//!
//! Each `#[tauri::command]` is a thin wrapper over a function in [`inner`] that takes
//! `&AppState`, so the behaviour is testable without a Tauri runtime. Tauri 2 maps the JS
//! argument names to camelCase by default (`flow_id` <- `flowId`), which is what the contract
//! uses.

use crate::backup::BackupLibrary;
use crate::clock::SystemClock;
use crate::events::Emitter;
use crate::flows::{Deps, FlowEngine};
use crate::host::keep_awake::platform_keep_awake;
use crate::host::setup::platform_host_setup;
use crate::ipc::{
    AppInfo, BackupSummary, DeviceStatus, FlowError, FlowKind, FlowOptions, HostOs, Settings,
    SettingsPatch,
};
use crate::logs::LogStore;
use crate::poller::Poller;
use crate::profile::builtin_profiles;
use crate::settings::SettingsStore;
use crate::tools::adb::RealAdb;
use crate::tools::edl::RealEdl;
use crate::tools::usb::NusbProbe;
use crate::tools::{Timeouts, ToolPaths};
use std::sync::Arc;
use tauri::{AppHandle, Runtime, State};

pub struct AppState {
    pub engine: Arc<FlowEngine>,
    pub poller: Arc<Poller>,
    pub library: BackupLibrary,
    pub settings: Arc<SettingsStore>,
    pub log: Arc<LogStore>,
    pub info: AppInfo,
}

pub fn host_os() -> HostOs {
    if cfg!(target_os = "macos") {
        HostOs::Macos
    } else if cfg!(windows) {
        HostOs::Windows
    } else {
        HostOs::Linux
    }
}

/// The production dependencies. Never panics on missing tools: flows report `toolMissing`.
pub fn real_deps(tools: ToolPaths, emitter: Arc<dyn Emitter>) -> Deps {
    let clock = Arc::new(SystemClock::new());
    let log = Arc::new(LogStore::new(emitter.clone(), clock.clone()));
    let timeouts = Timeouts::default();
    let profiles = builtin_profiles();
    let (loader, part_size) = profiles
        .first()
        .map(|p| (tools.loader(&p.edl.loader), p.boot_size))
        .unwrap_or_default();
    Deps {
        adb: Arc::new(RealAdb {
            path: tools.adb(),
            log: log.clone(),
            timeouts: timeouts.clone(),
        }),
        edl: Arc::new(RealEdl {
            path: tools.edl(),
            loader,
            log: log.clone(),
            timeouts: timeouts.clone(),
            part_size,
        }),
        usb: Arc::new(NusbProbe),
        host: platform_host_setup(&tools),
        awake: platform_keep_awake(),
        clock,
        emitter,
        log,
        library: BackupLibrary::new(BackupLibrary::default_root()),
        settings: Arc::new(SettingsStore::new(SettingsStore::default_path())),
        profiles,
        tools,
        timeouts,
        poller: None,
        magisk_apk_sha256: crate::magisk::MAGISK_APK_SHA256.to_string(),
    }
}

/// Spawns the device poller over `deps` (interval `timeouts.poll`), hands it to the engine
/// and builds the managed state. The caller triggers the first `poll_once`.
pub fn build_state(mut deps: Deps, demo: bool) -> AppState {
    // The poller probes every 1.5 s; its adb output must never reach the flow log (it would
    // flood the UI and push a failed flow's lines out of the ring buffer).
    let poll_adb = deps.adb.quiet().unwrap_or_else(|| deps.adb.clone());
    let poller = Poller::spawn(
        poll_adb,
        deps.usb.clone(),
        deps.profiles.clone(),
        deps.emitter.clone(),
        deps.timeouts.poll,
    );
    deps.poller = Some(poller.clone());
    let library = BackupLibrary::new(deps.library.root.clone());
    let settings = deps.settings.clone();
    let log = deps.log.clone();
    let info = AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        demo,
        os: host_os(),
        backup_root: library.root.to_string_lossy().into_owned(),
    };
    AppState {
        engine: FlowEngine::new(deps),
        poller,
        library,
        settings,
        log,
        info,
    }
}

/// Review focus 5: the window may not close while a partition write is in progress.
pub fn should_block_close(engine: &FlowEngine) -> bool {
    engine.writing()
}

/// Close-request handler body: when blocked, re-emits the current `flow://state` so the UI
/// can explain why, and returns true (the caller then calls `prevent_close`).
pub fn guard_close(engine: &FlowEngine) -> bool {
    if !should_block_close(engine) {
        return false;
    }
    if let Some(s) = engine.current() {
        engine.deps().emitter.flow_state(&s);
    }
    true
}

/// App-quit handler body (`RunEvent::ExitRequested`): same rule as closing the window. The
/// caller calls `prevent_exit` when this returns true.
pub fn guard_exit(engine: &FlowEngine) -> bool {
    guard_close(engine)
}

/// Id of the app's own Quit menu item. On macOS the stock Quit item sends `terminate:`, which
/// ends the app without a preventable `ExitRequested`; ours calls `AppHandle::exit`, which
/// goes through `ExitRequested` and so through [`guard_exit`].
pub const QUIT_MENU_ID: &str = "inkroot-quit";

pub fn is_quit_request(menu_id: &str) -> bool {
    menu_id == QUIT_MENU_ID
}

/// The default macOS menu with its stock Quit item replaced by [`QUIT_MENU_ID`]
/// (same label and Cmd+Q accelerator).
#[cfg(target_os = "macos")]
pub fn app_menu<R: Runtime>(handle: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = Menu::default(handle)?;
    if let Some(MenuItemKind::Submenu(app_sub)) = menu.items()?.into_iter().next() {
        let items = app_sub.items()?;
        // The stock app submenu ends with the predefined Quit item.
        if let Some(last) = items.len().checked_sub(1)
            && matches!(items[last], MenuItemKind::Predefined(_))
        {
            app_sub.remove_at(last)?;
        }
        let name = handle.package_info().name.clone();
        app_sub.append(&MenuItem::with_id(
            handle,
            QUIT_MENU_ID,
            format!("Quit {name}"),
            true,
            Some("CmdOrCtrl+Q"),
        )?)?;
    } else {
        // Unexpected shape: still offer a guarded Quit.
        menu.append(&MenuItem::with_id(
            handle,
            QUIT_MENU_ID,
            "Quit",
            true,
            Some("CmdOrCtrl+Q"),
        )?)?;
    }
    Ok(menu)
}

pub mod inner {
    use super::*;
    use crate::ipc::ErrorCode;
    use std::path::PathBuf;

    const REVEAL_MESSAGE: &str = "Inkroot can only show folders inside your backups folder.";

    pub fn get_app_info(s: &AppState) -> Result<AppInfo, FlowError> {
        Ok(s.info.clone())
    }

    pub fn get_device(s: &AppState) -> Result<DeviceStatus, FlowError> {
        Ok(s.poller.latest())
    }

    pub fn list_backups(
        s: &AppState,
        serial: Option<String>,
    ) -> Result<Vec<BackupSummary>, FlowError> {
        Ok(s.library.list(serial.as_deref()))
    }

    pub fn start_flow(
        s: &AppState,
        kind: FlowKind,
        options: FlowOptions,
    ) -> Result<String, FlowError> {
        s.engine.start(kind, options)
    }

    pub fn answer_prompt(
        s: &AppState,
        flow_id: String,
        prompt_id: String,
        option_id: String,
    ) -> Result<(), FlowError> {
        s.engine.answer(&flow_id, &prompt_id, &option_id);
        Ok(())
    }

    pub fn cancel_flow(s: &AppState, flow_id: String) -> Result<(), FlowError> {
        s.engine.cancel(&flow_id)
    }

    pub fn get_log(s: &AppState, flow_id: Option<String>) -> Result<String, FlowError> {
        Ok(s.log.text(flow_id.as_deref()))
    }

    /// Validates that `path` (after canonicalisation) lies inside the backup root and returns
    /// the caller's path unchanged. The command wrapper does the actual reveal.
    pub fn reveal_path(s: &AppState, path: String) -> Result<PathBuf, FlowError> {
        let refuse = |detail: String| FlowError {
            code: ErrorCode::Internal,
            message: REVEAL_MESSAGE.into(),
            detail,
            suggested_action: None,
        };
        let root = std::fs::canonicalize(&s.library.root).map_err(|e| {
            refuse(format!(
                "backup root {} unavailable: {e}",
                s.library.root.display()
            ))
        })?;
        let p = std::fs::canonicalize(&path)
            .map_err(|e| refuse(format!("cannot resolve {path}: {e}")))?;
        if !p.starts_with(&root) {
            return Err(refuse(format!(
                "{} is outside the backup root {}",
                p.display(),
                root.display()
            )));
        }
        // The canonical form is only for the containment check: on Windows it is the
        // verbatim `\\?\C:\...` form, which Explorer may refuse.
        Ok(PathBuf::from(path))
    }

    pub fn get_settings(s: &AppState) -> Result<Settings, FlowError> {
        Ok(s.settings.get())
    }

    pub fn set_settings(s: &AppState, settings: SettingsPatch) -> Result<Settings, FlowError> {
        s.settings.apply(settings)
    }
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo, FlowError> {
    inner::get_app_info(&state)
}

#[tauri::command]
pub fn get_device(state: State<'_, AppState>) -> Result<DeviceStatus, FlowError> {
    inner::get_device(&state)
}

/// Async: Tauri runs it off the main thread (it reads manifests from disk).
#[tauri::command]
pub async fn list_backups(
    state: State<'_, AppState>,
    serial: Option<String>,
) -> Result<Vec<BackupSummary>, FlowError> {
    inner::list_backups(&state, serial)
}

/// Async: Tauri runs it off the main thread, because starting a flow pauses the poller, which
/// waits for an in-flight device probe.
#[tauri::command]
pub async fn start_flow(
    state: State<'_, AppState>,
    kind: FlowKind,
    options: FlowOptions,
) -> Result<String, FlowError> {
    inner::start_flow(&state, kind, options)
}

#[tauri::command]
pub fn answer_prompt(
    state: State<'_, AppState>,
    flow_id: String,
    prompt_id: String,
    option_id: String,
) -> Result<(), FlowError> {
    inner::answer_prompt(&state, flow_id, prompt_id, option_id)
}

#[tauri::command]
pub fn cancel_flow(state: State<'_, AppState>, flow_id: String) -> Result<(), FlowError> {
    inner::cancel_flow(&state, flow_id)
}

#[tauri::command]
pub fn get_log(state: State<'_, AppState>, flow_id: Option<String>) -> Result<String, FlowError> {
    inner::get_log(&state, flow_id)
}

#[tauri::command]
pub fn reveal_path<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), FlowError> {
    use tauri_plugin_opener::OpenerExt;
    let p = inner::reveal_path(&state, path)?;
    app.opener().reveal_item_in_dir(&p).map_err(|e| FlowError {
        code: crate::ipc::ErrorCode::Internal,
        message: "Inkroot couldn't open that folder. Please try again.".into(),
        detail: format!("reveal {} failed: {e}", p.display()),
        suggested_action: None,
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, FlowError> {
    inner::get_settings(&state)
}

#[tauri::command]
pub fn set_settings(
    state: State<'_, AppState>,
    settings: SettingsPatch,
) -> Result<Settings, FlowError> {
    inner::set_settings(&state, settings)
}

/// The invoke handler registering every contract command.
pub fn invoke_handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        get_app_info,
        get_device,
        list_backups,
        start_flow,
        answer_prompt,
        cancel_flow,
        get_log,
        reveal_path,
        get_settings,
        set_settings
    ]
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ipc::{FlowState, LogLine, Progress};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountLogs(AtomicUsize, Mutex<Vec<String>>);
    impl Emitter for CountLogs {
        fn device(&self, _: &DeviceStatus) {}
        fn flow_state(&self, _: &FlowState) {}
        fn progress(&self, _: &Progress) {}
        fn log(&self, l: &LogLine) {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.lock().unwrap().push(l.line.clone());
        }
    }

    /// A stand-in `adb` that answers like a chatty device: `getprop` prints many lines.
    const FAKE_ADB: &str = r#"#!/bin/sh
case "$*" in
  devices) printf 'List of devices attached\nNMX1\tdevice\n' ;;
  *getprop*) i=0; while [ $i -lt 200 ]; do echo "[ro.fake.$i]: [v]"; i=$((i+1)); done ;;
  *) echo "chatter"; exit 1 ;;
esac
"#;

    #[test]
    fn idle_polling_never_touches_the_flow_log() {
        use std::os::unix::fs::PermissionsExt;
        let _g = crate::env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let tools = ToolPaths {
            root: tmp.path().join("res"),
        };
        std::fs::create_dir_all(tools.adb().parent().unwrap()).unwrap();
        std::fs::write(tools.adb(), FAKE_ADB).unwrap();
        std::fs::set_permissions(tools.adb(), std::fs::Permissions::from_mode(0o755)).unwrap();
        unsafe {
            std::env::set_var("INKROOT_BACKUP_DIR", tmp.path().join("backups"));
            std::env::set_var("INKROOT_CONFIG_DIR", tmp.path().join("config"));
        }
        let emitter = Arc::new(CountLogs::default());
        let deps = real_deps(tools, emitter.clone());
        unsafe {
            std::env::remove_var("INKROOT_BACKUP_DIR");
            std::env::remove_var("INKROOT_CONFIG_DIR");
        }
        let state = build_state(deps, false);
        state.poller.poll_once();
        state.poller.stop();
        // The probe really ran the stand-in adb (it found an authorized device)...
        assert!(
            matches!(state.poller.latest(), DeviceStatus::Adb { .. }),
            "{:?}",
            state.poller.latest()
        );
        // ...and nothing reached the log store or the UI.
        assert_eq!(state.log.text(None), "");
        assert_eq!(emitter.0.load(Ordering::SeqCst), 0);

        // Inside a flow, the device probe still logs adb chatter, but not getprop's raw
        // property dump.
        let d = state.engine.deps();
        let _ = crate::device::probe(d.adb.as_ref(), d.usb.as_ref(), &d.profiles, &d.timeouts);
        let text = state.log.text(None);
        assert!(text.contains("chatter"), "{text}");
        assert!(!text.contains("[ro.fake."), "{text}");
    }
}
