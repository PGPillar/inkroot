mod support;

use inkroot_lib::backup::StockEvidence;
use inkroot_lib::fakes::{Call, Mode, PATCH_MARKER, fake_boot_image};
use inkroot_lib::flows::import::RESCUE_ONLY_MESSAGE;
use inkroot_lib::image::sha256_file;
use inkroot_lib::ipc::{
    BackupSource, ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, Slot, StepId,
};
use inkroot_lib::magisk::STOCK_CHECK_CMD;
use inkroot_lib::profile::KnownStockBoot;
use std::path::{Path, PathBuf};
use support::*;

const FW: &str = "TKQ1.230721.002";

fn known(sha: &str, slot: Slot) -> KnownStockBoot {
    KnownStockBoot {
        sha256: sha.to_string(),
        slot,
        display_id: FW.to_string(),
    }
}

fn logged(h: &Harness, text: &str) -> bool {
    h.events.logs().iter().any(|l| l.line.contains(text))
}

/// Import never asks the tablet anything beyond the probe: no pushes, no magiskboot.
fn assert_no_tablet_check(h: &Harness) {
    for c in h.world.calls() {
        match c {
            Call::Push { .. } => panic!("import pushed to the tablet: {c:?}"),
            Call::Shell { cmd, .. } => assert!(
                cmd != STOCK_CHECK_CMD && !cmd.contains("/data/local/tmp"),
                "import ran a tablet check: {cmd}"
            ),
            _ => {}
        }
    }
}

fn write_img(dir: &Path, name: &str, bytes: &[u8]) -> (PathBuf, String) {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    let sha = sha256_file(&p).unwrap();
    (p, sha)
}

fn patched_image() -> Vec<u8> {
    let mut img = fake_boot_image(4096, 9);
    let at = img.len() - PATCH_MARKER.len();
    img[at..].copy_from_slice(PATCH_MARKER);
    img
}

fn no_device_world() -> inkroot_lib::fakes::FakeWorld {
    let w = stock_world();
    w.lock().mode = Mode::Off;
    w
}

fn import(h: &Harness, path: &Path, answers: &[(&str, &str)]) -> FlowState {
    let opts = FlowOptions {
        image_path: Some(path.to_string_lossy().into_owned()),
        ..FlowOptions::default()
    };
    run_to_end(h, FlowKind::ImportBackup, opts, answers)
}

fn stock_check_calls(h: &Harness) -> usize {
    h.world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::Shell { cmd, .. } if cmd == STOCK_CHECK_CMD))
        .count()
}

#[test]
fn import_known_hash_without_device_is_stock() {
    let src = tempfile::tempdir().unwrap();
    let (path, sha) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 7));
    let h = harness_opts(no_device_world(), true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![known(&sha.to_uppercase(), Slot::B)];
    });
    let end = import(&h, &path, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step_count, 3);
    let s = end.result.unwrap().backup.unwrap();
    assert_eq!(s.serial, "unassigned");
    assert!(s.stock && s.verified);
    assert_eq!(s.source, BackupSource::Imported);
    assert_eq!(s.slot, Slot::B);
    assert_eq!(s.firmware, FW);
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.files[0].name, "boot_b.img");
    assert_eq!(s.files[0].sha256, sha);
    assert_eq!(s.files[0].size, 4096);
    let (dir, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(sha256_file(&dir.join("boot_b.img")).unwrap(), sha);
    assert_eq!(m.stock_evidence, StockEvidence::KnownHash);
    assert_eq!(m.imported_from.as_deref(), Some(path.to_str().unwrap()));
    assert_eq!(m.firmware.build_id, FW);
    assert_eq!(m.firmware.incremental, "226");
    assert!(m.firmware.fingerprint.starts_with("ONYX/TabBoox"));
    assert_no_tablet_check(&h);
    assert!(!logged(&h, RESCUE_ONLY_MESSAGE));
    // The original is untouched.
    assert_eq!(sha256_file(&path).unwrap(), sha);
}

