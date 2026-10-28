mod support;

use inkroot_lib::clock::Clock;
use inkroot_lib::fakes::{Call, FakeAwake, FakeEdl, Mode, fake_boot_image};
use inkroot_lib::flows::common::{
    cable_check, check_device, enter_edl, wait_boot, with_keep_awake, write_and_verify,
};
use inkroot_lib::flows::{FlowCtx, FlowEnd, prompt_options};
use inkroot_lib::ipc::{
    Emphasis, ErrorCode, FlowKind, FlowOptions, FlowResult, FlowState, FlowStatus, PromptKind,
    SettingsPatch, StepId, SuggestedAction,
};
use inkroot_lib::tools::ToolError;
use inkroot_lib::tools::adb::RebootTarget;
use inkroot_lib::tools::edl::Edl;
use inkroot_lib::tools::process::CancelToken;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use support::*;

const BUSY_MSG: &str = "Inkroot is already working on something. Let it finish first.";
const RISK_MSG: &str = "Please read and accept the risk notice first.";
const WRITING_MSG: &str =
    "Inkroot is writing to your tablet right now. Cancelling could damage it.";
const MISMATCH_MSG: &str = "The copy on your tablet doesn't match yet, so Inkroot didn't restart it. Re-seat the cable and try again, or use Rescue.";
const BOOT_TIMEOUT_MSG: &str = "Your tablet hasn't restarted yet.";

type FlowOut = Result<FlowResult, FlowEnd>;

fn ok() -> FlowOut {
    Ok(FlowResult::default())
}

/// A flow that asks one `edlNotFound` prompt and finishes when it is answered.
fn one_prompt_flow(ctx: &FlowCtx) -> FlowOut {
    ctx.step(StepId::EnterEdl, 1, 1);
    ctx.ask(PromptKind::EdlNotFound)?;
    ok()
}

/// Writes a boot image fixture; returns (path, sha).
fn image_fixture(dir: &Path, seed: u8) -> (PathBuf, String) {
    let p = dir.join(format!("img-{seed}.img"));
    std::fs::write(&p, fake_boot_image(4096, seed)).unwrap();
    let sha = inkroot_lib::image::sha256_file(&p).unwrap();
    (p, sha)
}

/// check_device → enter_edl → write_and_verify(boot_b) as one test-only flow.
fn write_flow(img: PathBuf, sha: String, scratch: PathBuf) -> impl FnOnce(&FlowCtx) -> FlowOut {
    move |ctx| {
        ctx.step(StepId::CheckDevice, 1, 3);
        let (serial, _info, _profile) = check_device(ctx, None)?;
        ctx.step(StepId::EnterEdl, 2, 3);
        enter_edl(ctx, Some(&serial))?;
        ctx.step(StepId::Write, 3, 3);
        write_and_verify(ctx, "boot_b", &img, &sha, &scratch)?;
        ok()
    }
}

fn assert_invariants(states: &[FlowState]) {
    for s in states {
        assert_eq!(
            s.prompt.is_some(),
            s.status == FlowStatus::WaitingForUser,
            "prompt iff waitingForUser: {s:?}"
        );
        assert_eq!(
            s.error.is_some(),
            s.status == FlowStatus::Failed,
            "error iff failed: {s:?}"
        );
        assert_eq!(
            s.result.is_some(),
            s.status == FlowStatus::Done,
            "result iff done: {s:?}"
        );
        assert!(s.step_index >= 1 && s.step_index <= s.step_count, "{s:?}");
    }
}

