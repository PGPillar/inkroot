//! Restarting out of EDL (`edl reset`): retries, the stillInEdl prompt, and never a silent
//! multi-minute wait while the tablet sits in rescue mode.

mod support;

use inkroot_lib::clock::Clock;
use inkroot_lib::fakes::{Call, Mode};
use inkroot_lib::flows::common::{RESET_ATTEMPTS, RESET_SETTLE};
use inkroot_lib::ipc::{ErrorCode, FlowKind, FlowOptions, FlowStatus, Prompt, StepId};
use support::*;

fn resets(h: &Harness) -> usize {
    h.world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::EdlReset))
        .count()
}

fn devices_polls(h: &Harness) -> usize {
    h.world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::Devices))
        .count()
}

/// The user held the power button: the tablet drops out of EDL and starts up.
fn hold_power(h: &Harness) {
    let mut w = h.world.lock();
    w.faults.reset_errors = 0;
    w.faults.reset_ignored = 0;
    w.mode = Mode::Off;
    w.pending_boot = true;
}

fn log_text(h: &Harness, flow_id: &str) -> Vec<String> {
    h.events
        .logs()
        .into_iter()
        .filter(|l| l.flow_id.as_deref() == Some(flow_id))
        .map(|l| l.line)
        .collect()
}

/// Answers `p` and waits until the flow has moved past it.
fn answer(h: &Harness, flow_id: &str, p: &Prompt, option: &str) {
    h.engine.answer(flow_id, &p.id, option);
    wait_state(h, flow_id, |s| {
        s.prompt.as_ref().map(|q| q.id.as_str()) != Some(p.id.as_str())
    });
}

fn saw_step(h: &Harness, flow_id: &str, step: StepId) -> bool {
    h.events
        .states()
        .iter()
        .any(|s| s.flow_id == flow_id && s.step == step)
}

#[test]
fn reset_failing_once_is_retried_and_succeeds() {
    let h = harness(stock_world());
    h.world.lock().faults.reset_errors = 1;
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .unwrap();
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(resets(&h), 2);
    let logs = log_text(&h, &id);
    assert!(
        logs.iter()
            .any(|l| l.starts_with("The restart command reported a problem")),
        "{logs:#?}"
    );
    assert!(!logs.iter().any(|l| l.contains("stillInEdl")), "{logs:#?}");
}

#[test]
fn reset_that_leaves_the_tablet_in_edl_is_retried() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    root_the_world(&h.world, true);
    // `edl reset` exits 0, but the 9008 device is still there 10 s later.
    h.world.lock().faults.reset_ignored = 1;
    let before = h.clock.now();
    let end = run_to_end(&h, FlowKind::Unroot, FlowOptions::default(), &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(resets(&h), 2);
    assert!(!h.world.lock().rooted);
    // One settle window was spent waiting for the tablet to leave EDL, not minutes.
    assert!(h.clock.now() - before >= RESET_SETTLE);
}

#[test]
fn reset_failing_persistently_prompts_instead_of_waiting() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    h.world.lock().faults.reset_errors = u32::MAX;
    let start = h.clock.now();
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "cableCheck");
    answer(&h, &id, &p, "have");

    let p = wait_prompt(&h, &id, "stillInEdl");
    let ids: Vec<&str> = p.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["retry", "heldPower"]);
    assert_eq!(resets(&h), RESET_ATTEMPTS as usize);
    // Never went on to wait for adb, and the fake clock shows no multi-minute wait.
    assert!(!saw_step(&h, &id, StepId::WaitBoot));
    assert!(h.clock.now() - start < std::time::Duration::from_secs(120));
    let state = latest_state(&h, &id).unwrap();
    assert_eq!(state.step, StepId::Reset);
    assert!(!state.writing);

    // "Try restarting again": another round of attempts, then the same prompt.
    let polls = devices_polls(&h);
    answer(&h, &id, &p, "retry");
    let p2 = wait_prompt(&h, &id, "stillInEdl");
    assert_eq!(resets(&h), 2 * RESET_ATTEMPTS as usize);
    assert_eq!(devices_polls(&h), polls, "no adb polling while in EDL");

    // The user holds power: the tablet restarts and the flow carries on.
    hold_power(&h);
    answer(&h, &id, &p2, "heldPower");
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert!(h.world.lock().rooted);
    assert_eq!(resets(&h), 2 * RESET_ATTEMPTS as usize);
}

