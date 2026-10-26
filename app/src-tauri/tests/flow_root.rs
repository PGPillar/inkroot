mod support;

use inkroot_lib::backup::StockEvidence;
use inkroot_lib::fakes::{Call, Mode, PATCH_MARKER};
use inkroot_lib::image::sha256_file;
use inkroot_lib::ipc::{
    BackupSummary, ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, SettingsPatch, StepId,
    SuggestedAction,
};
use inkroot_lib::magisk::PATCH_CMD;
use inkroot_lib::tools::adb::RebootTarget;
use support::*;

const NO_STOCK_MSG: &str = "Inkroot needs an original safety copy of your tablet first.";
const VERIFY_FAILED_MSG: &str = "Your tablet restarted, but root didn't start. Your safety copy is ready if you want to go back.";

fn run_root(h: &Harness, answers: &[(&str, &str)]) -> FlowState {
    run_to_end(h, FlowKind::Root, FlowOptions::default(), answers)
}

/// Takes a verified stock backup with the Back up flow, then clears the call log.
fn seed_backup(h: &Harness) -> BackupSummary {
    let end = run_to_end(h, FlowKind::Backup, FlowOptions::default(), &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert!(s.stock && s.verified);
    h.world.lock().calls.clear();
    s
}

fn is_patch(c: &Call) -> bool {
    matches!(c, Call::Shell { cmd, .. } if cmd == PATCH_CMD)
}

fn is_edl(c: &Call) -> bool {
    matches!(
        c,
        Call::EdlRead { .. } | Call::EdlWrite { .. } | Call::EdlReset
    )
}

fn is_reboot_edl(c: &Call) -> bool {
    matches!(
        c,
        Call::Reboot {
            target: RebootTarget::Edl,
            ..
        }
    )
}

fn pos(calls: &[Call], f: impl Fn(&Call) -> bool) -> usize {
    calls
        .iter()
        .position(f)
        .unwrap_or_else(|| panic!("call not found in {calls:#?}"))
}

fn count(calls: &[Call], f: impl Fn(&Call) -> bool) -> usize {
    calls.iter().filter(|c| f(c)).count()
}

#[test]
fn root_happy_path_call_order() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .expect("start root");
    // While the cableCheck prompt is open the flow is blocked, so the call log length here
    // marks exactly where the prompt sits in the call order.
    let prompt = wait_prompt(&h, &id, "cableCheck");
    let at_prompt = h.world.calls().len();
    h.engine.answer(&id, &prompt.id, "have");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|p| p.id.as_str()) != Some(prompt.id.as_str())
    });
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step_count, 4);
    let result = end.result.unwrap();
    assert_eq!(result.magisk_version.as_deref(), Some("30.7"));
    let summary = result.backup.expect("backup summary");
    assert_eq!(summary.id, seeded.id);
    let patched_file = summary
        .files
        .iter()
        .find(|f| f.name == "patched-boot.img")
        .expect("patched-boot.img listed in result.backup.files");

    let calls = h.world.calls();
    let patch = pos(&calls, is_patch);
    let reboot = pos(&calls, is_reboot_edl);
    let write = pos(
        &calls,
        |c| matches!(c, Call::EdlWrite { part, .. } if part == "boot_b"),
    );
    let read = pos(
        &calls,
        |c| matches!(c, Call::EdlRead { part } if part == "boot_b"),
    );
    let reset = pos(&calls, |c| matches!(c, Call::EdlReset));
    // Shell PATCH_CMD < cableCheck prompt < Reboot{Edl} < EdlWrite < EdlRead < EdlReset.
    assert!(patch < at_prompt, "patch ran before the cableCheck prompt");
    assert!(
        at_prompt <= reboot,
        "reboot edl only after the cableCheck prompt"
    );
    assert!(reboot < write && write < read && read < reset);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 1);
    let cable_state = h
        .events
        .states()
        .into_iter()
        .find(|s| {
            s.prompt
                .as_ref()
                .is_some_and(|p| prompt_literal(p) == "cableCheck")
        })
        .unwrap();
    assert_eq!(cable_state.step, StepId::CableCheck);
    assert_eq!(cable_state.step_index, 3);
    assert_eq!(h.settings.get().has_rescue_cable, Some(true));

    // The patched image was written, and it is what the manifest records.
    let (dir, m) = h.engine.deps().library.get(&summary.id).unwrap();
    let patched = m.patched.as_ref().expect("manifest.patched set");
    assert_eq!(patched.file, "patched-boot.img");
    assert_eq!(patched.magisk_version, "30.7");
    let on_disk = sha256_file(&dir.join("patched-boot.img")).unwrap();
    assert_eq!(patched.sha256, on_disk);
    assert_eq!(patched_file.sha256, on_disk);
    assert_eq!(patched_file.size, 4096);
    let written = calls
        .iter()
        .find_map(|c| match c {
            Call::EdlWrite { part, sha } if part == "boot_b" => Some(sha.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(written, on_disk);
    // The stock entry is unaffected and still verifies.
    assert!(
        h.engine
            .deps()
            .library
            .verify_file(&dir, &m, "boot_b.img")
            .is_ok()
    );
    assert!(m.stock && m.verified);

    // Tablet rooted and back in Android.
    let w = h.world.lock();
    assert!(w.rooted);
    assert_eq!(w.mode, Mode::System { authorized: true });
    let b = &w.partitions["boot_b"];
    assert!(b.windows(PATCH_MARKER.len()).any(|x| x == PATCH_MARKER));
    drop(w);

    // Steps: patch/cableCheck/enterEdl/write/verifyWrite/reset at 3, waitBoot/verifyRoot at 4.
    for s in h
        .events
        .states()
        .iter()
        .filter(|s| s.flow_id == end.flow_id)
    {
        assert_eq!(s.step_count, 4, "{s:?}");
        let want = match s.step {
            StepId::CheckDevice => 1,
            StepId::Patch
            | StepId::CableCheck
            | StepId::EnterEdl
            | StepId::Write
            | StepId::VerifyWrite
            | StepId::Reset => 3,
            StepId::WaitBoot | StepId::VerifyRoot => 4,
            _ => 2,
        };
        assert_eq!(s.step_index, want, "{s:?}");
    }
    let steps: Vec<StepId> = h
        .events
        .states()
        .iter()
        .filter(|s| s.flow_id == end.flow_id)
        .map(|s| s.step)
        .collect();
    for st in [
        StepId::Patch,
        StepId::CableCheck,
        StepId::EnterEdl,
        StepId::Write,
        StepId::VerifyWrite,
        StepId::Reset,
        StepId::WaitBoot,
        StepId::VerifyRoot,
    ] {
        assert!(steps.contains(&st), "{st:?} missing from {steps:?}");
    }
}

#[test]
fn root_uses_existing_stock_backup_without_new_reads() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.result.unwrap().backup.unwrap().id, seeded.id);
    let calls = h.world.calls();
    let patch = pos(&calls, is_patch);
    assert!(
        !calls[..patch].iter().any(is_edl),
        "no EDL calls before the patch: {calls:#?}"
    );
    assert!(!calls[..patch].iter().any(is_reboot_edl));
    // Only the one read-back after the write.
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlRead { .. })), 1);
    assert_eq!(h.engine.deps().library.list(None).len(), 1);
}