#[test]
fn prompt_options_match_contract() {
    let ids = |k| {
        prompt_options(k)
            .into_iter()
            .map(|o| (o.id, o.emphasis))
            .collect::<Vec<_>>()
    };
    let s = |v: &[(&str, Emphasis)]| {
        v.iter()
            .map(|(a, e)| (a.to_string(), *e))
            .collect::<Vec<_>>()
    };
    use Emphasis::*;
    assert_eq!(
        ids(PromptKind::CableCheck),
        s(&[
            ("have", Primary),
            ("whereToGet", Secondary),
            ("continueWithout", Danger)
        ])
    );
    assert_eq!(
        ids(PromptKind::CableCheckConfirm),
        s(&[("confirmContinue", Danger), ("back", Primary)])
    );
    assert_eq!(
        ids(PromptKind::ChooseSlot),
        s(&[("a", Secondary), ("b", Secondary)])
    );
    assert_eq!(
        ids(PromptKind::KeepWaitingOrRescue),
        s(&[("keepWaiting", Primary), ("goToRescue", Secondary)])
    );
    for k in [PromptKind::InstallDriver, PromptKind::InstallUdev] {
        assert_eq!(ids(k), s(&[("install", Primary), ("cancel", Secondary)]));
    }
    for k in [PromptKind::EdlNotFound, PromptKind::RetryWrite] {
        assert_eq!(ids(k), s(&[("retry", Primary), ("cancel", Secondary)]));
    }
    assert_eq!(
        ids(PromptKind::ConnectionLostAfterWrite),
        s(&[
            ("restartedOk", Primary),
            ("runAgain", Secondary),
            ("cancel", Secondary)
        ])
    );
    assert_eq!(
        ids(PromptKind::StillInEdl),
        s(&[("retry", Primary), ("heldPower", Secondary)])
    );
    assert_eq!(
        ids(PromptKind::TabletStarting),
        s(&[("homeScreen", Primary)])
    );
    assert_eq!(ids(PromptKind::CableGuide), s(&[("done", Primary)]));
    let labels: Vec<String> = prompt_options(PromptKind::CableCheck)
        .into_iter()
        .map(|o| o.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Yes, I have one",
            "Show me where to get one",
            "Continue without one"
        ]
    );
    assert_eq!(
        prompt_options(PromptKind::InstallUdev)[0].label,
        "Allow access"
    );
}

#[test]
fn start_rejects_second_flow_busy() {
    let h = harness(stock_world());
    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(one_prompt_flow),
        )
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    for kind in [FlowKind::Backup, FlowKind::ImportBackup, FlowKind::Root] {
        let e = h
            .engine
            .start_with(kind, FlowOptions::default(), Box::new(|_| ok()))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
        assert_eq!(e.message, BUSY_MSG);
        let e = h.engine.start(kind, FlowOptions::default()).unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
    }
    h.engine.answer(&id, &p.id, "retry");
    assert_eq!(wait_terminal(&h, &id).status, FlowStatus::Done);

    // Race-free: right after observing a terminal state, start() never reports busy.
    for _ in 0..50 {
        let id = h
            .engine
            .start_with(FlowKind::Backup, FlowOptions::default(), Box::new(|_| ok()))
            .expect("not busy after terminal state");
        assert_eq!(wait_terminal(&h, &id).status, FlowStatus::Done);
    }
    assert!(!h.engine.writing());
}

#[test]
fn risk_gate_blocks_root_unroot_rescue_until_accepted() {
    let h = harness_without_risk(stock_world());
    for kind in [FlowKind::Root, FlowKind::Unroot, FlowKind::Rescue] {
        let e = h.engine.start(kind, FlowOptions::default()).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal, "{kind:?}");
        assert_eq!(e.message, RISK_MSG, "{kind:?}");
        let e = h
            .engine
            .start_with(kind, FlowOptions::default(), Box::new(|_| ok()))
            .unwrap_err();
        assert_eq!(e.message, RISK_MSG, "{kind:?}");
    }
    assert!(h.events.states().is_empty());
    // Other kinds aren't gated.
    let st = run_flow_with(&h, FlowKind::Backup, FlowOptions::default(), |_| ok(), &[]);
    assert_eq!(st.status, FlowStatus::Done);

    h.settings
        .apply(SettingsPatch {
            accepted_risk_version: Some(Some(1)),
            ..SettingsPatch::default()
        })
        .unwrap();
    for kind in [FlowKind::Root, FlowKind::Unroot, FlowKind::Rescue] {
        let st = run_flow_with(&h, kind, FlowOptions::default(), |_| ok(), &[]);
        assert_eq!(st.status, FlowStatus::Done, "{kind:?}");
    }
}

#[test]
fn start_runs_tool_check() {
    let h = harness(stock_world());
    let loader = h.engine.deps().tools.loader(&test_profile().edl.loader);
    std::fs::remove_file(loader).unwrap();
    let e = h
        .engine
        .start_with(FlowKind::Backup, FlowOptions::default(), Box::new(|_| ok()))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ToolMissing);
    // importBackup needs no tools.
    let st = run_flow_with(
        &h,
        FlowKind::ImportBackup,
        FlowOptions::default(),
        |_| ok(),
        &[],
    );
    assert_eq!(st.status, FlowStatus::Done);
}

