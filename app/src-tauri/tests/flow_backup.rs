mod support;

use inkroot_lib::backup::StockEvidence;
use inkroot_lib::fakes::{Call, Mode, PATCH_MARKER, fake_boot_image};
use inkroot_lib::image::sha256_file;
use inkroot_lib::ipc::{
    BackupSource, ErrorCode, FlowKind, FlowOptions, FlowStatus, Slot, StepId, SuggestedAction,
};
use inkroot_lib::magisk::STOCK_CHECK_CMD;
use inkroot_lib::profile::KnownStockBoot;
#[cfg(unix)]
use inkroot_lib::tools::adb::RebootTarget;
use sha2::{Digest, Sha256};
use support::*;

fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

fn stock_check_calls(h: &Harness) -> usize {
    h.world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::Shell { cmd, .. } if cmd == STOCK_CHECK_CMD))
        .count()
}

/// Every manifest.json anywhere under the library root.
fn manifest_files(h: &Harness) -> Vec<std::path::PathBuf> {
    fn walk(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().is_some_and(|n| n == "manifest.json") {
                out.push(path);
            }
        }
    }
    let mut v = vec![];
    walk(h.lib_dir.path(), &mut v);
    v
}

fn run_backup(h: &Harness) -> inkroot_lib::ipc::FlowState {
    run_to_end(h, FlowKind::Backup, FlowOptions::default(), &[])
}

#[test]
fn backup_happy_path_writes_verified_stock_manifest() {
    let h = harness(stock_world());
    let parts = h.world.lock().partitions.clone();
    let end = run_backup(&h);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.step_count, 4);
    let summary = end.result.unwrap().backup.expect("backup summary");
    assert_eq!(summary.serial, NOTE_MAX_SERIAL);
    assert!(summary.stock && summary.verified);
    assert_eq!(summary.source, BackupSource::Inkroot);
    assert_eq!(summary.slot, Slot::B);
    assert_eq!(summary.firmware, "TKQ1.230721.002");
    let names: Vec<&str> = summary.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["boot_a.img", "boot_b.img", "vbmeta_a.img", "vbmeta_b.img"]
    );
    for f in &summary.files {
        let part = f.name.trim_end_matches(".img");
        assert_eq!(f.sha256, sha(&parts[part]), "{}", f.name);
        assert_eq!(f.size, parts[part].len() as u64);
        let on_disk = std::path::Path::new(&summary.path).join(&f.name);
        assert_eq!(sha256_file(&on_disk).unwrap(), f.sha256);
    }
    let (dir, m) = h.engine.deps().library.get(&summary.id).unwrap();
    assert_eq!(dir.to_string_lossy(), summary.path);
    assert_eq!(m.stock_evidence, StockEvidence::MagiskbootTest);
    assert!(m.stock && m.verified);
    assert_eq!(m.inkroot_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(m.profile_id.as_deref(), Some("note-max"));
    assert_eq!(m.model, "NoteMax");
    assert_eq!(m.firmware.build_id, "TKQ1.230721.002");
    assert_eq!(m.firmware.incremental, "226");
    assert!(m.firmware.fingerprint.starts_with("ONYX/TabBoox"));
    assert!(chrono::DateTime::parse_from_rfc3339(&m.created_at).is_ok());
    assert!(m.imported_from.is_none() && m.patched.is_none());
    // No scratch files left behind.
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "boot_a.img",
            "boot_b.img",
            "manifest.json",
            "vbmeta_a.img",
            "vbmeta_b.img"
        ]
    );
    assert_eq!(stock_check_calls(&h), 1);
    // Device back in System.
    assert_eq!(h.world.lock().mode, Mode::System { authorized: true });
    // Steps in order.
    let mut seen: Vec<(StepId, u8)> = vec![];
    for s in h.events.states() {
        if seen.last() != Some(&(s.step, s.step_index)) {
            seen.push((s.step, s.step_index));
        }
    }
    let idx = |st: StepId| seen.iter().position(|x| x.0 == st).unwrap();
    assert!(idx(StepId::CheckDevice) < idx(StepId::EnterEdl));
    assert!(idx(StepId::EnterEdl) < idx(StepId::ReadPartitions));
    assert!(idx(StepId::ReadPartitions) < idx(StepId::VerifyBackup));
    assert!(seen.contains(&(StepId::EnterEdl, 2)));
    assert!(seen.contains(&(StepId::ReadPartitions, 3)));
    assert!(seen.contains(&(StepId::VerifyBackup, 4)));
    assert!(!h.events.progress().is_empty());
}

#[test]
fn backup_known_hash_skips_magiskboot() {
    let world = stock_world();
    let boot_b = sha(&world.lock().partitions["boot_b"]);
    let h = harness_opts(world, true, |d| {
        d.profiles[0].known_stock_boot_sha256 = vec![KnownStockBoot {
            sha256: boot_b.to_uppercase(),
            slot: Slot::B,
            display_id: "TKQ1.230721.002".into(),
        }];
    });
    let end = run_backup(&h);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert!(m.stock && m.verified);
    assert_eq!(m.stock_evidence, StockEvidence::KnownHash);
    assert_eq!(stock_check_calls(&h), 0);
}

