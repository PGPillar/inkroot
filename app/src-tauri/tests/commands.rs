mod support;

use inkroot_lib::commands::{
    AppState, QUIT_MENU_ID, build_state, guard_close, guard_exit, inner, is_quit_request,
    should_block_close,
};
use inkroot_lib::demo::{DEMO_SERIAL, DemoScenario, demo_deps};
use inkroot_lib::flows::{BUSY_MESSAGE, FlowEngine, FlowFn, WRITING_MESSAGE};
use inkroot_lib::ipc::{
    ErrorCode, FlowKind, FlowOptions, FlowResult, FlowState, FlowStatus, PromptKind, SettingsPatch,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::RecordingEmitter;

fn demo_state(scenario: DemoScenario) -> (AppState, RecordingEmitter) {
    let rec = RecordingEmitter::default();
    let deps = demo_deps(scenario, Arc::new(rec.clone()), 0.0);
    (build_state(deps, true), rec)
}

fn wait_for(engine: &FlowEngine, id: &str, pred: impl Fn(&FlowState) -> bool) -> FlowState {
    let start = Instant::now();
    loop {
        if let Some(s) = engine.current().filter(|s| s.flow_id == id)
            && pred(&s)
        {
            return s;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn terminal(s: &FlowState) -> bool {
    matches!(
        s.status,
        FlowStatus::Done | FlowStatus::Failed | FlowStatus::Cancelled
    )
}

/// Marks a write in progress and holds it there until the prompt is answered.
fn writing_flow() -> FlowFn {
    Box::new(|ctx| {
        ctx.set_writing(true);
        let r = ctx.ask(PromptKind::RetryWrite);
        ctx.set_writing(false);
        r?;
        Ok(FlowResult::default())
    })
}

/// Waits on a prompt (no write) until answered or cancelled.
fn prompt_flow() -> FlowFn {
    Box::new(|ctx| {
        ctx.ask(PromptKind::EdlNotFound)?;
        Ok(FlowResult::default())
    })
}

#[test]
fn close_blocked_while_writing() {
    let (state, rec) = demo_state(DemoScenario::Fresh);
    assert!(!should_block_close(&state.engine));
    assert!(!guard_close(&state.engine));

    let id = state
        .engine
        .start_with(FlowKind::Backup, FlowOptions::default(), writing_flow())
        .unwrap();
    let s = wait_for(&state.engine, &id, |s| {
        s.writing && s.status == FlowStatus::WaitingForUser
    });
    assert!(should_block_close(&state.engine));

    // The guard re-emits the current snapshot so the UI shows why close was refused.
    let before = rec.states().len();
    assert!(guard_close(&state.engine));
    let states = rec.states();
    assert_eq!(states.len(), before + 1);
    assert_eq!(states.last().unwrap(), &s);

    // Quitting the app (RunEvent::ExitRequested, incl. the macOS Quit menu item) is blocked
    // the same way, and also re-emits.
    assert!(guard_exit(&state.engine));
    assert_eq!(rec.states().len(), before + 2);
    assert_eq!(rec.states().last().unwrap(), &s);

    // Cancel is refused mid-write too.
    let err = inner::cancel_flow(&state, id.clone()).unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
    assert_eq!(err.message, WRITING_MESSAGE);

    let prompt = s.prompt.unwrap();
    inner::answer_prompt(&state, id.clone(), prompt.id, "cancel".into()).unwrap();
    let end = wait_for(&state.engine, &id, terminal);
    assert!(!end.writing);
    assert!(!should_block_close(&state.engine));
    assert!(!guard_close(&state.engine));
    assert!(!guard_exit(&state.engine));
}

#[test]
fn quit_menu_item_routes_through_the_exit_guard() {
    assert!(is_quit_request(QUIT_MENU_ID));
    assert!(!is_quit_request("quit"));
    assert!(!is_quit_request(""));
}

#[test]
fn start_flow_busy_maps_to_flow_error_json() {
    let (state, _rec) = demo_state(DemoScenario::Fresh);
    let id = state
        .engine
        .start_with(FlowKind::Backup, FlowOptions::default(), prompt_flow())
        .unwrap();
    wait_for(&state.engine, &id, |s| {
        s.status == FlowStatus::WaitingForUser
    });

    let err = inner::start_flow(&state, FlowKind::Backup, FlowOptions::default()).unwrap_err();
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({
            "code": "busy",
            "message": BUSY_MESSAGE,
            "detail": "another flow is active",
            "suggestedAction": null
        })
    );

    inner::cancel_flow(&state, id.clone()).unwrap();
    let end = wait_for(&state.engine, &id, terminal);
    assert_eq!(end.status, FlowStatus::Cancelled);
    // Idle again: a real flow starts and returns its id.
    let id2 = inner::start_flow(&state, FlowKind::Backup, FlowOptions::default()).unwrap();
    assert!(!id2.is_empty() && id2 != id);
    wait_for(&state.engine, &id2, terminal);
}

#[test]
fn list_backups_filters_by_serial_newest_first() {
    let (state, _rec) = demo_state(DemoScenario::Rooted);
    let seeded = inner::list_backups(&state, None).unwrap();
    assert_eq!(seeded.len(), 1);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&seeded[0].path).join("manifest.json"))
            .unwrap(),
    )
    .unwrap();
    let add = |serial: &str, folder: &str, created: &str| {
        let dir = state.library.root.join(serial).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = manifest.clone();
        m["serial"] = json!(serial);
        m["createdAt"] = json!(created);
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
    };
    add(DEMO_SERIAL, "2030-01-01_1000", "2030-01-01T10:00:00+00:00");
    add("OTHER0001", "2031-01-01_1000", "2031-01-01T10:00:00+00:00");

    let mine = inner::list_backups(&state, Some(DEMO_SERIAL.into())).unwrap();
    assert_eq!(mine.len(), 2);
    assert!(mine.iter().all(|b| b.serial == DEMO_SERIAL));
    assert_eq!(mine[0].id, format!("{DEMO_SERIAL}/2030-01-01_1000"));
    assert_eq!(mine[1].id, seeded[0].id);

    let all = inner::list_backups(&state, None).unwrap();
    let ids: Vec<&str> = all.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "OTHER0001/2031-01-01_1000",
            &format!("{DEMO_SERIAL}/2030-01-01_1000"),
            seeded[0].id.as_str()
        ]
    );
    assert!(
        inner::list_backups(&state, Some("NOPE".into()))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reveal_path_outside_backup_root_refused() {
    let (state, _rec) = demo_state(DemoScenario::Rooted);
    let outside = tempfile::tempdir().unwrap();
    let escape = state.library.root.join("..").join("tools");
    let missing = state.library.root.join("does-not-exist");
    for p in [
        outside.path().to_path_buf(),
        escape,
        missing,
        std::path::PathBuf::from("/"),
    ] {
        let err = inner::reveal_path(&state, p.to_string_lossy().into_owned()).unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal, "{p:?}");
        assert!(!err.message.contains('/'), "plain message: {}", err.message);
        assert!(err.suggested_action.is_none());
    }

    let backup = &inner::list_backups(&state, None).unwrap()[0];
    // The path handed to the opener is the caller's own (not the canonical form, which is
    // `\\?\C:\...` on Windows).
    let ok = inner::reveal_path(&state, backup.path.clone()).unwrap();
    assert_eq!(ok, std::path::PathBuf::from(&backup.path));
    let dotted = std::path::Path::new(&backup.path)
        .join("..")
        .join(std::path::Path::new(&backup.path).file_name().unwrap());
    let ok = inner::reveal_path(&state, dotted.to_string_lossy().into_owned()).unwrap();
    assert_eq!(ok, dotted);
    // The root itself is allowed too.
    let root = state.library.root.to_string_lossy().into_owned();
    assert!(inner::reveal_path(&state, root).is_ok());
}