#[test]
fn every_kind_is_registered() {
    // Task 12 registered the last flows: every kind now starts (this unrooted tablet makes
    // reinstall end calmly with notRooted), and the engine is not left busy.
    let h = harness(stock_world());
    let st = run_to_end(
        &h,
        FlowKind::ReinstallBootService,
        FlowOptions::default(),
        &[],
    );
    assert_eq!(st.status, FlowStatus::Failed, "{st:?}");
    assert_eq!(st.error.unwrap().code, ErrorCode::NotRooted);
    let st = run_flow_with(&h, FlowKind::Backup, FlowOptions::default(), |_| ok(), &[]);
    assert_eq!(st.status, FlowStatus::Done);
}

#[test]
fn cancel_rejected_while_writing() {
    let h = harness(stock_world());
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let id = h
        .engine
        .start_with(
            FlowKind::Unroot,
            FlowOptions::default(),
            Box::new(move |ctx| {
                ctx.step(StepId::Write, 1, 1);
                ctx.set_writing(true);
                release_rx.recv_timeout(Duration::from_secs(10)).ok();
                ctx.set_writing(false);
                ctx.check_cancel()?;
                ok()
            }),
        )
        .unwrap();
    wait_state(&h, &id, |s| s.writing);
    assert!(h.engine.writing());
    let e = h.engine.cancel(&id).unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
    assert_eq!(e.message, WRITING_MSG);
    release_tx.send(()).unwrap();
    let end = wait_terminal(&h, &id);
    assert_eq!(
        end.status,
        FlowStatus::Done,
        "rejected cancel must not cancel"
    );
    assert!(!end.writing);
    assert!(!h.engine.writing());

    // Not writing: cancel wakes a pending prompt and the flow ends cancelled.
    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(one_prompt_flow),
        )
        .unwrap();
    wait_prompt(&h, &id, "edlNotFound");
    h.engine.cancel(&id).unwrap();
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Cancelled);
    assert!(end.prompt.is_none() && end.error.is_none() && end.result.is_none());
    assert_eq!(h.engine.current().unwrap().status, FlowStatus::Cancelled);
}

#[test]
fn stale_prompt_answer_ignored() {
    let h = harness(stock_world());
    let answers = Arc::new(Mutex::new(Vec::new()));
    let a2 = answers.clone();
    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(move |ctx| {
                ctx.step(StepId::EnterEdl, 1, 1);
                let first = ctx.ask(PromptKind::EdlNotFound)?;
                let second = ctx.ask(PromptKind::EdlNotFound)?;
                a2.lock().unwrap().extend([first, second]);
                ok()
            }),
        )
        .unwrap();
    let p1 = wait_prompt(&h, &id, "edlNotFound");
    // Unknown prompt id, wrong flow id and an option that isn't offered: all ignored.
    h.engine.answer(&id, "not-a-prompt", "retry");
    h.engine.answer("other-flow", &p1.id, "retry");
    h.engine.answer(&id, &p1.id, "bogus");
    std::thread::sleep(Duration::from_millis(50));
    let s = latest_state(&h, &id).unwrap();
    assert_eq!(s.status, FlowStatus::WaitingForUser);
    assert_eq!(s.prompt.as_ref().unwrap().id, p1.id);

    h.engine.answer(&id, &p1.id, "retry");
    let p2 = wait_state(&h, &id, |s| {
        s.prompt.as_ref().is_some_and(|p| p.id != p1.id)
    })
    .prompt
    .unwrap();
    assert_ne!(p1.id, p2.id, "prompt ids are unique per instance");
    // Answering the first (stale) prompt again doesn't answer the second.
    h.engine.answer(&id, &p1.id, "cancel");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        latest_state(&h, &id).unwrap().prompt.unwrap().id,
        p2.id,
        "stale answer must be ignored"
    );
    h.engine.answer(&id, &p2.id, "cancel");
    assert_eq!(wait_terminal(&h, &id).status, FlowStatus::Done);
    assert_eq!(*answers.lock().unwrap(), ["retry", "cancel"]);
}

#[test]
fn write_and_verify_mismatch_prompts_retry_and_never_resets() {
    let h = harness(stock_world());
    h.world.lock().faults.write_corrupts_times = 1;
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img, sha, scratch.clone()),
        &[("retryWrite", "cancel")],
    );
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::HashMismatch);
    assert_eq!(e.message, MISMATCH_MSG);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    assert!(!end.writing);
    let calls = h.world.calls();
    assert!(
        !calls.contains(&Call::EdlReset),
        "never reset after a mismatch"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::EdlWrite { .. }))
            .count(),
        1
    );
    assert_eq!(h.world.lock().mode, Mode::Edl, "tablet stays in EDL");
    assert!(!scratch.join(".verify-boot_b.img").exists());
}