#[test]
fn root_takes_backup_first_when_none() {
    let h = harness(stock_world());
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let summary = end.result.unwrap().backup.unwrap();
    assert!(summary.stock && summary.verified);
    let names: Vec<&str> = summary.files.iter().map(|f| f.name.as_str()).collect();
    for n in [
        "boot_a.img",
        "boot_b.img",
        "vbmeta_a.img",
        "vbmeta_b.img",
        "patched-boot.img",
    ] {
        assert!(names.contains(&n), "{n} missing from {names:?}");
    }
    let lib = &h.engine.deps().library;
    assert_eq!(lib.list(None).len(), 1);
    let (_, m) = lib.get(&summary.id).unwrap();
    assert!(m.patched.is_some());

    // All four partitions were read before the patch ran.
    let calls = h.world.calls();
    let patch = pos(&calls, is_patch);
    for part in ["boot_a", "boot_b", "vbmeta_a", "vbmeta_b"] {
        assert!(
            calls[..patch]
                .iter()
                .any(|c| matches!(c, Call::EdlRead { part: p } if p == part)),
            "{part} not read before patch"
        );
    }

    // R29: the whole backup phase is shown as step 2 of 4.
    let states: Vec<FlowState> = h
        .events
        .states()
        .into_iter()
        .filter(|s| s.flow_id == end.flow_id)
        .collect();
    let first_patch = states.iter().position(|s| s.step == StepId::Patch).unwrap();
    let phase: Vec<&FlowState> = states[..first_patch]
        .iter()
        .filter(|s| s.step != StepId::CheckDevice)
        .collect();
    assert!(!phase.is_empty());
    for s in &phase {
        assert_eq!(s.step_index, 2, "{s:?}");
        assert_eq!(s.step_count, 4, "{s:?}");
    }
    for st in [
        StepId::EnterEdl,
        StepId::ReadPartitions,
        StepId::VerifyBackup,
        StepId::RebootToSystem,
    ] {
        assert!(phase.iter().any(|s| s.step == st), "{st:?} not in phase");
    }
}

