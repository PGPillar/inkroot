pub mod backup;
pub mod clock;
pub mod commands;
#[cfg(any(test, feature = "demo"))]
pub mod demo;
pub mod device;
pub mod events;
#[cfg(any(test, feature = "demo"))]
pub mod fakes;
pub mod flows;
pub mod host;
pub mod image;
pub mod ipc;
pub mod logs;
pub mod magisk;
pub mod poller;
pub mod profile;
pub mod settings;
pub mod tools;

#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Demo deps (`INKROOT_DEMO=1`, scenario from `INKROOT_DEMO_SCENARIO`, speed from
/// `INKROOT_DEMO_SPEED`, default 1.0). `None` unless demo mode is enabled.
#[cfg(feature = "demo")]
fn demo_deps_from_env(emitter: std::sync::Arc<dyn events::Emitter>) -> Option<flows::Deps> {
    if !demo::demo_enabled() {
        return None;
    }
    let speed = std::env::var("INKROOT_DEMO_SPEED")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(1.0);
    Some(demo::demo_deps(
        demo::DemoScenario::from_env(),
        emitter,
        speed,
    ))
}

#[cfg(not(feature = "demo"))]
fn demo_deps_from_env(_: std::sync::Arc<dyn events::Emitter>) -> Option<flows::Deps> {
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use std::sync::Arc;
    use tauri::Manager;

    let builder = tauri::Builder::default();
    // Must be the first plugin: a second launch focuses the running window and exits.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(commands::invoke_handler());
    // macOS: replace the stock Quit (which terminates without a preventable exit request)
    // with one that goes through `ExitRequested` and the write guard.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(commands::app_menu);
    builder
        .on_menu_event(|app, event| {
            if commands::is_quit_request(event.id().as_ref()) {
                app.exit(0);
            }
        })
        .setup(|app| {
            let emitter: Arc<dyn events::Emitter> =
                Arc::new(events::TauriEmitter(app.handle().clone()));
            let (deps, demo) = match demo_deps_from_env(emitter.clone()) {
                Some(d) => (d, true),
                None => {
                    let tools = tools::ToolPaths::resolve(app.path().resource_dir().ok());
                    (commands::real_deps(tools, emitter), false)
                }
            };
            let state = commands::build_state(deps, demo);
            let poller = state.poller.clone();
            std::thread::spawn(move || poller.poll_once());
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && let Some(state) = window.try_state::<commands::AppState>()
                && commands::guard_close(&state.engine)
            {
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Inkroot")
        .run(|app, event| {
            // Review focus 5: quitting (app.exit, the Quit menu item, last window gone) is
            // refused while a partition write is in progress.
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && let Some(state) = app.try_state::<commands::AppState>()
                && commands::guard_exit(&state.engine)
            {
                api.prevent_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    #[test]
    fn conf_identity() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], "io.github.n0dal.inkroot");
        assert_eq!(conf["productName"], "Inkroot");
        let w = &conf["app"]["windows"][0];
        assert_eq!(w["width"], 880);
        assert_eq!(w["height"], 640);
        assert_eq!(w["minWidth"], 760);
        assert_eq!(w["minHeight"], 560);
        assert_eq!(conf["bundle"]["resources"], json!(["resources/**/*"]));
    }
}