#[test]
fn backup_known_hash_for_other_slot_or_firmware_falls_through_to_magiskboot() {
    let world = stock_world();
    let boot_b = sha(&world.lock().partitions["boot_b"]);
    for (slot, display_id) in [(Slot::A, "TKQ1.230721.002"), (Slot::B, "TKQ1.999999.001")] {
        let world = stock_world();
        let boot_b = boot_b.clone();
        let h = harness_opts(world, true, move |d| {
            d.profiles[0].known_stock_boot_sha256 = vec![KnownStockBoot {
                sha256: boot_b,
                slot,
                display_id: display_id.into(),
            }];
        });
        let end = run_backup(&h);
        assert_eq!(end.status, FlowStatus::Done, "{end:?}");
        let s = end.result.unwrap().backup.unwrap();
        let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
        assert!(m.stock && m.verified);
        assert_eq!(m.stock_evidence, StockEvidence::MagiskbootTest, "{slot:?}");
        assert_eq!(stock_check_calls(&h), 1);
    }
}

#[test]
fn backup_on_rooted_device_marks_not_stock() {
    let world = stock_world();
    {
        let mut w = world.lock();
        let mut img = fake_boot_image(4096, 2);
        let at = img.len() - PATCH_MARKER.len();
        img[at..].copy_from_slice(PATCH_MARKER);
        w.partitions.insert("boot_b".into(), img);
        w.rooted = true;
        w.magisk_version = Some("30.7".into());
    }
    let h = harness(world);
    let end = run_backup(&h);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert!(!s.stock);
    assert!(s.verified);
    let (_, m) = h.engine.deps().library.get(&s.id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::NotStockRooted);
    assert_eq!(stock_check_calls(&h), 0);
    assert!(
        h.engine
            .deps()
            .library
            .stock_for(
                NOTE_MAX_SERIAL,
                "TKQ1.230721.002",
                Slot::B,
                &h.engine.deps().profiles[0]
            )
            .is_none()
    );
}

#[test]
fn backup_read_failure_resets_and_fails_cleanly() {
    let h = harness(stock_world());
    h.world.lock().faults.read_fails_on = Some("boot_b".into());
    let end = run_backup(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::EdlTimeout);
    assert_eq!(e.suggested_action, Some(SuggestedAction::Reconnect));
    assert!(!e.message.contains("edl"), "{}", e.message);
    assert_eq!(end.step, StepId::ReadPartitions);
    let calls = h.world.calls();
    let failed_read = calls
        .iter()
        .position(|c| matches!(c, Call::EdlRead { part } if part == "boot_b"))
        .expect("boot_b read attempted");
    assert!(
        calls[failed_read..].contains(&Call::EdlReset),
        "best-effort reset after the failed read: {calls:?}"
    );
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })));
    assert!(
        h.engine
            .deps()
            .library
            .list(None)
            .iter()
            .all(|s| !s.verified)
    );
    assert!(manifest_files(&h).is_empty(), "no manifest written");
    assert!(
        h.engine
            .deps()
            .library
            .stock_for(
                NOTE_MAX_SERIAL,
                "TKQ1.230721.002",
                Slot::B,
                &h.engine.deps().profiles[0]
            )
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn backup_unwritable_root_fails_before_edl() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return; // root ignores permissions
    }
    let h = harness(stock_world());
    let root = h.lib_dir.path().to_path_buf();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500)).unwrap();
    let end = run_backup(&h);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let e = expect_failed(&end);
    assert_eq!(end.step, StepId::CheckDevice);
    assert_eq!(end.step_index, 1);
    assert!(e.message.contains("Documents"), "{}", e.message);
    let calls = h.world.calls();
    assert!(
        !calls.iter().any(|c| matches!(
            c,
            Call::Reboot {
                target: RebootTarget::Edl,
                ..
            }
        )),
        "{calls:?}"
    );
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlRead { .. })));
    assert_eq!(h.world.lock().mode, Mode::System { authorized: true });
}

#[test]
fn backup_verify_mismatch_fails_not_verified() {
    let h = harness(stock_world());
    h.world.lock().faults.verify_read_differs = true;
    let end = run_backup(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::VerifyFailed);
    assert_eq!(e.suggested_action, Some(SuggestedAction::Retry));
    assert_eq!(end.step, StepId::VerifyBackup);
    let calls = h.world.calls();
    let reads = calls
        .iter()
        .filter(|c| matches!(c, Call::EdlRead { part } if part == "boot_b"))
        .count();
    assert_eq!(reads, 2, "boot_b read twice: {calls:?}");
    let last_read = calls
        .iter()
        .rposition(|c| matches!(c, Call::EdlRead { .. }))
        .unwrap();
    assert!(calls[last_read..].contains(&Call::EdlReset));
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })));
    assert!(
        h.engine
            .deps()
            .library
            .list(None)
            .iter()
            .all(|s| !s.verified)
    );
    assert!(manifest_files(&h).is_empty());
}