#[test]
fn write_error_prompts_retry_and_never_resets() {
    let h = harness(stock_world());
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    let world = h.world.clone();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        move |ctx| {
            ctx.step(StepId::Write, 1, 1);
            world.lock().mode = Mode::Edl;
            // Make the write itself fail (edl times out) with the tablet still in EDL.
            world.lock().faults.write_errors = 2;
            write_and_verify(ctx, "boot_b", &img, &sha, &scratch)?;
            ok()
        },
        &[("retryWrite", "retry"), ("retryWrite", "cancel")],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::HashMismatch);
    assert!(!h.world.calls().contains(&Call::EdlReset));
}

#[test]
fn write_retry_then_match_resets_exactly_once() {
    let h = harness(stock_world());
    h.world.lock().faults.write_corrupts_times = 1;
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img.clone(), sha, scratch.clone()),
        &[("retryWrite", "retry")],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let calls = h.world.calls();
    assert_eq!(calls.iter().filter(|c| **c == Call::EdlReset).count(), 1);
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::EdlWrite { .. }))
            .count(),
        2
    );
    // Reset is the last edl call, after the matching read-back.
    let last_edl = calls
        .iter()
        .rev()
        .find(|c| {
            matches!(
                c,
                Call::EdlWrite { .. } | Call::EdlRead { .. } | Call::EdlReset
            )
        })
        .unwrap();
    assert_eq!(*last_edl, Call::EdlReset);
    assert_eq!(
        h.world.lock().partitions["boot_b"],
        std::fs::read(&img).unwrap()
    );
    assert!(!scratch.join(".verify-boot_b.img").exists());
    assert!(!h.events.progress().is_empty());
    let steps: Vec<StepId> = h.events.states().iter().map(|s| s.step).collect();
    for st in [StepId::Write, StepId::VerifyWrite, StepId::Reset] {
        assert!(steps.contains(&st), "{st:?} in {steps:?}");
    }
}

#[test]
fn write_refuses_changed_image_bad_image() {
    let h = harness(stock_world());
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    std::fs::write(&img, fake_boot_image(4096, 10)).unwrap();
    let scratch = h.lib_dir.path().to_path_buf();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img, sha, scratch),
        &[],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::BadImage);
    let calls = h.world.calls();
    assert!(
        !calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })),
        "{calls:?}"
    );
    // Nothing was written, so the tablet the flow sent into EDL is restarted, not stranded.
    assert_eq!(
        calls.iter().filter(|c| **c == Call::EdlReset).count(),
        1,
        "{calls:?}"
    );
}

#[test]
fn enter_edl_timeout_offers_tips_and_continues_when_the_tablet_shows_up() {
    let h = harness(stock_world());
    h.world.lock().faults.edl_never_appears = true;
    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(|ctx| {
                ctx.step(StepId::CheckDevice, 1, 2);
                let (serial, _, _) = check_device(ctx, None)?;
                ctx.step(StepId::EnterEdl, 2, 2);
                enter_edl(ctx, Some(&serial))?;
                ok()
            }),
        )
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    assert!(h.clock.now() >= Duration::from_secs(60), "waited wait_edl");
    // "Try again" takes the tips down and starts the wait over; they come back after 60 s.
    let again = h.clock.now();
    h.engine.answer(&id, &p.id, "retry");
    let p2 = wait_state(&h, &id, |s| s.prompt.as_ref().is_some_and(|q| q.id != p.id))
        .prompt
        .unwrap();
    assert_eq!(prompt_literal(&p2), "edlNotFound");
    assert!(h.clock.now() - again >= Duration::from_secs(60));
    // The tips don't block: the tablet shows up and the flow goes on with nothing answered.
    h.world.lock().faults.edl_never_appears = false;
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let reboots = h
        .world
        .calls()
        .into_iter()
        .filter(|c| {
            matches!(
                c,
                Call::Reboot {
                    target: RebootTarget::Edl,
                    ..
                }
            )
        })
        .count();
    assert_eq!(reboots, 1, "retry must not re-issue adb reboot edl");

    // Cancel on the prompt ends the flow cancelled.
    h.world.lock().mode = Mode::System { authorized: true };
    h.world.lock().faults.edl_never_appears = true;
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::EnterEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[("edlNotFound", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Cancelled);
}

