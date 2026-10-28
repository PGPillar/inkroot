mod support;

use inkroot_lib::clock::Clock;
use inkroot_lib::fakes::{Call, Mode, fake_boot_image};
use inkroot_lib::image::sha256_file;
use inkroot_lib::ipc::{
    ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, RescueOutcome, Slot, StepId,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
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
    let calls = h.world.calls();
    let reset = calls
        .iter()
        .position(|c| matches!(c, Call::EdlReset))
        .unwrap();
    assert!(!calls[..reset].iter().any(is_adb_call));
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
fn rescue_makes_no_adb_call_before_the_restart() {
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
    let reset = calls
        .iter()
        .position(|c| matches!(c, Call::EdlReset))
        .expect("reset");
    assert!(calls[..reset].iter().any(is_edl_call));
    assert!(!calls[..reset].iter().any(is_adb_call), "{calls:#?}");
    // Afterwards only looks: never a reboot, push or write over adb.
    assert!(
        calls[reset..]
            .iter()
            .all(|c| !matches!(c, Call::Reboot { .. } | Call::Push { .. })),
        "{calls:#?}"
    );
}

/// Seeds a stock backup (a real backup flow), then bricks the tablet, applies `faults` and
/// starts Rescue from that backup.
fn rescue_bricked(h: &Harness, faults: impl FnOnce(&mut inkroot_lib::fakes::Faults)) -> String {
    let seeded = seed_stock_backup(h);
    brick(h);
    faults(&mut h.world.lock().faults);
    h.engine
        .start(FlowKind::Rescue, with_backup_id(&seeded.id))
        .expect("start rescue")
}

fn outcome(s: &FlowState) -> Option<RescueOutcome> {
    s.result.as_ref().and_then(|r| r.rescue_outcome)
}

fn saw_written_then_wait(h: &Harness, id: &str) {
    let states: Vec<FlowState> = h
        .events
        .states()
        .into_iter()
        .filter(|s| s.flow_id == id)
        .collect();
    let wait = states
        .iter()
        .position(|s| s.step == StepId::WaitBoot)
        .expect("waitBoot step shown");
    assert_eq!(states[wait].step_index, 4);
    assert!(
        states[..wait].iter().any(|s| s.step == StepId::Reset),
        "waitBoot only after the restart"
    );
    assert!(
        h.events
            .logs()
            .iter()
            .any(|l| l.line.starts_with("Written and checked")),
        "written-and-checked line logged"
    );
}

#[test]
fn rescue_tablet_back_authorized_reports_device() {
    let h = harness(stock_world());
    let id = rescue_bricked(&h, |_| {});
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step, StepId::Done);
    assert_eq!(outcome(&end), Some(RescueOutcome::TabletBack));
    let dev = end
        .result
        .as_ref()
        .unwrap()
        .device
        .as_ref()
        .expect("device");
    assert!(!dev.rooted);
    saw_written_then_wait(&h, &id);
    // Seen on the first look: the "I can see my home screen" offer never showed.
    assert!(
        !h.events
            .states()
            .iter()
            .any(|s| s.flow_id == id && s.prompt.is_some())
    );
}

#[test]
fn rescue_tablet_back_unauthorized() {
    let h = harness(stock_world());
    let id = rescue_bricked(&h, |f| f.boots_unauthorized = true);
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(outcome(&end), Some(RescueOutcome::TabletBack));
    assert!(end.result.as_ref().unwrap().device.is_none());
}

#[test]
fn rescue_tablet_never_seen_is_done_not_failed() {
    let h = harness(stock_world());
    let id = rescue_bricked(&h, |f| f.boot_hangs = true);
    let before = h.clock.now();
    let end = wait_terminal(&h, &id);
    // The "I can see my home screen" offer was up while Inkroot watched.
    let offered = h
        .events
        .states()
        .into_iter()
        .filter(|s| s.flow_id == id)
        .find_map(|s| s.prompt)
        .expect("tabletStarting offered");
    assert_eq!(prompt_literal(&offered), "tabletStarting");
    assert_eq!(offered.options.len(), 1);
    assert_eq!(offered.options[0].id, "homeScreen");
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert!(end.error.is_none());
    assert!(end.prompt.is_none());
    assert_eq!(outcome(&end), Some(RescueOutcome::NotSeen));
    let waited = h.clock.now() - before;
    assert!(
        waited >= h.engine.deps().timeouts.wait_rescue_boot,
        "{waited:?}"
    );
    assert!(waited < h.engine.deps().timeouts.wait_rescue_boot + Duration::from_secs(60));
    saw_written_then_wait(&h, &id);
    // Quiet: one "not visible" line, not one per poll.
    let quiet = h
        .events
        .logs()
        .iter()
        .filter(|l| l.line == "Tablet not visible over USB right now")
        .count();
    assert_eq!(quiet, 1);
}

#[test]
fn rescue_tablet_back_in_edl_is_reported() {
    let h = harness(stock_world());
    let id = rescue_bricked(&h, |f| f.boot_returns_to_edl = true);
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(outcome(&end), Some(RescueOutcome::BackInEdl));
    assert_eq!(h.world.lock().mode, Mode::Edl);
    // Verified write, one restart; never a second write or reset behind the user's back.
    assert_eq!(resets(&h), 1);
}

/// Virtual time that also takes a little real time per sleep, so a test can answer a prompt
/// while the flow is still polling.
struct SlowClock(inkroot_lib::clock::FakeClock);

impl Clock for SlowClock {
    fn now(&self) -> Duration {
        self.0.now()
    }
    fn sleep(&self, d: Duration) {
        std::thread::sleep(Duration::from_millis(5));
        self.0.sleep(d);
    }
    fn wall(&self) -> chrono::DateTime<chrono::Local> {
        self.0.wall()
    }
}

#[test]
fn rescue_user_ends_wait_early() {
    let h = harness_opts(stock_world(), true, |d| {
        d.clock = std::sync::Arc::new(SlowClock(inkroot_lib::clock::FakeClock::new(
            chrono::Local::now(),
        )));
    });
    let id = rescue_bricked(&h, |f| f.boot_hangs = true);
    let p = wait_prompt(&h, &id, "tabletStarting");
    let at = h.engine.deps().clock.now();
    h.engine.answer(&id, &p.id, "homeScreen");
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(outcome(&end), Some(RescueOutcome::HomeScreenSeen));
    assert!(h.engine.deps().clock.now() - at < h.engine.deps().timeouts.wait_rescue_boot);
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
