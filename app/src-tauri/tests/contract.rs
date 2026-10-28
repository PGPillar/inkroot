use inkroot_lib::events::*;
use inkroot_lib::ipc::*;
use serde_json::{Value, json};

fn info() -> DeviceInfo {
    DeviceInfo {
        serial: "S1".into(),
        model: "NoteMax".into(),
        profile_id: Some("note-max".into()),
        display_name: "Note Max".into(),
        build_id: "B".into(),
        display_id: "D".into(),
        incremental: "I".into(),
        firmware_label: "4.2".into(),
        slot: Slot::B,
        battery: 80,
        rooted: true,
        magisk_version: Some("30.7".into()),
        boot_service_running: false,
    }
}

fn backup() -> BackupSummary {
    BackupSummary {
        id: "S1/2026-10-25_1200".into(),
        serial: "S1".into(),
        created_at: "2026-10-25T12:00:00Z".into(),
        model: "NoteMax".into(),
        firmware: "D".into(),
        slot: Slot::A,
        stock: true,
        verified: true,
        source: BackupSource::Inkroot,
        path: "/b".into(),
        files: vec![BackupFile {
            name: "boot_a.img".into(),
            size: 10,
            sha256: "ab".into(),
        }],
    }
}

fn ser<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap()
}

#[test]
fn device_status_adb_shape() {
    let v = ser(&DeviceStatus::Adb {
        info: info(),
        gate: None,
    });
    assert_eq!(v["kind"], "adb");
    assert_eq!(v["gate"], Value::Null);
    assert!(v.as_object().unwrap().contains_key("gate"));
    let i = &v["info"];
    assert_eq!(i["profileId"], "note-max");
    assert_eq!(i["displayName"], "Note Max");
    assert_eq!(i["buildId"], "B");
    assert_eq!(i["displayId"], "D");
    assert_eq!(i["firmwareLabel"], "4.2");
    assert_eq!(i["magiskVersion"], "30.7");
    assert_eq!(i["bootServiceRunning"], false);
    assert_eq!(i["slot"], "b");
    let v = ser(&DeviceStatus::Adb {
        info: info(),
        gate: Some(Gate::LowBattery),
    });
    assert_eq!(v["gate"], "lowBattery");
}

#[test]
fn device_status_unit_variants() {
    assert_eq!(ser(&DeviceStatus::None), json!({"kind":"none"}));
    assert_eq!(
        ser(&DeviceStatus::Unauthorized),
        json!({"kind":"unauthorized"})
    );
    assert_eq!(ser(&DeviceStatus::Edl), json!({"kind":"edl"}));
}

fn check<T: serde::Serialize>(table: Vec<(T, &str)>) {
    for (v, s) in table {
        assert_eq!(ser(&v), json!(s));
    }
}

