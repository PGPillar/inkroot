mod support;

use inkroot_lib::clock::Clock;
use inkroot_lib::fakes::{Call, Mode, fake_boot_image};
use inkroot_lib::image::sha256_file;
use inkroot_lib::ipc::{ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, Slot, StepId};
use std::path::{Path, PathBuf};
use support::*;

fn write_img(dir: &Path, name: &str, bytes: &[u8]) -> (PathBuf, String) {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    let sha = sha256_file(&p).unwrap();
    (p, sha)
}

/// The tablet is soft-bricked: its active boot is garbage and it sits in EDL mode.
fn brick(h: &Harness) {
    let mut w = h.world.lock();
    w.partitions.insert("boot_b".into(), vec![0u8; 4096]);
    w.mode = Mode::Edl;
    w.calls.clear();
}

fn rescue(h: &Harness, opts: FlowOptions, answers: &[(&str, &str)]) -> FlowState {
    run_to_end(h, FlowKind::Rescue, opts, answers)
}

fn loose(path: &Path) -> FlowOptions {
    FlowOptions {
        image_path: Some(path.to_string_lossy().into_owned()),
        ..FlowOptions::default()
    }
}

fn resets(h: &Harness) -> usize {
    h.world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::EdlReset))
        .count()
}

#[test]
fn rescue_from_backup_happy() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    let stock_b = fake_boot_image(4096, 2);
    brick(&h);
    let end = rescue(
        &h,
        FlowOptions {
            backup_id: Some(seeded.id.clone()),
            ..FlowOptions::default()
        },
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step, StepId::Done);
    assert_eq!(end.step_index, 4);
    assert_eq!(end.step_count, 4);
    assert_eq!(resets(&h), 1);
    let calls = h.world.calls();
    let write = calls
        .iter()
        .position(|c| matches!(c, Call::EdlWrite { part, .. } if part == "boot_b"))
        .expect("boot_b written");
    let reset = calls
        .iter()
        .position(|c| matches!(c, Call::EdlReset))
        .unwrap();
    assert!(write < reset);
    assert_eq!(h.world.lock().partitions["boot_b"], stock_b);
    // No scratch file left behind in the backup folder.
    let (dir, _) = h.engine.deps().library.get(&seeded.id).unwrap();
    assert!(std::fs::read_dir(&dir).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".verify")
    }));

    // Step numbering: pickImage 1, cableGuide/waitEdl 2, write/verifyWrite/reset 3, done 4.
    let steps: Vec<(StepId, u8)> = h
        .events
        .states()
        .iter()
        .filter(|s| s.flow_id == end.flow_id)
        .map(|s| {
            assert_eq!(s.step_count, 4, "{s:?}");
            assert_eq!(s.kind, FlowKind::Rescue, "{s:?}");
            (s.step, s.step_index)
        })
        .collect();
    for want in [
        (StepId::PickImage, 1),
        (StepId::CableGuide, 2),
        (StepId::WaitEdl, 2),
        (StepId::Write, 3),
        (StepId::VerifyWrite, 3),
        (StepId::Reset, 3),
        (StepId::Done, 4),
    ] {
        assert!(steps.contains(&want), "{want:?} missing from {steps:?}");
    }
}

#[test]
fn rescue_from_backup_uses_requested_slot() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    {
        let mut w = h.world.lock();
        w.partitions.insert("boot_a".into(), vec![0u8; 4096]);
        w.mode = Mode::Edl;
        w.calls.clear();
    }
    let end = rescue(
        &h,
        FlowOptions {
            backup_id: Some(seeded.id),
            slot: Some(Slot::A),
            ..FlowOptions::default()
        },
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(
        h.world.lock().partitions["boot_a"],
        fake_boot_image(4096, 1)
    );
}