#[test]
fn root_already_rooted_fails() {
    let world = stock_world();
    {
        let mut w = world.lock();
        w.rooted = true;
        w.magisk_version = Some("30.7".into());
    }
    let h = harness(world);
    let end = run_root(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::AlreadyRooted);
    let calls = h.world.calls();
    assert!(!calls.iter().any(is_edl));
    assert!(!calls.iter().any(is_reboot_edl));
    assert!(!calls.iter().any(is_patch));
}

#[test]
fn root_refuses_non_stock_backup_suggests_import() {
    let world = stock_world();
    world.lock().faults.stock_check_exit = Some(1);
    let h = harness(world);
    let end = run_root(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::NoStockBackup);
    assert_eq!(e.message, NO_STOCK_MSG);
    assert_eq!(e.suggested_action, Some(SuggestedAction::ImportBackup));
    let calls = h.world.calls();
    assert!(!calls.iter().any(is_patch));
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })));
    assert_eq!(end.step_index, 2);
}

#[test]
fn root_patch_failure_makes_no_edl_calls() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    h.world.lock().faults.patch_fails = true;
    let end = run_root(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::PatchFailed);
    let calls = h.world.calls();
    assert!(calls.iter().any(is_patch));
    assert!(!calls.iter().any(is_reboot_edl), "{calls:#?}");
    assert!(!calls.iter().any(is_edl), "{calls:#?}");
    // No patched image recorded.
    let (dir, m) = h.engine.deps().library.get(&seeded.id).unwrap();
    assert!(m.patched.is_none());
    assert!(m.file("patched-boot.img").is_none());
    assert!(!dir.join("patched-boot.img").exists());
    // Never asked about the cable: the prompt only comes after a successful patch.
    assert!(
        !h.events
            .states()
            .iter()
            .any(|s| s.prompt.is_some() || s.step == StepId::CableCheck)
    );
}

#[test]
fn root_hash_mismatch_never_resets() {
    let h = harness(stock_world());
    seed_backup(&h);
    h.world.lock().faults.write_corrupts_times = 1;
    let end = run_root(&h, &[("cableCheck", "have"), ("retryWrite", "cancel")]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    let calls = h.world.calls();
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 0);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 1);
    assert_eq!(h.world.lock().mode, Mode::Edl);
    assert!(!end.writing);
}

#[test]
fn root_boot_timeout_offers_rescue() {
    let h = harness(stock_world());
    seed_backup(&h);
    h.world.lock().faults.boot_hangs = true;
    let end = run_root(
        &h,
        &[
            ("cableCheck", "have"),
            ("keepWaitingOrRescue", "goToRescue"),
        ],
    );
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::BootTimeout);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    assert_eq!(end.step, StepId::WaitBoot);
    assert_eq!(end.step_index, 4);
    // The write itself checked out and was followed by exactly one reset.
    assert_eq!(count(&h.world.calls(), |c| matches!(c, Call::EdlReset)), 1);
}