#[test]
fn enums_serialize_as_contract_literals() {
    use StepId::*;
    check(vec![
        (CheckDevice, "checkDevice"),
        (EnterEdl, "enterEdl"),
        (ReadPartitions, "readPartitions"),
        (VerifyBackup, "verifyBackup"),
        (RebootToSystem, "rebootToSystem"),
        (PickImage, "pickImage"),
        (StockCheck, "stockCheck"),
        (ChooseSlot, "chooseSlot"),
        (SaveImport, "saveImport"),
        (Patch, "patch"),
        (CableCheck, "cableCheck"),
        (Write, "write"),
        (VerifyWrite, "verifyWrite"),
        (Reset, "reset"),
        (WaitBoot, "waitBoot"),
        (VerifyRoot, "verifyRoot"),
        (CableGuide, "cableGuide"),
        (WaitEdl, "waitEdl"),
        (PushScripts, "pushScripts"),
        (Done, "done"),
    ]);
    check(vec![
        (PromptKind::CableCheck, "cableCheck"),
        (PromptKind::CableCheckConfirm, "cableCheckConfirm"),
        (PromptKind::ChooseSlot, "chooseSlot"),
        (PromptKind::KeepWaitingOrRescue, "keepWaitingOrRescue"),
        (PromptKind::InstallDriver, "installDriver"),
        (PromptKind::InstallUdev, "installUdev"),
        (PromptKind::EdlNotFound, "edlNotFound"),
        (PromptKind::RetryWrite, "retryWrite"),
        (
            PromptKind::ConnectionLostAfterWrite,
            "connectionLostAfterWrite",
        ),
        (PromptKind::StillInEdl, "stillInEdl"),
        (PromptKind::TabletStarting, "tabletStarting"),
    ]);
    check(vec![
        (RescueOutcome::TabletBack, "tabletBack"),
        (RescueOutcome::NotSeen, "notSeen"),
        (RescueOutcome::HomeScreenSeen, "homeScreenSeen"),
        (RescueOutcome::BackInEdl, "backInEdl"),
    ]);
    check(vec![
        (ErrorCode::NoDevice, "noDevice"),
        (ErrorCode::Unauthorized, "unauthorized"),
        (ErrorCode::UnknownModel, "unknownModel"),
        (ErrorCode::UnknownFirmware, "unknownFirmware"),
        (ErrorCode::LowBattery, "lowBattery"),
        (ErrorCode::AlreadyRooted, "alreadyRooted"),
        (ErrorCode::NotRooted, "notRooted"),
        (ErrorCode::NoStockBackup, "noStockBackup"),
        (ErrorCode::BadImage, "badImage"),
        (ErrorCode::NotStock, "notStock"),
        (ErrorCode::EdlTimeout, "edlTimeout"),
        (ErrorCode::HashMismatch, "hashMismatch"),
        (ErrorCode::PatchFailed, "patchFailed"),
        (ErrorCode::BootTimeout, "bootTimeout"),
        (ErrorCode::VerifyFailed, "verifyFailed"),
        (ErrorCode::ToolMissing, "toolMissing"),
        (ErrorCode::DriverMissing, "driverMissing"),
        (ErrorCode::Busy, "busy"),
        (ErrorCode::Internal, "internal"),
    ]);
    check(vec![
        (FlowKind::Backup, "backup"),
        (FlowKind::ImportBackup, "importBackup"),
        (FlowKind::Root, "root"),
        (FlowKind::Rescue, "rescue"),
        (FlowKind::Unroot, "unroot"),
        (FlowKind::ReinstallBootService, "reinstallBootService"),
    ]);
    check(vec![
        (FlowStatus::Running, "running"),
        (FlowStatus::WaitingForUser, "waitingForUser"),
        (FlowStatus::Done, "done"),
        (FlowStatus::Failed, "failed"),
        (FlowStatus::Cancelled, "cancelled"),
    ]);
    check(vec![
        (Gate::UnknownModel, "unknownModel"),
        (Gate::UnknownFirmware, "unknownFirmware"),
        (Gate::LowBattery, "lowBattery"),
    ]);
    check(vec![
        (Emphasis::Primary, "primary"),
        (Emphasis::Secondary, "secondary"),
        (Emphasis::Danger, "danger"),
    ]);
    check(vec![
        (SuggestedAction::Retry, "retry"),
        (SuggestedAction::GoToRescue, "goToRescue"),
        (SuggestedAction::Reconnect, "reconnect"),
        (SuggestedAction::ChargeDevice, "chargeDevice"),
        (SuggestedAction::ImportBackup, "importBackup"),
    ]);
    check(vec![
        (LogStream::Stdout, "stdout"),
        (LogStream::Stderr, "stderr"),
        (LogStream::Info, "info"),
    ]);
    check(vec![
        (HostOs::Macos, "macos"),
        (HostOs::Windows, "windows"),
        (HostOs::Linux, "linux"),
    ]);
    check(vec![
        (BackupSource::Inkroot, "inkroot"),
        (BackupSource::Imported, "imported"),
    ]);
    check(vec![(Slot::A, "a"), (Slot::B, "b")]);
}

#[test]
fn rescue_result_shape() {
    assert_eq!(
        ser(&FlowResult {
            rescue_outcome: Some(RescueOutcome::NotSeen),
            ..FlowResult::default()
        }),
        json!({"rescueOutcome":"notSeen"})
    );
    let back = ser(&FlowResult {
        rescue_outcome: Some(RescueOutcome::TabletBack),
        device: Some(info()),
        ..FlowResult::default()
    });
    assert_eq!(back["rescueOutcome"], "tabletBack");
    assert_eq!(back["device"]["rooted"], true);
}

#[test]
fn slot_helpers() {
    assert_eq!(Slot::A.suffix(), "_a");
    assert_eq!(Slot::B.suffix(), "_b");
    assert_eq!(Slot::B.boot(), "boot_b");
    assert_eq!(Slot::from_suffix("_b"), Some(Slot::B));
    assert_eq!(Slot::from_suffix("a"), Some(Slot::A));
    assert_eq!(Slot::from_suffix(""), None);
}

fn flow_state() -> FlowState {
    FlowState {
        flow_id: "f1".into(),
        kind: FlowKind::Root,
        step: StepId::CableCheck,
        step_index: 3,
        step_count: 4,
        status: FlowStatus::Running,
        prompt: None,
        error: None,
        result: None,
        writing: false,
    }
}

#[test]
fn flow_state_shape() {
    let v = ser(&flow_state());
    assert_eq!(
        v,
        json!({"flowId":"f1","kind":"root","step":"cableCheck","stepIndex":3,"stepCount":4,
        "status":"running","prompt":null,"error":null,"result":null,"writing":false})
    );
}

