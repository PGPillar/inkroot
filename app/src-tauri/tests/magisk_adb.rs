mod support;

use inkroot_lib::fakes::{Call, FakeAdb, fake_boot_image};
use inkroot_lib::ipc::ErrorCode;
use inkroot_lib::magisk::*;
use inkroot_lib::tools::Timeouts;
use std::path::PathBuf;
use support::*;

struct Fx {
    _dir: tempfile::TempDir,
    payload: Vec<(PathBuf, &'static str)>,
    stock: PathBuf,
    rc: PathBuf,
    bootsvc: PathBuf,
    service: PathBuf,
    out: PathBuf,
}

fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let (apk, sha) = fixture_apk(dir.path());
    let payload = extract_payload(&apk, &sha, &dir.path().join("payload")).unwrap();
    let stock = dir.path().join("boot_b.img");
    std::fs::write(&stock, fake_boot_image(4096, 2)).unwrap();
    let rc = dir.path().join("init.bootsvc.rc");
    let bootsvc = dir.path().join("bootsvc.sh");
    let service = dir.path().join("inkroot-default-service.sh");
    for f in [&rc, &bootsvc, &service] {
        std::fs::write(f, b"x").unwrap();
    }
    let out = dir.path().join("patched.img");
    Fx {
        _dir: dir,
        payload,
        stock,
        rc,
        bootsvc,
        service,
        out,
    }
}

fn push_all(adb: &FakeAdb, f: &Fx) {
    push_patch_inputs(
        adb,
        NOTE_MAX_SERIAL,
        &f.payload,
        &f.stock,
        &f.rc,
        &f.bootsvc,
        &f.service,
    )
    .unwrap();
}

#[test]
fn push_patch_inputs_order_matches_build_and_flash() {
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    let f = fx();
    push_all(&adb, &f);
    let remotes: Vec<String> = w
        .calls()
        .into_iter()
        .filter_map(|c| match c {
            Call::Push { remote, .. } => Some(remote),
            _ => None,
        })
        .collect();
    let mut want: Vec<String> = APK_PAYLOAD
        .iter()
        .map(|(_, n)| format!("{MGK_DIR}/{n}"))
        .collect();
    want.extend([
        format!("{MGK_DIR}/init.bootsvc.rc"),
        format!("{MGK_DIR}/stock_boot.img"),
        DEVICE_BOOTSVC.to_string(),
        DEVICE_SERVICE.to_string(),
    ]);
    assert_eq!(remotes, want);
    assert!(w.calls().contains(&Call::Shell {
        serial: NOTE_MAX_SERIAL.into(),
        cmd: MGK_RESET_CMD.into()
    }));
    assert_eq!(
        w.lock().files[&format!("{MGK_DIR}/stock_boot.img")],
        fake_boot_image(4096, 2)
    );
}

#[test]
fn run_patch_sends_exact_patch_cmd() {
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    let f = fx();
    push_all(&adb, &f);
    let p = test_profile();
    let sha = run_patch(
        &adb,
        NOTE_MAX_SERIAL,
        &p.magisk.patch_env,
        p.boot_size,
        &f.out,
        &Timeouts::default(),
    )
    .unwrap();
    assert_eq!(sha.len(), 64);
    assert!(w.calls().contains(&Call::Shell {
        serial: NOTE_MAX_SERIAL.into(),
        cmd: PATCH_CMD.into()
    }));
    let bytes = std::fs::read(&f.out).unwrap();
    assert_eq!(bytes.len(), 4096);
    assert!(bytes.windows(15).any(|x| x == b"INKROOT-PATCHED"));
}

#[test]
fn run_patch_failure_is_patch_failed() {
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    let f = fx();
    push_all(&adb, &f);
    w.lock().faults.patch_fails = true;
    let p = test_profile();
    let e = run_patch(
        &adb,
        NOTE_MAX_SERIAL,
        &p.magisk.patch_env,
        p.boot_size,
        &f.out,
        &Timeouts::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::PatchFailed);
    assert!(!f.out.exists());
}

fn cleanups(w: &inkroot_lib::fakes::FakeWorld) -> usize {
    w.calls()
        .iter()
        .filter(|c| matches!(c, Call::Shell { cmd, .. } if cmd == CHECK_CLEANUP_CMD))
        .count()
}