#[test]
fn root_verify_failure() {
    let h = harness(stock_world());
    seed_backup(&h);
    h.world.lock().faults.patch_drops_marker = true;
    let end = run_root(&h, &[("cableCheck", "have")]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::VerifyFailed);
    assert_eq!(e.message, VERIFY_FAILED_MSG);
    assert_eq!(e.suggested_action, None);
    assert_eq!(end.step, StepId::VerifyRoot);
    assert_eq!(end.step_index, 4);
    assert!(!h.world.lock().rooted);
    // It polled for a while (fake clock) rather than giving up at once.
    let magisk_checks = count(
        &h.world.calls(),
        |c| matches!(c, Call::Shell { cmd, .. } if cmd == "/debug_ramdisk/magisk -c"),
    );
    assert!(magisk_checks > 2, "only {magisk_checks} magisk checks");
}

#[test]
fn root_skips_cable_prompt_when_has_cable_true() {
    let h = harness(stock_world());
    h.settings
        .apply(SettingsPatch {
            has_rescue_cable: Some(Some(true)),
            ..SettingsPatch::default()
        })
        .unwrap();
    seed_backup(&h);
    let end = run_root(&h, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert!(
        !h.events
            .states()
            .iter()
            .any(|s| s.flow_id == end.flow_id && s.prompt.is_some())
    );
}

#[test]
fn root_refuses_tampered_backup() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    let img = std::path::Path::new(&seeded.path).join("boot_b.img");
    let mut bytes = std::fs::read(&img).unwrap();
    bytes[100] ^= 0xFF;
    std::fs::write(&img, bytes).unwrap();
    let end = run_root(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::BadImage);
    let calls = h.world.calls();
    assert!(!calls.iter().any(is_patch), "{calls:#?}");
    assert!(!calls.iter().any(is_edl), "{calls:#?}");
    assert!(!calls.iter().any(is_reboot_edl));
    assert!(!calls.iter().any(|c| matches!(c, Call::Push { .. })));
}

#[test]
fn root_patch_env_is_verity_safe() {
    let h = harness(stock_world());
    seed_backup(&h);
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let calls = h.world.calls();
    let cmd = calls
        .iter()
        .find_map(|c| match c {
            Call::Shell { cmd, .. } if cmd.contains("boot_patch.sh stock_boot.img") => {
                Some(cmd.clone())
            }
            _ => None,
        })
        .expect("patch command ran");
    assert!(
        cmd.contains("KEEPVERITY=true KEEPFORCEENCRYPT=true"),
        "{cmd}"
    );
    assert_eq!(cmd, PATCH_CMD);
    // The stock image pushed for patching is the backed-up active slot.
    assert!(calls.iter().any(|c| matches!(
        c,
        Call::Push { local_name, remote } if local_name == "boot_b.img"
            && remote == "/data/local/tmp/mgk/stock_boot.img"
    )));
    for remote in [
        "/data/local/tmp/mgk/init.bootsvc.rc",
        "/data/local/tmp/bootsvc.sh",
        "/data/local/tmp/service.sh",
    ] {
        assert!(
            calls
                .iter()
                .any(|c| matches!(c, Call::Push { remote: r, .. } if r == remote)),
            "{remote} not pushed"
        );
    }
}

