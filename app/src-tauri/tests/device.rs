mod support;

use inkroot_lib::device::*;
use inkroot_lib::fakes::{Call, FakeAdb, FakeUsb, Mode};
use inkroot_lib::ipc::{DeviceStatus, Gate, Slot};
use inkroot_lib::poller::Poller;
use inkroot_lib::tools::Timeouts;
use inkroot_lib::tools::adb::AdbState;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use support::*;

fn run_probe(w: &inkroot_lib::fakes::FakeWorld) -> (DeviceStatus, Option<String>) {
    probe(
        &FakeAdb::new(w.clone()),
        &FakeUsb::new(w.clone()),
        &[test_profile()],
        &Timeouts::default(),
    )
}

#[test]
fn parse_getprop_reads_note_max_props() {
    let out = "[ro.product.model]: [NoteMax]\r\n[ro.product.manufacturer]: [ONYX]\n[ro.boot.slot_suffix]: [_b]\n[empty]: []\ngarbage line\n";
    let m = parse_getprop(out);
    assert_eq!(m["ro.product.model"], "NoteMax");
    assert_eq!(m["ro.product.manufacturer"], "ONYX");
    assert_eq!(m["ro.boot.slot_suffix"], "_b");
    assert_eq!(m["empty"], "");
    assert_eq!(m.len(), 4);
    let w = stock_world();
    let adb = FakeAdb::new(w);
    use inkroot_lib::tools::adb::Adb;
    let o = adb
        .shell(NOTE_MAX_SERIAL, "getprop", Duration::from_secs(1))
        .unwrap();
    let p = props_from(&parse_getprop(&o.stdout), NOTE_MAX_SERIAL);
    assert_eq!(p.model, "NoteMax");
    assert_eq!(p.device, "NoteMax");
    assert_eq!(p.manufacturer, "ONYX");
    assert_eq!(p.build_id, "TKQ1.230721.002");
    assert_eq!(p.display_id, "TKQ1.230721.002");
    assert_eq!(p.incremental, "226");
    assert_eq!(p.slot_suffix, "_b");
    assert_eq!(p.serial, NOTE_MAX_SERIAL);
}

#[test]
fn parse_magisk_version_variants() {
    assert_eq!(parse_magisk_version("30.7:MAGISK:R"), Some("30.7".into()));
    assert_eq!(parse_magisk_version("30.7:MAGISK:R\n"), Some("30.7".into()));
    assert_eq!(
        parse_magisk_version("  27.0:MAGISK:D\r\n"),
        Some("27.0".into())
    );
    assert_eq!(parse_magisk_version(""), None);
    assert_eq!(parse_magisk_version("not found"), None);
    assert_eq!(parse_magisk_version(":MAGISK:R"), None);
}

#[test]
fn bootsvc_helpers() {
    let log = "boot t1 uid=0 ctx=u:r:magisk:s0\nnoise\nboot t2 uid=2000 ctx=u:r:shell:s0\n";
    assert_eq!(bootsvc_boot_lines(log), 2);
    assert!(!bootsvc_ok(log));
    assert!(bootsvc_ok("boot t1 uid=0 ctx=u:r:magisk:s0\n"));
    assert!(!bootsvc_ok(""));
    assert_eq!(
        parse_battery("  AC powered: false\n  level: 73\n"),
        Some(73)
    );
    assert_eq!(parse_battery("nothing"), None);
}

#[test]
fn probe_none() {
    let w = stock_world();
    w.lock().mode = Mode::Off;
    assert_eq!(run_probe(&w), (DeviceStatus::None, None));
}

#[test]
fn probe_unauthorized() {
    let w = stock_world();
    w.lock().mode = Mode::System { authorized: false };
    assert_eq!(run_probe(&w).0, DeviceStatus::Unauthorized);
}

#[test]
fn probe_edl_wins() {
    let w = stock_world();
    w.lock().mode = Mode::Edl;
    w.lock()
        .other_devices
        .push(("PHONE1".into(), AdbState::Device, BTreeMap::new()));
    assert_eq!(run_probe(&w).0, DeviceStatus::Edl);
}

#[test]
fn probe_note_max_ok_has_no_gate() {
    let w = stock_world();
    let (st, serial) = run_probe(&w);
    assert_eq!(serial.as_deref(), Some(NOTE_MAX_SERIAL));
    let DeviceStatus::Adb { info, gate } = st else {
        panic!("expected adb");
    };
    assert_eq!(gate, None);
    assert_eq!(info.slot, Slot::B);
    assert_eq!(info.firmware_label, "4.2");
    assert_eq!(info.battery, 80);
    assert_eq!(info.profile_id.as_deref(), Some("note-max"));
    assert_eq!(info.display_name, "Note Max");
    assert!(!info.rooted);
    assert_eq!(info.magisk_version, None);
    assert!(!info.boot_service_running);
    let json = serde_json::to_value(DeviceStatus::Adb { info, gate }).unwrap();
    assert!(json["gate"].is_null());
}

#[test]
fn probe_wrong_incremental_is_unknown_firmware() {
    let w = stock_world();
    w.lock()
        .props
        .insert("ro.build.version.incremental".into(), "999".into());
    let DeviceStatus::Adb { info, gate } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert_eq!(gate, Some(Gate::UnknownFirmware));
    assert_eq!(info.firmware_label, "TKQ1.230721.002");
}

#[test]
fn probe_unknown_model_gate() {
    let w = stock_world();
    w.lock()
        .props
        .insert("ro.product.model".into(), "Pixel".into());
    let DeviceStatus::Adb { info, gate } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert_eq!(gate, Some(Gate::UnknownModel));
    assert_eq!(info.profile_id, None);
}