/// After a stock-check error the flow fails, but the verified copy is kept as non-stock.
fn assert_kept_verified_non_stock(h: &Harness) {
    let rows = h.engine.deps().library.list(None);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].verified && !rows[0].stock);
    assert_eq!(rows[0].files.len(), 4);
    let (_, m) = h.engine.deps().library.get(&rows[0].id).unwrap();
    assert_eq!(m.stock_evidence, StockEvidence::Unchecked);
    assert!(
        h.engine
            .deps()
            .library
            .stock_for(
                NOTE_MAX_SERIAL,
                "TKQ1.230721.002",
                Slot::B,
                &h.engine.deps().profiles[0]
            )
            .is_none()
    );
    // The tablet was restarted before the check.
    assert_eq!(h.world.lock().mode, Mode::System { authorized: true });
    assert_eq!(stock_check_calls(h), 0);
    assert!(
        !h.events
            .logs()
            .iter()
            .any(|l| l.line.contains("not marked as verified")),
        "a verified copy must not be logged as unverified"
    );
}

#[test]
fn backup_stock_check_tool_missing_fails_but_keeps_verified_copy() {
    let h = harness(stock_world());
    let d = h.engine.deps();
    let apk = d.tools.magisk_apk(&d.profiles[0].magisk.version);
    std::fs::remove_file(&apk).unwrap();
    let end = run_backup(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::ToolMissing, "{e:?}");
    assert_kept_verified_non_stock(&h);
}

#[test]
fn backup_stock_check_apk_integrity_failure_fails_internal() {
    let h = harness_opts(stock_world(), true, |d| {
        d.magisk_apk_sha256 = "0".repeat(64);
    });
    let end = run_backup(&h);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::Internal, "{e:?}");
    assert!(
        e.message.contains("failed its integrity check"),
        "{}",
        e.message
    );
    assert_kept_verified_non_stock(&h);
}

#[test]
fn backup_driver_cancel_resets_tablet() {
    let h = harness(stock_world());
    h.host.set_needs_driver(true);
    let end = run_to_end(
        &h,
        FlowKind::Backup,
        FlowOptions::default(),
        &[("installDriver", "cancel")],
    );
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlRead { .. })));
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlWrite { .. })));
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::EdlReset)).count(),
        1
    );
    assert_ne!(h.world.lock().mode, Mode::Edl);
}

#[test]
fn backup_edl_not_found_cancel_resets_tablet_that_arrived_late() {
    let h = harness(stock_world());
    h.world.lock().faults.edl_never_appears = true;
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .unwrap();
    let p = wait_prompt(&h, &id, "edlNotFound");
    h.world.lock().faults.edl_never_appears = false;
    h.engine.answer(&id, &p.id, "cancel");
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let calls = h.world.calls();
    assert!(!calls.iter().any(|c| matches!(c, Call::EdlRead { .. })));
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::EdlReset)).count(),
        1
    );
}

#[test]
fn backup_read_failure_resets_only_once() {
    let h = harness(stock_world());
    h.world.lock().faults.read_fails_on = Some("boot_a".into());
    let end = run_to_end(&h, FlowKind::Backup, FlowOptions::default(), &[]);
    assert_eq!(end.status, FlowStatus::Failed, "{end:?}");
    assert_eq!(
        h.world
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::EdlReset))
            .count(),
        1
    );
}

#[test]
fn backup_read_failure_reset_is_quick_and_a_hanging_one_is_not_retried() {
    let h = harness(stock_world());
    {
        let mut w = h.world.lock();
        w.faults.read_fails_on = Some("boot_a".into());
        w.faults.reset_hangs = true;
    }
    let end = run_to_end(&h, FlowKind::Backup, FlowOptions::default(), &[]);
    assert_eq!(end.status, FlowStatus::Failed, "{end:?}");
    let resets = h
        .world
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::EdlReset))
        .count();
    // One short-timeout attempt; a reset that hangs isn't run again (and again).
    assert_eq!(resets, 1);
    assert_eq!(h.world.lock().quick_resets, 1);
    assert!(
        h.events
            .logs()
            .iter()
            .any(|l| l.line.contains("Hold its power button")),
        "{:?}",
        h.events.logs()
    );
}

#[test]
fn backup_cancel_mid_read_ends_and_logs_cancel_once() {
    let h = harness(stock_world());
    h.world.lock().delays.read = std::time::Duration::from_millis(400);
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .unwrap();
    wait_state(&h, &id, |s| s.step == StepId::ReadPartitions);
    for _ in 0..5 {
        h.engine.cancel(&id).unwrap();
    }
    let end = wait_terminal(&h, &id);
    assert_eq!(end.status, FlowStatus::Cancelled, "{end:?}");
    let n = h
        .events
        .logs()
        .iter()
        .filter(|l| l.line == "Cancel requested")
        .count();
    assert_eq!(n, 1, "{:?}", h.events.logs());
    // The best-effort restart after the cancel is the short one.
    assert!(h.world.lock().quick_resets >= 1);
    assert!(
        h.engine
            .current()
            .is_some_and(|s| s.status == FlowStatus::Cancelled)
    );
}