#[test]
fn import_known_hash_right_slot_with_device_is_stock() {
    let src = tempfile::tempdir().unwrap();
    let (path, sha) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 7));
    let h = harness_opts(stock_world(), true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![known(&sha, Slot::B)];
    });
    let end = import(&h, &path, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert!(s.stock && s.verified);
    assert_eq!(s.serial, NOTE_MAX_SERIAL);
    assert_eq!(s.slot, Slot::B);
    assert_eq!(s.firmware, FW);
    let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::KnownHash);
    assert!(m.firmware.fingerprint.starts_with("ONYX/TabBoox"));
    assert_no_tablet_check(&h);
}

/// Asserts an import ended Done as a Rescue-only (not stock) copy.
fn assert_rescue_only(h: &Harness, end: FlowState) -> inkroot_lib::backup::Manifest {
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step_count, 3);
    let s = end.result.unwrap().backup.unwrap();
    assert!(!s.stock);
    assert!(s.verified);
    assert_eq!(s.source, BackupSource::Imported);
    let (dir, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::UnknownImport);
    assert!(!m.stock);
    assert_eq!(
        sha256_file(&dir.join(&s.files[0].name)).unwrap(),
        s.files[0].sha256
    );
    assert!(
        logged(h, RESCUE_ONLY_MESSAGE),
        "rescue-only copy not logged"
    );
    assert_no_tablet_check(h);
    m
}

#[test]
fn import_unknown_hash_without_device_is_rescue_only() {
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 7));
    let h = harness(no_device_world());
    let m = assert_rescue_only(&h, import(&h, &path, &[]));
    assert_eq!(m.serial, "unassigned");
    assert_eq!(m.firmware.display_id, "");
}

#[test]
fn import_unknown_hash_with_device_is_rescue_only_without_tablet_check() {
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 7));
    let h = harness(stock_world());
    let m = assert_rescue_only(&h, import(&h, &path, &[]));
    assert_eq!(m.serial, NOTE_MAX_SERIAL);
    assert_eq!(m.firmware.display_id, FW);
}

#[test]
fn import_magisk_patched_is_rescue_only_not_rejected() {
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &patched_image());
    let h = harness(stock_world());
    assert_rescue_only(&h, import(&h, &path, &[]));
}

#[test]
fn import_known_hash_wrong_slot_is_rescue_only() {
    let src = tempfile::tempdir().unwrap();
    // The known entry is for slot B; the file is named (and filed) as boot_a.
    let (path, sha) = write_img(src.path(), "boot_a.img", &fake_boot_image(4096, 7));
    for world in [no_device_world(), stock_world()] {
        let sha = sha.clone();
        let h = harness_opts(world, true, move |d| {
            d.profiles[0].known_stock_boot_sha256 = vec![known(&sha, Slot::B)];
        });
        let m = assert_rescue_only(&h, import(&h, &path, &[]));
        assert_eq!(m.slot, Slot::A);
    }
    // opts.slot picks the wrong slot for a plainly named file.
    let (plain, sha) = write_img(src.path(), "stock.img", &fake_boot_image(4096, 7));
    let h = harness_opts(no_device_world(), true, move |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![known(&sha, Slot::B)];
    });
    let opts = FlowOptions {
        image_path: Some(plain.to_string_lossy().into_owned()),
        slot: Some(Slot::A),
        ..FlowOptions::default()
    };
    assert_rescue_only(&h, run_to_end(&h, FlowKind::ImportBackup, opts, &[]));
}

#[test]
fn import_known_hash_device_on_other_firmware_is_rescue_only() {
    let src = tempfile::tempdir().unwrap();
    let (path, sha) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 7));
    let world = stock_world();
    world
        .lock()
        .props
        .insert("ro.build.display.id".into(), "TKQ1.999999.001".into());
    let h = harness_opts(world, true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![known(&sha, Slot::B)];
    });
    let m = assert_rescue_only(&h, import(&h, &path, &[]));
    assert_eq!(m.serial, NOTE_MAX_SERIAL);
    assert_eq!(m.firmware.display_id, "TKQ1.999999.001");
}