#[test]
fn enter_edl_skips_reboot_when_already_in_edl() {
    let h = harness(stock_world());
    h.world.lock().mode = Mode::Edl;
    let end = run_flow_with(
        &h,
        FlowKind::Rescue,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::WaitEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Reboot { .. }))
    );
}

#[test]
fn enter_edl_windows_driver_prompt() {
    let h = harness(stock_world());
    h.host.set_needs_driver(true);
    let flow = |ctx: &FlowCtx| {
        ctx.step(StepId::EnterEdl, 1, 1);
        enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
        ok()
    };
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        flow,
        &[("installDriver", "install")],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert!(h.world.calls().contains(&Call::DriverInstall));

    // Declining ends the flow cancelled.
    h.host.set_needs_driver(true);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        flow,
        &[("installDriver", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Cancelled);
}

#[test]
fn enter_edl_windows_installs_missing_driver_before_rebooting() {
    let h = harness(stock_world());
    h.host.set_needs_driver_ahead(true);
    let flow = |ctx: &FlowCtx| {
        ctx.step(StepId::EnterEdl, 1, 1);
        enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
        ok()
    };
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        flow,
        &[("installDriver", "install")],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let calls = h.world.calls();
    let install = calls.iter().position(|c| *c == Call::DriverInstall);
    let reboot = calls.iter().position(support::is_reboot_edl_call);
    assert!(
        matches!((install, reboot), (Some(i), Some(r)) if i < r),
        "driver must be installed before adb reboot edl: {calls:?}"
    );
    // Asked once: the check in EDL finds the driver in place.
    assert_eq!(
        calls.iter().filter(|c| **c == Call::DriverInstall).count(),
        1
    );

    // Declining ends the flow before the tablet is sent anywhere.
    let h = harness(stock_world());
    h.host.set_needs_driver_ahead(true);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        flow,
        &[("installDriver", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Cancelled);
    let calls = h.world.calls();
    assert!(!calls.iter().any(support::is_reboot_edl_call), "{calls:?}");
    assert!(!calls.iter().any(support::is_edl_call), "{calls:?}");
}

#[test]
fn enter_edl_pre_check_skipped_when_already_in_edl() {
    let h = harness(stock_world());
    h.world.lock().mode = Mode::Edl;
    h.host.set_needs_driver_ahead(true);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::EnterEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[("installDriver", "install")],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let calls = h.world.calls();
    assert_eq!(
        calls.iter().filter(|c| **c == Call::DriverInstall).count(),
        1
    );
    assert!(!calls.iter().any(support::is_reboot_edl_call));
}

#[test]
fn enter_edl_udev_prompt() {
    let h = harness(stock_world());
    h.host.set_needs_udev(true);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::EnterEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[("installUdev", "install")],
    );
    assert_eq!(end.status, FlowStatus::Done);
    // Asked ahead, while the tablet is still in Android, like the Windows driver.
    let calls = h.world.calls();
    let install = calls.iter().position(|c| *c == Call::UdevInstall).unwrap();
    let reboot = calls.iter().position(support::is_reboot_edl_call).unwrap();
    assert!(install < reboot, "{calls:?}");
}

#[test]
fn enter_edl_udev_rechecks_until_access_arrives() {
    // The rule is in but udev takes a moment to reach the device: Inkroot waits it out.
    let h = harness(stock_world());
    h.world.lock().mode = Mode::Edl;
    h.world.lock().faults.usb_denied = true;
    h.host.set_udev_settle_checks(3);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::EnterEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[("installUdev", "install")],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert!(
        h.clock.now() >= Duration::from_millis(1500),
        "waited for udev"
    );

    // Never settling within UDEV_SETTLE: a clear permission error.
    let h = harness(stock_world());
    h.world.lock().mode = Mode::Edl;
    h.world.lock().faults.usb_denied = true;
    h.host.set_udev_settle_checks(1000);
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::EnterEdl, 1, 1);
            enter_edl(ctx, Some(support::NOTE_MAX_SERIAL))?;
            ok()
        },
        &[("installUdev", "install")],
    );
    assert_eq!(end.status, FlowStatus::Failed, "{end:?}");
    assert_eq!(end.error.unwrap().code, ErrorCode::UsbPermission);
}

fn cable_flow(ctx: &FlowCtx) -> FlowOut {
    ctx.step(StepId::CableCheck, 1, 1);
    cable_check(ctx)?;
    ok()
}