#[test]
fn rescue_loose_file_slot_from_filename() {
    let h = harness(stock_world());
    brick(&h);
    let src = tempfile::tempdir().unwrap();
    let img = fake_boot_image(4096, 9);
    let (path, sha) = write_img(src.path(), "my_boot_a_copy.img", &img);
    let end = rescue(&h, loose(&path), &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(h.world.lock().partitions["boot_a"], img);
    let written: Vec<(String, String)> = h
        .world
        .calls()
        .iter()
        .filter_map(|c| match c {
            Call::EdlWrite { part, sha } => Some((part.clone(), sha.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(written, vec![("boot_a".to_string(), sha)]);
    assert_eq!(resets(&h), 1);
    // The slot came from the filename: no chooseSlot prompt.
    assert!(
        h.events
            .states()
            .iter()
            .all(|s| s.prompt.is_none() && s.step != StepId::ChooseSlot)
    );
    // The scratch read-back never lands next to the user's file.
    assert_eq!(std::fs::read_dir(src.path()).unwrap().count(), 1);
}

#[test]
fn rescue_loose_file_asks_slot_when_unknown() {
    let h = harness(stock_world());
    brick(&h);
    let src = tempfile::tempdir().unwrap();
    let img = fake_boot_image(4096, 9);
    let (path, _) = write_img(src.path(), "startup.img", &img);
    let end = rescue(&h, loose(&path), &[("chooseSlot", "b")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(h.world.lock().partitions["boot_b"], img);
    let ask = h
        .events
        .states()
        .into_iter()
        .find(|s| s.prompt.is_some())
        .unwrap();
    assert_eq!(ask.step, StepId::ChooseSlot);
    assert_eq!(ask.step_index, 1);
}

#[test]
fn rescue_waits_then_edl_not_found_prompt() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    {
        // Not in EDL yet: the user is still fiddling with the cable.
        let mut w = h.world.lock();
        w.mode = Mode::Off;
        w.calls.clear();
    }
    let t0 = h.clock.now();
    let id = h
        .engine
        .start(
            FlowKind::Rescue,
            FlowOptions {
                backup_id: Some(seeded.id),
                ..FlowOptions::default()
            },
        )
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    let at = latest_state(&h, &id).unwrap();
    assert_eq!(at.step, StepId::WaitEdl);
    assert_eq!(at.step_index, 2);
    assert!(h.clock.now() - t0 >= std::time::Duration::from_secs(60));
    // Now it shows up; "Try again" polls again and the rescue completes.
    h.world.lock().mode = Mode::Edl;
    h.engine.answer(&id, &p.id, "retry");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(resets(&h), 1);
    assert!(!h.world.calls().iter().any(is_adb_call));
}

#[test]
fn rescue_edl_not_found_cancel_writes_nothing() {
    let h = harness(stock_world());
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 9));
    h.world.lock().mode = Mode::Off;
    let end = rescue(&h, loose(&path), &[("edlNotFound", "cancel")]);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    assert!(!h.world.calls().iter().any(is_edl_call));
}

#[test]
fn rescue_bad_image_rejected() {
    let h = harness(stock_world());
    brick(&h);
    let src = tempfile::tempdir().unwrap();

    // Wrong magic.
    let mut bad = fake_boot_image(4096, 9);
    bad[..8].copy_from_slice(b"NOTABOOT");
    let (path, _) = write_img(src.path(), "boot_b.img", &bad);
    let end = rescue(&h, loose(&path), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);

    // Wrong size.
    let (path, _) = write_img(src.path(), "boot_a.img", &fake_boot_image(1024, 9));
    let end = rescue(&h, loose(&path), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);

    // Neither a backup nor a file.
    let end = rescue(&h, FlowOptions::default(), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);

    // A backup that doesn't exist.
    let end = rescue(
        &h,
        FlowOptions {
            backup_id: Some("NOPE/2026-01-01_0000".into()),
            ..FlowOptions::default()
        },
        &[],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);

    assert!(h.world.calls().is_empty(), "{:#?}", h.world.calls());
}

#[test]
fn rescue_refuses_tampered_backup() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    brick(&h);
    let (dir, _) = h.engine.deps().library.get(&seeded.id).unwrap();
    let mut b = std::fs::read(dir.join("boot_b.img")).unwrap();
    b[100] ^= 0xFF;
    std::fs::write(dir.join("boot_b.img"), b).unwrap();
    let end = rescue(
        &h,
        FlowOptions {
            backup_id: Some(seeded.id),
            ..FlowOptions::default()
        },
        &[],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);
    assert!(h.world.calls().is_empty(), "{:#?}", h.world.calls());
}

#[test]
fn rescue_never_calls_adb() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    brick(&h);
    let end = rescue(
        &h,
        FlowOptions {
            backup_id: Some(seeded.id),
            ..FlowOptions::default()
        },
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let calls = h.world.calls();
    assert!(calls.iter().any(is_edl_call));
    assert!(!calls.iter().any(is_adb_call), "{calls:#?}");
}

#[test]
fn rescue_driver_cancel_never_resets() {
    // Rescue never sent the tablet into EDL (it may be bricked): cancel leaves it alone.
    let h = harness(stock_world());
    let src = tempfile::tempdir().unwrap();
    let (path, _) = write_img(src.path(), "boot_b.img", &fake_boot_image(4096, 9));
    h.world.lock().mode = Mode::Edl;
    h.host.set_needs_driver(true);
    let end = rescue(&h, loose(&path), &[("installDriver", "cancel")]);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    assert!(!h.world.calls().iter().any(is_edl_call));
    assert_eq!(h.world.lock().mode, Mode::Edl);
}