#[test]
fn root_ignores_stock_backup_from_other_slot() {
    let world = stock_world();
    world
        .lock()
        .props
        .insert("ro.boot.slot_suffix".into(), "_a".into());
    let h = harness(world);
    let old = seed_backup(&h);
    assert_eq!(old.slot, inkroot_lib::ipc::Slot::A);
    // The active slot changes without a firmware change.
    h.world
        .lock()
        .props
        .insert("ro.boot.slot_suffix".into(), "_b".into());
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert_ne!(
        s.id, old.id,
        "the slot-A backup must not be used for slot B"
    );
    assert_eq!(s.slot, inkroot_lib::ipc::Slot::B);
    assert_eq!(h.engine.deps().library.list(None).len(), 2);
    // A fresh backup was read before patching, and the old one was left untouched.
    let calls = h.world.calls();
    let patch = pos(&calls, is_patch);
    assert!(
        calls[..patch]
            .iter()
            .any(|c| matches!(c, Call::EdlRead { .. }))
    );
    let (_, m) = h.engine.deps().library.get(&old.id).unwrap();
    assert!(m.patched.is_none());
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, Call::EdlWrite { part, .. } if part == "boot_b"))
    );
}

#[test]
fn root_bootsvc_log_read_error_fails_before_patch() {
    let h = harness(stock_world());
    seed_backup(&h);
    h.world.lock().faults.bootsvc_log_errors = 100;
    let end = run_root(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::Internal);
    assert_eq!(
        e.message,
        "Inkroot couldn't read your tablet's start-up log. Check the cable and try again."
    );
    assert_eq!(e.suggested_action, Some(SuggestedAction::Reconnect));
    let calls = h.world.calls();
    let reads = count(
        &calls,
        |c| matches!(c, Call::Shell { cmd, .. } if cmd == "cat /data/local/tmp/bootsvc.log"),
    );
    assert_eq!(reads, 3, "retried a few times");
    assert!(!calls.iter().any(is_patch));
    assert!(!calls.iter().any(|c| matches!(c, Call::Push { .. })));
    assert!(!calls.iter().any(is_edl));
    assert!(!calls.iter().any(is_reboot_edl));
}

#[test]
fn root_bootsvc_log_transient_error_is_retried() {
    let h = harness(stock_world());
    seed_backup(&h);
    h.world.lock().faults.bootsvc_log_errors = 1;
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
}

#[test]
fn root_uses_requested_backup_id() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    let older = clone_backup(&h, &seeded.id, "2000-01-01_0000", |_| {});
    h.settings
        .apply(SettingsPatch {
            has_rescue_cable: Some(Some(true)),
            ..SettingsPatch::default()
        })
        .unwrap();
    let end = run_to_end(&h, FlowKind::Root, with_backup_id(&older), &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.result.unwrap().backup.unwrap().id, older);
    // The named backup now records the patched image; the newer one is untouched.
    let lib = &h.engine.deps().library;
    assert!(lib.get(&older).unwrap().1.patched.is_some());
    assert!(lib.get(&seeded.id).unwrap().1.patched.is_none());
}

#[test]
fn root_refuses_unusable_backup_id_without_new_backup() {
    for (name, edit) in [
        (
            "2000-01-01_0001",
            Box::new(|m: &mut inkroot_lib::backup::Manifest| m.slot = inkroot_lib::ipc::Slot::A)
                as Box<dyn FnOnce(&mut inkroot_lib::backup::Manifest)>,
        ),
        ("2000-01-01_0002", Box::new(|m| m.stock = false)),
    ] {
        let h = harness(stock_world());
        let seeded = seed_backup(&h);
        let bad = clone_backup(&h, &seeded.id, name, edit);
        let before = h.engine.deps().library.list(None).len();
        let end = run_to_end(&h, FlowKind::Root, with_backup_id(&bad), &[]);
        let e = expect_failed(&end);
        assert_eq!(e.code, ErrorCode::NoStockBackup, "{e:?}");
        assert_eq!(e.suggested_action, Some(SuggestedAction::ImportBackup));
        assert_eq!(e.message, NO_STOCK_MSG);
        let calls = h.world.calls();
        assert!(
            !calls.iter().any(|c| is_reboot_edl(c) || is_edl(c)),
            "{calls:#?}"
        );
        // No silent fallback to a fresh backup.
        assert_eq!(h.engine.deps().library.list(None).len(), before);
    }
}