#[test]
fn cable_check_skips_when_has_cable_true() {
    let h = harness(stock_world());
    h.settings
        .apply(SettingsPatch {
            has_rescue_cable: Some(Some(true)),
            ..SettingsPatch::default()
        })
        .unwrap();
    let end = run_flow_with(&h, FlowKind::Root, FlowOptions::default(), cable_flow, &[]);
    assert_eq!(end.status, FlowStatus::Done);
    assert!(h.events.states().iter().all(|s| s.prompt.is_none()));
}

#[test]
fn cable_check_have_persists_true() {
    let h = harness(stock_world());
    let end = run_flow_with(
        &h,
        FlowKind::Root,
        FlowOptions::default(),
        cable_flow,
        &[("cableCheck", "whereToGet"), ("cableCheck", "have")],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert_eq!(h.settings.get().has_rescue_cable, Some(true));
}

#[test]
fn cable_check_continue_requires_confirm() {
    let h = harness(stock_world());
    let end = run_flow_with(
        &h,
        FlowKind::Root,
        FlowOptions::default(),
        cable_flow,
        &[
            ("cableCheck", "continueWithout"),
            ("cableCheckConfirm", "confirmContinue"),
        ],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert_eq!(h.settings.get().has_rescue_cable, Some(false));
}

#[test]
fn cable_check_back_reasks() {
    let h = harness(stock_world());
    let end = run_flow_with(
        &h,
        FlowKind::Root,
        FlowOptions::default(),
        cable_flow,
        &[
            ("cableCheck", "continueWithout"),
            ("cableCheckConfirm", "back"),
            ("cableCheck", "have"),
        ],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert_eq!(h.settings.get().has_rescue_cable, Some(true));
}

#[test]
fn wait_boot_timeout_offers_keep_waiting_or_rescue() {
    let h = harness(stock_world());
    {
        let mut w = h.world.lock();
        w.mode = Mode::Off;
        w.pending_boot = true;
        w.faults.boot_hangs = true;
    }
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::WaitBoot, 1, 1);
            wait_boot(ctx, support::NOTE_MAX_SERIAL)?;
            ok()
        },
        &[
            ("keepWaitingOrRescue", "keepWaiting"),
            ("keepWaitingOrRescue", "goToRescue"),
        ],
    );
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::BootTimeout);
    assert_eq!(e.message, BOOT_TIMEOUT_MSG);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    assert!(h.clock.now() >= Duration::from_secs(360));

    // keepWaiting after the tablet finally boots → done.
    h.world.lock().faults.boot_hangs = true;
    let world = h.world.clone();
    let id = h
        .engine
        .start_with(
            FlowKind::Unroot,
            FlowOptions::default(),
            Box::new(|ctx| {
                ctx.step(StepId::WaitBoot, 1, 1);
                wait_boot(ctx, support::NOTE_MAX_SERIAL)?;
                ok()
            }),
        )
        .unwrap();
    let p = wait_prompt(&h, &id, "keepWaitingOrRescue");
    world.lock().faults.boot_hangs = false;
    h.engine.answer(&id, &p.id, "keepWaiting");
    assert_eq!(wait_terminal(&h, &id).status, FlowStatus::Done);
}

#[test]
fn check_device_maps_statuses() {
    let h = harness(stock_world());
    let flow = |want: Option<bool>| {
        move |ctx: &FlowCtx| {
            ctx.step(StepId::CheckDevice, 1, 1);
            check_device(ctx, want)?;
            ok()
        }
    };
    let code_of = |h: &Harness, want: Option<bool>| {
        let end = run_flow_with(h, FlowKind::Backup, FlowOptions::default(), flow(want), &[]);
        end.error.map(|e| (e.code, e.suggested_action))
    };
    assert_eq!(code_of(&h, None), None);
    assert_eq!(
        code_of(&h, Some(true)).map(|c| c.0),
        Some(ErrorCode::NotRooted)
    );
    h.world.lock().battery = 10;
    assert_eq!(
        code_of(&h, None),
        Some((ErrorCode::LowBattery, Some(SuggestedAction::ChargeDevice)))
    );
    h.world.lock().battery = 80;
    h.world.lock().mode = Mode::System { authorized: false };
    assert_eq!(
        code_of(&h, None).map(|c| c.0),
        Some(ErrorCode::Unauthorized)
    );
    h.world.lock().mode = Mode::Off;
    assert_eq!(
        code_of(&h, None),
        Some((ErrorCode::NoDevice, Some(SuggestedAction::Reconnect)))
    );
    h.world.lock().mode = Mode::Edl;
    assert_eq!(
        code_of(&h, None),
        Some((ErrorCode::NoDevice, Some(SuggestedAction::GoToRescue)))
    );
    h.world.lock().mode = Mode::System { authorized: true };
    h.world
        .lock()
        .props
        .insert("ro.build.id".into(), "XYZ".into());
    assert_eq!(
        code_of(&h, None).map(|c| c.0),
        Some(ErrorCode::UnknownFirmware)
    );
    h.world
        .lock()
        .props
        .insert("ro.product.model".into(), "Pixel".into());
    assert_eq!(
        code_of(&h, None).map(|c| c.0),
        Some(ErrorCode::UnknownModel)
    );
}