#[test]
fn import_bad_magic_rejected_bad_image() {
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &[0u8; 4096]);
    let h = harness(stock_world());
    let end = import(&h, &path, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::BadImage);
    assert_eq!(end.step, StepId::PickImage);
    // Wrong size is also a bad image.
    let (short, _) = write_img(src.path(), "short.img", &fake_boot_image(4095, 7));
    assert_eq!(
        expect_failed(&import(&h, &short, &[])).code,
        ErrorCode::BadImage
    );
    // No image chosen at all.
    let end = run_to_end(&h, FlowKind::ImportBackup, FlowOptions::default(), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);
    assert!(h.engine.deps().library.list(None).is_empty());
    assert_eq!(stock_check_calls(&h), 0);
}

#[test]
fn import_slot_from_filename() {
    let src = tempfile::tempdir().unwrap();
    let (path, sha) = write_img(src.path(), "boot_a.stock.img", &fake_boot_image(4096, 7));
    // The connected tablet is on slot B; the filename wins.
    let h = harness(stock_world());
    let end = import(&h, &path, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert_eq!(s.slot, Slot::A);
    assert_eq!(s.serial, NOTE_MAX_SERIAL);
    // An unknown file is Rescue-only, even with a tablet connected.
    assert!(!s.stock);
    assert_eq!(s.files[0].name, "boot_a.img");
    assert_eq!(s.files[0].sha256, sha);
    let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::UnknownImport);
    assert_eq!(stock_check_calls(&h), 0);
    // opts.slot overrides the filename.
    let opts = FlowOptions {
        image_path: Some(path.to_string_lossy().into_owned()),
        slot: Some(Slot::B),
        ..FlowOptions::default()
    };
    let end = run_to_end(&h, FlowKind::ImportBackup, opts, &[]);
    assert_eq!(end.result.unwrap().backup.unwrap().slot, Slot::B);
    // No filename hint: the device's slot.
    let (plain, _) = write_img(src.path(), "stock.img", &fake_boot_image(4096, 7));
    let end = import(&h, &plain, &[]);
    assert_eq!(end.result.unwrap().backup.unwrap().slot, Slot::B);
}

#[test]
fn import_asks_slot_when_unknown() {
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "stock.img", &fake_boot_image(4096, 7));
    let h = harness(no_device_world());
    let end = import(&h, &path, &[("chooseSlot", "a")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert_eq!(s.slot, Slot::A);
    assert_eq!(s.files[0].name, "boot_a.img");
    assert!(Path::new(&s.path).join("boot_a.img").is_file());
    let asked = h
        .events
        .states()
        .into_iter()
        .find(|st| st.status == FlowStatus::WaitingForUser)
        .unwrap();
    assert_eq!(asked.step, StepId::ChooseSlot);
    assert_eq!(asked.step_index, 3);
    assert_eq!(asked.step_count, 3);
}

#[test]
fn import_known_hash_asks_slot_then_decides_stock() {
    let src = tempfile::tempdir().unwrap();
    let (path, sha) = write_img(src.path(), "stock.img", &fake_boot_image(4096, 7));
    let sha2 = sha.clone();
    let h = harness_opts(no_device_world(), true, move |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![known(&sha2, Slot::B)];
    });
    let end = import(&h, &path, &[("chooseSlot", "b")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert!(s.stock);
    assert_eq!(s.slot, Slot::B);
    let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::KnownHash);
    // Steps stay in contract order: pickImage 1, stockCheck 2, chooseSlot/saveImport 3.
    let mut seen: Vec<(StepId, u8)> = vec![];
    for st in h.events.states() {
        assert_eq!(st.step_count, 3);
        if seen.last() != Some(&(st.step, st.step_index)) {
            seen.push((st.step, st.step_index));
        }
    }
    assert_eq!(
        seen,
        [
            (StepId::PickImage, 1),
            (StepId::StockCheck, 2),
            (StepId::ChooseSlot, 3),
            (StepId::SaveImport, 3)
        ]
    );
    // Answering the wrong slot makes the same file Rescue-only.
    let end = import(&h, &path, &[("chooseSlot", "a")]);
    let s = end.result.unwrap().backup.unwrap();
    assert!(!s.stock);
    assert_eq!(s.slot, Slot::A);
}