#[test]
fn stock_check_exit_codes() {
    let f = fx();
    let t = Timeouts::default();
    let mut n = 0;
    for (code, want) in [
        (0, Some(true)),
        (1, Some(false)),
        (2, Some(false)),
        (9, None),
    ] {
        let w = stock_world();
        let adb = FakeAdb::new(w.clone());
        w.lock().faults.stock_check_exit = Some(code);
        let r = stock_check(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.stock, &t);
        match want {
            Some(b) => assert_eq!(r.unwrap(), b, "exit {code}"),
            None => assert_eq!(r.unwrap_err().code, ErrorCode::Internal),
        }
        assert_eq!(cleanups(&w), 1, "cleanup after exit {code}");
        assert!(
            w.lock().files.keys().all(|k| !k.starts_with(CHECK_DIR)),
            "check dir left behind"
        );
        n += 1;
    }
    assert_eq!(n, 4);
    // Without faults the fake inspects the image for the patch marker.
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    assert!(stock_check(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.stock, &t).unwrap());
    push_all(&adb, &f);
    let p = test_profile();
    run_patch(&adb, NOTE_MAX_SERIAL, &p.magisk.patch_env, 4096, &f.out, &t).unwrap();
    assert!(!stock_check(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.out, &t).unwrap());
}

#[test]
fn hook_present_true_false() {
    let f = fx();
    let t = Timeouts::default();
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    assert!(!hook_present(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.stock, &t).unwrap());
    push_all(&adb, &f);
    let p = test_profile();
    run_patch(&adb, NOTE_MAX_SERIAL, &p.magisk.patch_env, 4096, &f.out, &t).unwrap();
    assert!(hook_present(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.out, &t).unwrap());
    w.lock().faults.hook_check_exit = Some(1);
    assert!(!hook_present(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.out, &t).unwrap());
    w.lock().faults.hook_check_exit = Some(0);
    assert!(hook_present(&adb, NOTE_MAX_SERIAL, &f.payload[0].0, &f.stock, &t).unwrap());
    assert!(cleanups(&w) >= 4);
}

#[test]
fn fake_boot_cycle_follows_active_slot_bytes() {
    use inkroot_lib::fakes::{FakeEdl, Mode};
    use inkroot_lib::tools::adb::{Adb, RebootTarget};
    use inkroot_lib::tools::edl::Edl;
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    let edl = FakeEdl::new(w.clone());
    let f = fx();
    push_all(&adb, &f);
    let p = test_profile();
    run_patch(
        &adb,
        NOTE_MAX_SERIAL,
        &p.magisk.patch_env,
        4096,
        &f.out,
        &Timeouts::default(),
    )
    .unwrap();
    adb.reboot(NOTE_MAX_SERIAL, RebootTarget::Edl).unwrap();
    assert_eq!(w.lock().mode, Mode::Edl);
    assert!(adb.devices().unwrap().is_empty());
    edl.write("boot_b", &f.out, &|_, _| {}).unwrap();
    edl.reset().unwrap();
    assert_eq!(w.lock().mode, Mode::Off);
    assert!(
        adb.shell(NOTE_MAX_SERIAL, "getprop", Default::default())
            .is_err()
    );
    assert_eq!(adb.devices().unwrap().len(), 1);
    let g = w.lock();
    assert!(g.rooted && g.service_running);
    assert_eq!(g.bootsvc_lines.len(), 1);
}

#[test]
fn edl_faults_never_appears_and_rereads_differ() {
    use inkroot_lib::fakes::FakeEdl;
    use inkroot_lib::tools::adb::{Adb, RebootTarget};
    use inkroot_lib::tools::edl::Edl;
    use inkroot_lib::tools::process::CancelToken;
    let w = stock_world();
    let adb = FakeAdb::new(w.clone());
    let edl = FakeEdl::new(w.clone());
    adb.reboot(NOTE_MAX_SERIAL, RebootTarget::Edl).unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = CancelToken::default();
    w.lock().faults.verify_read_differs = true;
    let mut seen = Vec::new();
    for i in 0..3 {
        let p = d.path().join(format!("r{i}"));
        edl.read("boot_b", &p, &c, &|_, _| {}).unwrap();
        seen.push(std::fs::read(p).unwrap());
    }
    assert_ne!(seen[0], seen[1]);
    assert_ne!(seen[1], seen[2]);
    assert_ne!(seen[0], seen[2]);
    assert_eq!(&seen[1][..8], b"ANDROID!");
    w.lock().faults.edl_never_appears = true;
    let p = d.path().join("x");
    std::fs::write(&p, fake_boot_image(4096, 9)).unwrap();
    for e in [
        edl.read("boot_b", &p, &c, &|_, _| {}).unwrap_err(),
        edl.write("boot_b", &p, &|_, _| {}).unwrap_err(),
        edl.reset().unwrap_err(),
    ] {
        assert!(
            matches!(e, inkroot_lib::tools::ToolError::Timeout { .. }),
            "{e:?}"
        );
    }
}