#[test]
fn probe_low_battery_gate() {
    let w = stock_world();
    w.lock().battery = 29;
    let DeviceStatus::Adb { gate, .. } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert_eq!(gate, Some(Gate::LowBattery));
    w.lock().battery = 30;
    let DeviceStatus::Adb { gate, .. } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert_eq!(gate, None);
}

#[test]
fn probe_prefers_profile_match_among_many() {
    let w = stock_world();
    let mut phone = BTreeMap::new();
    phone.insert("ro.product.model".to_string(), "Pixel 8".to_string());
    phone.insert("ro.product.device".to_string(), "shiba".to_string());
    phone.insert("ro.product.manufacturer".to_string(), "Google".to_string());
    // The phone sorts before the tablet in `adb devices` output.
    w.lock()
        .other_devices
        .push(("AAA-PHONE".into(), AdbState::Device, phone));
    let (st, serial) = run_probe(&w);
    assert_eq!(serial.as_deref(), Some(NOTE_MAX_SERIAL));
    let DeviceStatus::Adb { info, .. } = st else {
        panic!("expected adb");
    };
    assert_eq!(info.serial, NOTE_MAX_SERIAL);
    assert_eq!(info.profile_id.as_deref(), Some("note-max"));
}

#[test]
fn probe_unauthorized_tablet_beats_unrelated_phone() {
    // The tablet is waiting for "Allow USB debugging" and a phone is authorized: the user must
    // be told to tap Allow, not that their "tablet" (the phone) is the wrong model.
    let w = stock_world();
    let mut phone = BTreeMap::new();
    phone.insert("ro.product.model".to_string(), "Pixel 8".to_string());
    phone.insert("ro.product.device".to_string(), "shiba".to_string());
    phone.insert("ro.product.manufacturer".to_string(), "Google".to_string());
    {
        let mut g = w.lock();
        g.mode = Mode::System { authorized: false };
        g.other_devices
            .push(("AAA-PHONE".into(), AdbState::Device, phone));
    }
    let (st, serial) = run_probe(&w);
    assert_eq!(st, DeviceStatus::Unauthorized);
    assert_eq!(serial, None);
}

#[test]
fn probe_rooted_reports_magisk_and_bootsvc() {
    let w = stock_world();
    {
        let mut g = w.lock();
        g.rooted = true;
        g.magisk_version = Some("30.7".into());
        g.service_running = true;
        g.bootsvc_lines
            .push("boot 2026-10-26 uid=0 ctx=u:r:magisk:s0".into());
    }
    let DeviceStatus::Adb { info, .. } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert!(info.rooted);
    assert_eq!(info.magisk_version.as_deref(), Some("30.7"));
    assert!(info.boot_service_running);
    w.lock().service_running = false;
    let DeviceStatus::Adb { info, .. } = run_probe(&w).0 else {
        panic!("expected adb");
    };
    assert!(info.rooted);
    assert!(!info.boot_service_running);
    let t = Timeouts::default();
    assert_eq!(
        bootsvc_line_count(&FakeAdb::new(w.clone()), NOTE_MAX_SERIAL, &t),
        1
    );
    w.lock().bootsvc_lines.clear();
    assert_eq!(
        bootsvc_line_count(&FakeAdb::new(w.clone()), NOTE_MAX_SERIAL, &t),
        0
    );
}

#[test]
fn poller_emits_only_on_change() {
    let w = stock_world();
    let em = RecordingEmitter::default();
    let poller = Poller::spawn(
        Arc::new(FakeAdb::new(w.clone())),
        Arc::new(FakeUsb::new(w.clone())),
        vec![test_profile()],
        Arc::new(em.clone()),
        Duration::from_secs(3600),
    );
    assert_eq!(poller.latest(), DeviceStatus::None);
    poller.poll_once();
    poller.poll_once();
    assert_eq!(em.devices().len(), 1);
    assert!(matches!(poller.latest(), DeviceStatus::Adb { .. }));
    assert_eq!(poller.serial().as_deref(), Some(NOTE_MAX_SERIAL));
    w.lock().mode = Mode::Edl;
    poller.poll_once();
    poller.poll_once();
    assert_eq!(em.devices().len(), 2);
    assert_eq!(em.devices()[1], DeviceStatus::Edl);
    poller.pause();
    w.lock().mode = Mode::Off;
    poller.poll_once();
    assert_eq!(em.devices().len(), 2);
    poller.resume();
    poller.poll_once();
    assert_eq!(em.devices().len(), 3);
    assert_eq!(poller.latest(), DeviceStatus::None);
    poller.stop();
}

#[test]
fn probe_unknown_slot_is_gated() {
    for v in [None, Some("")] {
        let w = stock_world();
        match v {
            None => {
                w.lock().props.remove("ro.boot.slot_suffix");
            }
            Some(x) => {
                w.lock()
                    .props
                    .insert("ro.boot.slot_suffix".into(), x.into());
            }
        }
        let DeviceStatus::Adb { gate, .. } = run_probe(&w).0 else {
            panic!("expected adb");
        };
        assert_eq!(gate, Some(Gate::UnknownFirmware));
    }
}

#[test]
fn pause_stops_all_probing() {
    let w = stock_world();
    let poller = Poller::spawn(
        Arc::new(FakeAdb::new(w.clone())),
        Arc::new(FakeUsb::new(w.clone())),
        vec![test_profile()],
        Arc::new(RecordingEmitter::default()),
        Duration::from_secs(3600),
    );
    poller.pause();
    let n = w.calls().len();
    poller.poll_once();
    poller.poll_once();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(w.calls().len(), n);
    assert!(!w.calls().is_empty() || n == 0);
    let _ = Call::Devices;
    poller.stop();
}