#[test]
fn held_power_while_still_in_edl_asks_again() {
    let h = harness(stock_world());
    h.world.lock().faults.reset_errors = u32::MAX;
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "stillInEdl");
    assert_eq!(resets(&h), RESET_ATTEMPTS as usize);
    // The backup is already saved and verified before the restart.
    assert_eq!(h.engine.deps().library.list(None).len(), 1);
    answer(&h, &id, &p, "heldPower");
    let p2 = wait_prompt(&h, &id, "stillInEdl");
    // "I've restarted it" doesn't send another reset.
    assert_eq!(resets(&h), RESET_ATTEMPTS as usize);
    hold_power(&h);
    answer(&h, &id, &p2, "heldPower");
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
}

#[test]
fn rescue_reset_failing_persistently_prompts() {
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    {
        let mut w = h.world.lock();
        w.partitions.insert("boot_b".into(), vec![0u8; 4096]);
        w.mode = Mode::Edl;
        w.faults.reset_errors = u32::MAX;
    }
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
    let p = wait_prompt(&h, &id, "stillInEdl");
    assert_eq!(resets(&h), RESET_ATTEMPTS as usize);
    hold_power(&h);
    answer(&h, &id, &p, "heldPower");
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
}

#[test]
fn cancel_at_still_in_edl_stops_without_more_resets() {
    let h = harness(stock_world());
    h.world.lock().faults.reset_errors = u32::MAX;
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .unwrap();
    wait_prompt(&h, &id, "stillInEdl");
    h.engine.cancel(&id).unwrap();
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    assert_eq!(resets(&h), RESET_ATTEMPTS as usize);
}

#[test]
fn mismatched_write_never_resets_even_with_reset_faults() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    {
        let mut w = h.world.lock();
        w.faults.write_corrupts_times = 1;
        w.faults.reset_errors = 1;
    }
    let end = run_to_end(
        &h,
        FlowKind::Root,
        FlowOptions::default(),
        &[("cableCheck", "have"), ("retryWrite", "cancel")],
    );
    assert_eq!(expect_failed(&end).code, ErrorCode::HashMismatch);
    assert_eq!(resets(&h), 0);
    assert_eq!(h.world.lock().mode, Mode::Edl);
}

#[test]
fn waiting_for_boot_logs_state_changes_only() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    h.world.lock().faults.boot_hangs = true;
    let id = h
        .engine
        .start(FlowKind::Root, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "cableCheck");
    answer(&h, &id, &p, "have");
    wait_prompt(&h, &id, "keepWaitingOrRescue");
    // Dozens of polls over the boot wait…
    assert!(devices_polls(&h) > 50, "{}", devices_polls(&h));
    // …but the log only has the lines around them.
    let logs = log_text(&h, &id);
    let from = logs
        .iter()
        .position(|l| l == "Waiting for the tablet to restart")
        .expect("wait_boot started");
    let during = &logs[from + 1..];
    assert!(during.len() <= 3, "{during:#?}");
    assert!(!logs.iter().any(|l| l.contains("List of devices")));
    // The tablet comes back unauthorized (for a whole wait), then authorized: one line each.
    h.world.lock().mode = Mode::System { authorized: false };
    let p = latest_state(&h, &id).unwrap().prompt.unwrap();
    answer(&h, &id, &p, "keepWaiting");
    let p2 = wait_prompt(&h, &id, "keepWaitingOrRescue");
    {
        let mut w = h.world.lock();
        w.faults.boot_hangs = false;
        w.mode = Mode::Off;
        w.pending_boot = true;
    }
    answer(&h, &id, &p2, "keepWaiting");
    let end = drive(&h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let logs = log_text(&h, &id);
    let n = |s: &str| logs.iter().filter(|l| *l == s).count();
    assert_eq!(n("Tablet seen: unauthorized"), 1, "{logs:#?}");
    assert_eq!(n("Tablet seen: device"), 1, "{logs:#?}");
}