#[test]
fn root_refuses_missing_backup_id() {
    let h = harness(stock_world());
    seed_backup(&h);
    let end = run_to_end(&h, FlowKind::Root, with_backup_id("NMX1234567/nope"), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::NoStockBackup);
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| is_reboot_edl(c) || is_edl(c))
    );
}

#[test]
fn root_refuses_each_unusable_backup_id() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    for (bad, why) in unusable_backup_ids(&h, &seeded.id) {
        h.world.lock().calls.clear();
        let before = h.engine.deps().library.list(None).len();
        let end = run_to_end(&h, FlowKind::Root, with_backup_id(&bad), &[]);
        let e = expect_failed(&end);
        assert_eq!(e.code, ErrorCode::NoStockBackup, "{why}: {e:?}");
        assert_eq!(
            e.suggested_action,
            Some(SuggestedAction::ImportBackup),
            "{why}"
        );
        let calls = h.world.calls();
        assert!(
            !calls.iter().any(|c| is_reboot_edl(c) || is_edl(c)),
            "{why}: {calls:#?}"
        );
        assert_eq!(h.engine.deps().library.list(None).len(), before, "{why}");
        assert_eq!(h.world.lock().mode, Mode::System { authorized: true });
    }
}

fn has_cable(h: &Harness) {
    h.settings
        .apply(SettingsPatch {
            has_rescue_cable: Some(Some(true)),
            ..SettingsPatch::default()
        })
        .unwrap();
}

#[test]
fn root_driver_cancel_after_reboot_edl_resets_tablet() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.host.set_needs_driver(true);
    let end = run_root(&h, &[("installDriver", "cancel")]);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert!(calls.iter().any(is_reboot_edl), "{calls:#?}");
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 0);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 1);
    assert!(pos(&calls, is_reboot_edl) < pos(&calls, |c| matches!(c, Call::EdlReset)));
    assert_ne!(h.world.lock().mode, Mode::Edl);
}

#[test]
fn root_edl_not_found_cancel_resets_tablet_that_arrived_late() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.world.lock().faults.edl_never_appears = true;
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    // The tablet shows up just as the user gives up.
    h.world.lock().faults.edl_never_appears = false;
    h.engine.answer(&id, &p.id, "cancel");
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 0);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 1);
}

#[test]
fn root_cancel_after_failed_write_never_resets() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.world.lock().faults.write_corrupts_times = 1;
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    wait_prompt(&h, &id, "retryWrite");
    h.engine.cancel(&id).unwrap();
    let end = wait_terminal(&h, &id);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    let calls = h.world.calls();
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 1);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 0);
    assert_eq!(h.world.lock().mode, Mode::Edl);
}

#[test]
fn root_retry_with_changed_image_is_hash_mismatch_not_untouched() {
    let h = harness(stock_world());
    let seeded = seed_backup(&h);
    has_cable(&h);
    h.world.lock().faults.write_corrupts_times = 1;
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "retryWrite");
    // The prepared file changes on disk between the attempts.
    let (dir, _) = h.engine.deps().library.get(&seeded.id).unwrap();
    let f = dir.join("patched-boot.img");
    let mut b = std::fs::read(&f).unwrap();
    b[100] ^= 0xFF;
    std::fs::write(&f, b).unwrap();
    h.engine.answer(&id, &p.id, "retry");
    let end = wait_terminal(&h, &id);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch, "{e:?}");
    assert_eq!(e.message, inkroot_lib::flows::common::HASH_MISMATCH_MESSAGE);
    assert!(!e.message.contains("before touching"));
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    let calls = h.world.calls();
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 1);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 0);
}

#[test]
fn root_retry_with_tablet_gone_asks_and_cancel_points_to_rescue() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.world.lock().faults.write_corrupts_times = 1;
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "retryWrite");
    // Cable pulled while the user decides.
    h.world.lock().mode = Mode::Off;
    h.engine.answer(&id, &p.id, "retry");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
    let end = drive(&h, &id, &[("edlNotFound", "cancel")]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch, "{e:?}");
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    let calls = h.world.calls();
    // The second `edl w` never started.
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 1);
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 0);
    assert!(
        !h.events
            .states()
            .iter()
            .any(|s| s.flow_id == id && s.writing && s.prompt.is_some())
    );
}