#[test]
fn check_device_already_rooted() {
    let h = harness(stock_world());
    {
        let mut w = h.world.lock();
        w.rooted = true;
        w.magisk_version = Some("30.7".into());
    }
    let end = run_flow_with(
        &h,
        FlowKind::Root,
        FlowOptions::default(),
        |ctx| {
            ctx.step(StepId::CheckDevice, 1, 1);
            check_device(ctx, Some(false))?;
            ok()
        },
        &[],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::AlreadyRooted);
}

/// Records how many keep-awake guards were active at each write.
struct SpyEdl {
    inner: FakeEdl,
    awake: Arc<FakeAwake>,
    seen: Arc<Mutex<Vec<usize>>>,
}

impl Edl for SpyEdl {
    fn read(
        &self,
        part: &str,
        out: &Path,
        cancel: &CancelToken,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), ToolError> {
        self.inner.read(part, out, cancel, progress)
    }
    fn write(&self, part: &str, img: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError> {
        self.seen.lock().unwrap().push(self.awake.active());
        self.inner.write(part, img, progress)
    }
    fn reset(&self) -> Result<(), ToolError> {
        self.inner.reset()
    }
}

#[test]
fn keep_awake_held_during_write() {
    let world = stock_world();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let w2 = world.clone();
    let mut awake_slot = None;
    let h = harness_opts(world, true, |deps| {
        let awake = Arc::new(FakeAwake::new());
        deps.awake = awake.clone();
        awake_slot = Some(awake.clone());
        deps.edl = Arc::new(SpyEdl {
            inner: FakeEdl::new(w2),
            awake,
            seen: seen2,
        });
    });
    let awake = awake_slot.unwrap();
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img, sha, scratch),
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(
        *seen.lock().unwrap(),
        [1],
        "one guard active during the write"
    );
    assert_eq!(awake.active(), 0, "released afterwards");
    assert!(awake.holds() >= 1);

    // with_keep_awake on its own.
    let hits = Arc::new(AtomicUsize::new(0));
    let hits2 = hits.clone();
    let awake2 = awake.clone();
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        move |ctx| {
            ctx.step(StepId::Write, 1, 1);
            let n = with_keep_awake(ctx, || awake2.active());
            hits2.store(n, Ordering::SeqCst);
            ok()
        },
        &[],
    );
    assert_eq!(end.status, FlowStatus::Done);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn set_kind_and_logs_are_tagged() {
    let h = harness(stock_world());
    let id = h
        .engine
        .start_with(
            FlowKind::ReinstallBootService,
            FlowOptions::default(),
            Box::new(|ctx| {
                ctx.step(StepId::CheckDevice, 1, 2);
                ctx.set_kind(FlowKind::Root);
                ctx.step(StepId::Patch, 3, 4);
                ok()
            }),
        )
        .unwrap();
    let end = wait_terminal(&h, &id);
    assert_eq!(end.kind, FlowKind::Root);
    assert_eq!(end.flow_id, id);
    assert!(h.engine.current().is_some_and(|s| s.flow_id == id));
    let logs = h.log.text(Some(&id));
    assert!(!logs.is_empty());
    // After the flow, log lines are no longer tagged with it.
    h.log.info("after");
    assert!(!h.log.text(Some(&id)).contains("after"));
}

#[test]
fn panicking_flow_fails_internal_and_frees_engine() {
    let h = harness(stock_world());
    let end = run_flow_with(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        |_| panic!("boom"),
        &[],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::Internal);
    let st = run_flow_with(&h, FlowKind::Backup, FlowOptions::default(), |_| ok(), &[]);
    assert_eq!(st.status, FlowStatus::Done);
}