#[test]
fn flow_error_shape() {
    let e = FlowError {
        code: ErrorCode::EdlTimeout,
        message: "m".into(),
        detail: "d".into(),
        suggested_action: Some(SuggestedAction::GoToRescue),
    };
    assert_eq!(
        ser(&e),
        json!({"code":"edlTimeout","message":"m","detail":"d","suggestedAction":"goToRescue"})
    );
    let e2 = FlowError {
        suggested_action: None,
        ..e
    };
    assert_eq!(ser(&e2)["suggestedAction"], Value::Null);
    assert_eq!(e2.to_string(), "m");
    let _: &dyn std::error::Error = &e2;
}

#[test]
fn flow_options_accepts_partial() {
    let o: FlowOptions = serde_json::from_value(json!({"imagePath":"/x.img"})).unwrap();
    assert_eq!(o.image_path.as_deref(), Some("/x.img"));
    assert_eq!(o.backup_id, None);
    assert_eq!(o.slot, None);
    let o: FlowOptions = serde_json::from_value(json!({"slot":"a"})).unwrap();
    assert_eq!(o.slot, Some(Slot::A));
    assert_eq!(ser(&FlowOptions::default()), json!({}));
}

#[test]
fn settings_patch_double_option() {
    let p: SettingsPatch = serde_json::from_value(json!({})).unwrap();
    assert_eq!(p.has_rescue_cable, None);
    assert_eq!(p.accepted_risk_version, None);
    let p: SettingsPatch = serde_json::from_value(json!({"hasRescueCable":null})).unwrap();
    assert_eq!(p.has_rescue_cable, Some(None));
    let p: SettingsPatch = serde_json::from_value(json!({"acceptedRiskVersion":1})).unwrap();
    assert_eq!(p.accepted_risk_version, Some(Some(1)));
    assert_eq!(
        ser(&Settings::default()),
        json!({"hasRescueCable":null,"acceptedRiskVersion":null})
    );
}

#[test]
fn log_line_and_app_info_shape() {
    let l = LogLine {
        flow_id: None,
        ts: "t".into(),
        stream: LogStream::Info,
        line: "x".into(),
    };
    assert_eq!(
        ser(&l),
        json!({"flowId":null,"ts":"t","stream":"info","line":"x"})
    );
    let a = AppInfo {
        version: "0.1.0".into(),
        demo: false,
        os: HostOs::Macos,
        backup_root: "/r".into(),
    };
    assert_eq!(
        ser(&a),
        json!({"version":"0.1.0","demo":false,"os":"macos","backupRoot":"/r"})
    );
}

#[test]
fn event_names() {
    assert_eq!(EVT_DEVICE, "device://status");
    assert_eq!(EVT_FLOW_STATE, "flow://state");
    assert_eq!(EVT_PROGRESS, "flow://progress");
    assert_eq!(EVT_LOG, "flow://log");
}

fn rt<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(x: T) {
    let back: T = serde_json::from_value(serde_json::to_value(&x).unwrap()).unwrap();
    assert_eq!(back, x);
}

#[test]
fn round_trip_all() {
    rt(info());
    rt(DeviceStatus::Adb {
        info: info(),
        gate: Some(Gate::UnknownModel),
    });
    rt(DeviceStatus::Edl);
    rt(backup());
    rt(FlowOptions {
        backup_id: Some("b".into()),
        image_path: Some("p".into()),
        slot: Some(Slot::B),
    });
    let prompt = Prompt {
        id: "p1".into(),
        kind: PromptKind::RetryWrite,
        options: vec![PromptOption {
            id: "retry".into(),
            label: "Retry".into(),
            emphasis: Emphasis::Danger,
        }],
    };
    rt(prompt.clone());
    let err = FlowError {
        code: ErrorCode::Busy,
        message: "m".into(),
        detail: "d".into(),
        suggested_action: Some(SuggestedAction::Retry),
    };
    rt(err.clone());
    let res = FlowResult {
        backup: Some(backup()),
        magisk_version: Some("30.7".into()),
        ..FlowResult::default()
    };
    rt(res.clone());
    rt(FlowResult {
        rescue_outcome: Some(RescueOutcome::TabletBack),
        device: Some(info()),
        ..FlowResult::default()
    });
    rt(FlowState {
        prompt: Some(prompt),
        error: Some(err),
        result: Some(res),
        writing: true,
        ..flow_state()
    });
    rt(Progress {
        flow_id: "f".into(),
        done: 1,
        total: 2,
        label: "l".into(),
    });
    rt(LogLine {
        flow_id: Some("f".into()),
        ts: "t".into(),
        stream: LogStream::Stderr,
        line: "x".into(),
    });
    rt(AppInfo {
        version: "v".into(),
        demo: true,
        os: HostOs::Linux,
        backup_root: "/r".into(),
    });
    rt(Settings {
        has_rescue_cable: Some(true),
        accepted_risk_version: Some(1),
    });
    rt(SettingsPatch {
        has_rescue_cable: Some(None),
        accepted_risk_version: Some(Some(1)),
    });
}
