mod support;

use inkroot_lib::backup::StockEvidence;
use inkroot_lib::fakes::{Call, Mode, fake_boot_image};
use inkroot_lib::ipc::{
    ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, StepId, SuggestedAction,
};
use inkroot_lib::tools::adb::RebootTarget;
use support::*;

fn unroot(h: &Harness) -> FlowState {
    run_to_end(h, FlowKind::Unroot, FlowOptions::default(), &[])
}

#[test]
fn unroot_happy_confirms_not_rooted() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    root_the_world(&h.world, true);
    let end = unroot(&h);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step_count, 3);
    let summary = end.result.unwrap().backup.expect("backup summary");
    assert_eq!(summary.id, seeded.id);

    let w = h.world.lock();
    assert!(!w.rooted);
    assert_eq!(w.mode, Mode::System { authorized: true });
    assert_eq!(w.partitions["boot_b"], fake_boot_image(4096, 2));
    drop(w);

    let calls = h.world.calls();
    let reboot = calls
        .iter()
        .position(|c| {
            matches!(
                c,
                Call::Reboot {
                    target: RebootTarget::Edl,
                    ..
                }
            )
        })
        .unwrap();
    let write = calls
        .iter()
        .position(|c| matches!(c, Call::EdlWrite { part, .. } if part == "boot_b"))
        .unwrap();
    let reset = calls
        .iter()
        .position(|c| matches!(c, Call::EdlReset))
        .unwrap();
    assert!(reboot < write && write < reset);
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::EdlReset)).count(),
        1
    );
    // `magisk -c` was asked again after the restart.
    let check_after = calls[reset..]
        .iter()
        .any(|c| matches!(c, Call::Shell { cmd, .. } if cmd == "/debug_ramdisk/magisk -c"));
    assert!(check_after);

    let steps: Vec<(StepId, u8)> = h
        .events
        .states()
        .iter()
        .filter(|s| s.flow_id == end.flow_id)
        .map(|s| {
            assert_eq!(s.step_count, 3, "{s:?}");
            (s.step, s.step_index)
        })
        .collect();
    for want in [
        (StepId::CheckDevice, 1),
        (StepId::EnterEdl, 2),
        (StepId::Write, 2),
        (StepId::VerifyWrite, 2),
        (StepId::Reset, 2),
        (StepId::WaitBoot, 3),
    ] {
        assert!(steps.contains(&want), "{want:?} missing from {steps:?}");
    }
}

#[test]
fn unroot_refuses_tampered_backup() {
    // Review Focus 1: the backup changed on disk after it was taken.
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    root_the_world(&h.world, true);
    let (dir, _) = h.engine.deps().library.get(&seeded.id).unwrap();
    let mut b = std::fs::read(dir.join("boot_b.img")).unwrap();
    b[200] ^= 0xFF;
    std::fs::write(dir.join("boot_b.img"), b).unwrap();

    let end = unroot(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::BadImage, "{e:?}");
    let calls = h.world.calls();
    assert!(!calls.iter().any(is_reboot_edl_call), "{calls:#?}");
    assert!(!calls.iter().any(is_edl_call), "{calls:#?}");
    let w = h.world.lock();
    assert!(w.rooted);
    assert_eq!(w.mode, Mode::System { authorized: true });
}

#[test]
fn unroot_not_rooted_fails() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    let end = unroot(&h);
    assert_eq!(expect_failed(&end).code, ErrorCode::NotRooted);
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| is_reboot_edl_call(c) || is_edl_call(c))
    );
}

#[test]
fn unroot_without_stock_backup_suggests_import() {
    let h = harness(stock_world());
    root_the_world(&h.world, true);
    let end = unroot(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::NoStockBackup);
    assert_eq!(e.suggested_action, Some(SuggestedAction::ImportBackup));
    assert!(!e.message.is_empty());
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| is_reboot_edl_call(c) || is_edl_call(c))
    );
}

#[test]
fn unroot_still_rooted_after_write_is_verify_failed() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    root_the_world(&h.world, true);
    // The backup's boot_b is replaced (consistently, manifest included) by a rooted image,
    // so the tablet comes back rooted.
    let (_, dir, mut m) = h
        .engine
        .deps()
        .library
        .stock_for(
            NOTE_MAX_SERIAL,
            "TKQ1.230721.002",
            inkroot_lib::ipc::Slot::B,
            &h.engine.deps().profiles[0],
        )
        .unwrap();
    let img = patched_boot(2);
    std::fs::write(dir.join("boot_b.img"), &img).unwrap();
    let sha = inkroot_lib::image::sha256_file(&dir.join("boot_b.img")).unwrap();
    m.files
        .iter_mut()
        .find(|f| f.name == "boot_b.img")
        .unwrap()
        .sha256 = sha;
    h.engine.deps().library.write_manifest(&dir, &m).unwrap();

    let end = unroot(&h);
    assert_eq!(expect_failed(&end).code, ErrorCode::VerifyFailed);
}