#[test]
fn root_tablet_gone_before_first_write_asks_then_writes_on_retry() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.host.set_needs_driver(true);
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "installDriver");
    h.world.lock().mode = Mode::Off;
    h.engine.answer(&id, &p.id, "install");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
    let p = wait_prompt(&h, &id, "edlNotFound");
    assert_eq!(
        count(&h.world.calls(), |c| matches!(c, Call::EdlWrite { .. })),
        0
    );
    h.world.lock().mode = Mode::Edl;
    h.engine.answer(&id, &p.id, "retry");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(
        count(&h.world.calls(), |c| matches!(c, Call::EdlWrite { .. })),
        1
    );
}

#[test]
fn root_tablet_gone_before_first_write_cancel_is_cancelled() {
    let h = harness(stock_world());
    seed_backup(&h);
    has_cable(&h);
    h.host.set_needs_driver(true);
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "installDriver");
    h.world.lock().mode = Mode::Off;
    h.engine.answer(&id, &p.id, "install");
    wait_state(&h, &id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
    let end = drive(&h, &id, &[("edlNotFound", "cancel")]);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlWrite { .. })), 0);
    // Nothing in EDL to restart.
    assert_eq!(count(&calls, |c| matches!(c, Call::EdlReset)), 0);
}

// R44: no legacy trust for imported backups.

/// Root on an unrooted tablet whose only "stock" backup is the imported `id`: it must not be
/// used; Root makes its own backup first (all four partitions read before the patch).
fn assert_root_ignores_import(h: &Harness, id: &str) {
    // Asking for the import by id is refused outright, before anything touches the tablet.
    let end = run_to_end(h, FlowKind::Root, with_backup_id(id), &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::NoStockBackup);
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| is_edl(c) || is_reboot_edl(c))
    );
    // Without an id, Root makes its own backup first.
    let end = run_root(h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let used = end.result.unwrap().backup.unwrap();
    assert_ne!(used.id, id);
    assert_eq!(used.source, inkroot_lib::ipc::BackupSource::Inkroot);
    let calls = h.world.calls();
    let patch = pos(&calls, is_patch);
    assert!(
        calls[..patch]
            .iter()
            .any(|c| matches!(c, Call::EdlRead { part } if part == "boot_a")),
        "no fresh backup before the patch"
    );
}

#[test]
fn root_ignores_imported_backup_with_magiskboot_evidence() {
    let h = harness(stock_world());
    let id = import_world_boot_b(&h, legacy_stock(StockEvidence::MagiskbootTest));
    assert_root_ignores_import(&h, &id);
}

#[test]
fn root_ignores_imported_known_hash_no_longer_listed() {
    // The profile's list is empty: a KnownHash import (as 1aeb/boot_a once was) is stale.
    let h = harness(stock_world());
    let id = import_world_boot_b(&h, legacy_stock(StockEvidence::KnownHash));
    assert_root_ignores_import(&h, &id);
}

#[test]
fn root_uses_valid_known_hash_import() {
    let h = harness_opts(stock_world(), true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![world_boot_b_known()];
    });
    let id = import_world_boot_b(&h, |m| {
        assert!(m.stock);
        assert_eq!(m.stock_evidence, StockEvidence::KnownHash);
    });
    let end = run_root(&h, &[("cableCheck", "have")]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.result.unwrap().backup.unwrap().id, id);
    // Only the read-back after the write: no fresh backup.
    assert_eq!(
        count(&h.world.calls(), |c| matches!(c, Call::EdlRead { .. })),
        1
    );
    // And by explicit id.
    let h = harness_opts(stock_world(), true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![world_boot_b_known()];
    });
    let id = import_world_boot_b(&h, |_| {});
    let end = run_to_end(
        &h,
        FlowKind::Root,
        with_backup_id(&id),
        &[("cableCheck", "have")],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
}