#[test]
fn set_settings_partial_patch() {
    let (state, _rec) = demo_state(DemoScenario::Fresh);
    let patch = |v: serde_json::Value| -> SettingsPatch { serde_json::from_value(v).unwrap() };

    let s = inner::set_settings(&state, patch(json!({ "hasRescueCable": true }))).unwrap();
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        json!({ "hasRescueCable": true, "acceptedRiskVersion": null })
    );
    let s = inner::set_settings(&state, patch(json!({ "acceptedRiskVersion": 1 }))).unwrap();
    assert_eq!(s.has_rescue_cable, Some(true));
    assert_eq!(s.accepted_risk_version, Some(1));
    let s = inner::set_settings(&state, patch(json!({ "hasRescueCable": null }))).unwrap();
    assert_eq!(s.has_rescue_cable, None);
    assert_eq!(s.accepted_risk_version, Some(1));
    assert_eq!(inner::get_settings(&state).unwrap(), s);
}

#[test]
fn app_info_reports_demo_and_os() {
    let rec = RecordingEmitter::default();
    let deps = demo_deps(DemoScenario::Fresh, Arc::new(rec), 0.0);
    let root = deps.library.root.clone();
    let state = build_state(deps, true);
    let info = inner::get_app_info(&state).unwrap();
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    assert_eq!(
        serde_json::to_value(&info).unwrap(),
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "demo": true,
            "os": os,
            "backupRoot": root.to_string_lossy(),
        })
    );
    assert_eq!(state.library.root, root);
    // get_device is the poller's latest status (nothing polled yet -> none).
    assert_eq!(
        serde_json::to_value(inner::get_device(&state).unwrap()).unwrap(),
        json!({ "kind": "none" })
    );
    state.poller.poll_once();
    let dev = serde_json::to_value(inner::get_device(&state).unwrap()).unwrap();
    assert_eq!(dev["kind"], "adb");
    assert_eq!(dev["info"]["displayName"], "Note Max");
}