#[test]
fn edl_snapshot_invariants() {
    let h = harness(stock_world());
    h.world.lock().faults.write_corrupts_times = 1;
    h.host.set_needs_driver(true);
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    // done, with driver prompt and a retry
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img.clone(), sha.clone(), scratch.clone()),
        &[("installDriver", "install"), ("retryWrite", "retry")],
    );
    assert_eq!(end.status, FlowStatus::Done);
    // failed
    h.world.lock().faults.write_corrupts_times = 1;
    h.world.lock().mode = Mode::System { authorized: true };
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img, sha, scratch),
        &[("retryWrite", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Failed);
    // cancelled
    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(one_prompt_flow),
        )
        .unwrap();
    wait_prompt(&h, &id, "edlNotFound");
    h.engine.cancel(&id).unwrap();
    wait_terminal(&h, &id);

    let states = h.events.states();
    assert!(states.len() > 10);
    assert_invariants(&states);
    assert!(states.iter().any(|s| s.writing));
    for s in states.iter().filter(|s| is_terminal(s.status)) {
        assert!(!s.writing, "{s:?}");
    }
}

#[test]
fn poller_paused_while_flow_active() {
    use inkroot_lib::fakes::{FakeAdb, FakeUsb};
    use inkroot_lib::poller::Poller;
    let world = stock_world();
    let w2 = world.clone();
    let mut slot = None;
    let h = harness_opts(world, true, |deps| {
        let p = Poller::spawn(
            Arc::new(FakeAdb::new(w2.clone())),
            Arc::new(FakeUsb::new(w2)),
            deps.profiles.clone(),
            deps.emitter.clone(),
            Duration::from_secs(3600),
        );
        deps.poller = Some(p.clone());
        slot = Some(p);
    });
    let poller = slot.unwrap();
    let devices = |h: &Harness| {
        h.world
            .calls()
            .iter()
            .filter(|c| **c == Call::Devices)
            .count()
    };
    poller.poll_once();
    let before = devices(&h);
    assert!(before >= 1, "poller probes while idle");

    let id = h
        .engine
        .start_with(
            FlowKind::Backup,
            FlowOptions::default(),
            Box::new(one_prompt_flow),
        )
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    let during = devices(&h);
    poller.poll_once();
    poller.poll_once();
    assert_eq!(devices(&h), during, "no adb probe while a flow is active");
    assert_eq!(during, before, "the flow itself made no devices() call");

    h.engine.answer(&id, &p.id, "retry");
    assert_eq!(wait_terminal(&h, &id).status, FlowStatus::Done);
    poller.poll_once();
    assert!(devices(&h) > during, "poller resumed after the flow ended");
    poller.stop();
}

fn vanish_run(
    returns_after: Option<u32>,
    mismatch: bool,
    answers: &[(&str, &str)],
) -> (FlowState, Vec<Call>) {
    let h = harness(stock_world());
    {
        let mut w = h.world.lock();
        w.faults.vanish_on_verify_read = true;
        w.faults.vanish_returns_after = returns_after;
        if mismatch {
            w.faults.write_corrupts_times = 1;
        }
    }
    let (img, sha) = image_fixture(h.lib_dir.path(), 9);
    let scratch = h.lib_dir.path().to_path_buf();
    let end = run_flow_with(
        &h,
        FlowKind::Unroot,
        FlowOptions::default(),
        write_flow(img, sha, scratch),
        answers,
    );
    (end, h.world.calls())
}

#[test]
fn connection_drop_then_reconnect_and_match_resets_once_without_prompt() {
    let (end, calls) = vanish_run(Some(3), false, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(calls.iter().filter(|c| **c == Call::EdlReset).count(), 1);
}

#[test]
fn connection_drop_then_reconnect_with_mismatch_asks_retry_and_never_resets() {
    let (end, calls) = vanish_run(Some(3), true, &[("retryWrite", "cancel")]);
    assert_eq!(expect_failed(&end).code, ErrorCode::HashMismatch);
    assert!(!calls.contains(&Call::EdlReset));
}

#[test]
fn connection_drop_never_reappearing_asks_connection_lost_and_never_resets() {
    let (end, calls) = vanish_run(None, false, &[("connectionLostAfterWrite", "runAgain")]);
    let e = expect_failed(&end);
    assert_eq!(e.suggested_action, Some(SuggestedAction::GoToRescue));
    assert!(!calls.contains(&Call::EdlReset));
}