#[test]
fn unroot_uses_requested_backup_id() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    let older = clone_backup(&h, &seeded.id, "2000-01-01_0000", |_| {});
    root_the_world(&h.world, true);
    let end = run_to_end(&h, FlowKind::Unroot, with_backup_id(&older), &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.result.unwrap().backup.unwrap().id, older);
    assert!(!h.world.lock().rooted);
}

#[test]
fn unroot_refuses_unusable_backup_id() {
    for (name, edit) in [
        (
            "2000-01-01_0001",
            Box::new(|m: &mut inkroot_lib::backup::Manifest| m.slot = inkroot_lib::ipc::Slot::A)
                as Box<dyn FnOnce(&mut inkroot_lib::backup::Manifest)>,
        ),
        ("2000-01-01_0002", Box::new(|m| m.stock = false)),
    ] {
        let h = harness(stock_world());
        let seeded = seed_stock_backup(&h);
        let bad = clone_backup(&h, &seeded.id, name, edit);
        root_the_world(&h.world, true);
        let end = run_to_end(&h, FlowKind::Unroot, with_backup_id(&bad), &[]);
        let e = expect_failed(&end);
        assert_eq!(e.code, ErrorCode::NoStockBackup, "{e:?}");
        assert_eq!(e.suggested_action, Some(SuggestedAction::ImportBackup));
        assert!(e.detail.contains(&bad), "{e:?}");
        let calls = h.world.calls();
        assert!(
            !calls
                .iter()
                .any(|c| is_reboot_edl_call(c) || is_edl_call(c)),
            "{calls:#?}"
        );
    }
}

#[test]
fn unroot_refuses_each_unusable_backup_id() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    let ids = unusable_backup_ids(&h, &seeded.id);
    root_the_world(&h.world, true);
    for (bad, why) in ids {
        h.world.lock().calls.clear();
        let end = run_to_end(&h, FlowKind::Unroot, with_backup_id(&bad), &[]);
        let e = expect_failed(&end);
        assert_eq!(e.code, ErrorCode::NoStockBackup, "{why}: {e:?}");
        assert_eq!(
            e.suggested_action,
            Some(SuggestedAction::ImportBackup),
            "{why}"
        );
        let calls = h.world.calls();
        assert!(
            !calls
                .iter()
                .any(|c| is_reboot_edl_call(c) || is_edl_call(c)),
            "{why}: {calls:#?}"
        );
        assert!(h.world.lock().rooted);
    }
}

#[test]
fn unroot_driver_cancel_after_reboot_edl_resets_tablet() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    root_the_world(&h.world, true);
    h.host.set_needs_driver(true);
    let end = run_to_end(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        &[("installDriver", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert!(calls.iter().any(is_reboot_edl_call), "{calls:#?}");
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })));
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::EdlReset)).count(),
        1
    );
    assert_ne!(h.world.lock().mode, Mode::Edl);
}

#[test]
fn unroot_cancel_at_retry_write_never_resets() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    root_the_world(&h.world, true);
    h.world.lock().faults.write_corrupts_times = 1;
    let end = run_to_end(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        &[("retryWrite", "cancel")],
    );
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    let calls = h.world.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::EdlWrite { .. }))
            .count(),
        1
    );
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlReset)));
    assert_eq!(h.world.lock().mode, Mode::Edl);
}

// R44: no legacy trust for imported backups.

fn assert_unroot_refuses_import(h: &Harness, id: &str) {
    root_the_world(&h.world, true);
    for opts in [FlowOptions::default(), with_backup_id(id)] {
        let end = run_to_end(h, FlowKind::Unroot, opts, &[]);
        let e = expect_failed(&end);
        assert_eq!(e.code, ErrorCode::NoStockBackup, "{e:?}");
        assert!(
            !h.world
                .calls()
                .iter()
                .any(|c| is_reboot_edl_call(c) || is_edl_call(c))
        );
    }
}

#[test]
fn unroot_refuses_imported_backup_with_magiskboot_evidence() {
    let h = harness(stock_world());
    let id = import_world_boot_b(&h, legacy_stock(StockEvidence::MagiskbootTest));
    assert_unroot_refuses_import(&h, &id);
}

#[test]
fn unroot_refuses_imported_known_hash_no_longer_listed() {
    let h = harness(stock_world());
    let id = import_world_boot_b(&h, legacy_stock(StockEvidence::KnownHash));
    assert_unroot_refuses_import(&h, &id);
}

#[test]
fn unroot_uses_valid_known_hash_import() {
    let h = harness_opts(stock_world(), true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![world_boot_b_known()];
    });
    let id = import_world_boot_b(&h, |_| {});
    root_the_world(&h.world, true);
    let end = unroot(&h);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.result.unwrap().backup.unwrap().id, id);
}