/// Drives the real `#[tauri::command]` wrappers through Tauri's mock IPC, proving the JS
/// argument names are the contract's camelCase ones.
#[test]
fn ipc_arg_names_are_camel_case() {
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;

    let rec = RecordingEmitter::default();
    let state = build_state(demo_deps(DemoScenario::Fresh, Arc::new(rec), 0.0), true);
    let app = mock_builder()
        .invoke_handler(inkroot_lib::commands::invoke_handler())
        .manage(state)
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |cmd: &str, body: serde_json::Value| {
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: if cfg!(windows) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| b.deserialize::<serde_json::Value>().unwrap())
    };

    assert_eq!(call("get_app_info", json!({})).unwrap()["demo"], true);
    assert_eq!(call("get_device", json!({})).unwrap()["kind"], "none");
    assert_eq!(call("list_backups", json!({})).unwrap(), json!([]));
    assert_eq!(
        call("list_backups", json!({ "serial": DEMO_SERIAL })).unwrap(),
        json!([])
    );
    assert_eq!(
        call("cancel_flow", json!({ "flowId": "nope" })).unwrap(),
        json!(null)
    );
    assert!(call("cancel_flow", json!({ "flow_id": "nope" })).is_err());
    assert_eq!(
        call(
            "answer_prompt",
            json!({ "flowId": "nope", "promptId": "p", "optionId": "o" })
        )
        .unwrap(),
        json!(null)
    );
    assert!(call("get_log", json!({})).unwrap().is_string());
    assert!(
        call("get_log", json!({ "flowId": "nope" }))
            .unwrap()
            .is_string()
    );
    assert_eq!(
        call(
            "set_settings",
            json!({ "settings": { "hasRescueCable": false } })
        )
        .unwrap()["hasRescueCable"],
        false
    );
    assert_eq!(
        call("get_settings", json!({})).unwrap()["hasRescueCable"],
        false
    );
    // Errors reach JS as the contract's FlowError object.
    let err = call(
        "reveal_path",
        json!({ "path": std::env::temp_dir().to_string_lossy() }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "internal");
    assert!(err.get("suggestedAction").is_some());
    let err = call("start_flow", json!({ "kind": "root", "options": {} })).unwrap_err();
    assert_eq!(err["code"], "internal");
    assert_eq!(
        err["message"],
        "Please read and accept the risk notice first."
    );
}
