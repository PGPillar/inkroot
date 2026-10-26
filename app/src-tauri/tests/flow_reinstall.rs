mod support;

use inkroot_lib::fakes::{Call, Mode};
use inkroot_lib::ipc::{
    ErrorCode, FlowKind, FlowOptions, FlowState, FlowStatus, SettingsPatch, StepId, SuggestedAction,
};
use inkroot_lib::magisk::{DEVICE_BOOTSVC, DEVICE_SERVICE, HOOK_CHECK_CMD};
use inkroot_lib::tools::adb::RebootTarget;
use support::*;

fn reinstall(h: &Harness, answers: &[(&str, &str)]) -> FlowState {
    run_to_end(
        h,
        FlowKind::ReinstallBootService,
        FlowOptions::default(),
        answers,
    )
}

fn no_edl(h: &Harness) {
    let calls = h.world.calls();
    assert!(
        !calls
            .iter()
            .any(|c| is_reboot_edl_call(c) || is_edl_call(c)),
        "{calls:#?}"
    );
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
fn reinstall_hook_present_pushes_scripts_without_edl() {
    let h = harness(stock_world());
    root_the_world(&h.world, true);
    let end = reinstall(&h, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.kind, FlowKind::ReinstallBootService);
    assert_eq!(end.step_count, 2);
    no_edl(&h);

    let calls = h.world.calls();
    let push_svc = calls
        .iter()
        .position(|c| matches!(c, Call::Push { local_name, remote } if local_name == "bootsvc.sh" && remote == DEVICE_BOOTSVC))
        .expect("bootsvc.sh pushed");
    let push_default = calls
        .iter()
        .position(|c| matches!(c, Call::Push { local_name, remote } if local_name == "inkroot-default-service.sh" && remote == DEVICE_SERVICE))
        .expect("default service pushed");
    let reboot = calls
        .iter()
        .position(|c| {
            matches!(
                c,
                Call::Reboot {
                    target: RebootTarget::System,
                    ..
                }
            )
        })
        .expect("adb reboot");
    assert!(push_svc < reboot && push_default < reboot);
    // The log already showed the hook: no magiskboot check was needed.
    assert!(
        !calls
            .iter()
            .any(|c| matches!(c, Call::Shell { cmd, .. } if cmd == HOOK_CHECK_CMD))
    );
    // A fresh root line was logged by the restart.
    let w = h.world.lock();
    assert_eq!(w.bootsvc_lines.len(), 2);
    assert_eq!(w.mode, Mode::System { authorized: true });
    drop(w);

    let steps: Vec<(StepId, u8)> = h
        .events
        .states()
        .iter()
        .filter(|s| s.flow_id == end.flow_id)
        .map(|s| {
            assert_eq!(s.kind, FlowKind::ReinstallBootService, "{s:?}");
            assert_eq!(s.step_count, 2, "{s:?}");
            (s.step, s.step_index)
        })
        .collect();
    for want in [
        (StepId::CheckDevice, 1),
        (StepId::PushScripts, 2),
        (StepId::WaitBoot, 2),
        (StepId::VerifyRoot, 2),
    ] {
        assert!(steps.contains(&want), "{want:?} missing from {steps:?}");
    }
}

#[test]
fn reinstall_hook_from_patched_backup_image() {
    // The log has no root line, but this tablet's newest backup holds the Inkroot-patched
    // image whose ramdisk lists the hook.
    let h = harness(stock_world());
    let seeded = seed_stock_backup(&h);
    let (dir, mut m) = h.engine.deps().library.get(&seeded.id).unwrap();
    let img = patched_boot(2);
    std::fs::write(dir.join("patched-boot.img"), &img).unwrap();
    let sha = inkroot_lib::image::sha256_file(&dir.join("patched-boot.img")).unwrap();
    m.patched = Some(inkroot_lib::backup::PatchedInfo {
        file: "patched-boot.img".into(),
        sha256: sha.clone(),
        magisk_version: "30.7".into(),
    });
    m.files.push(inkroot_lib::ipc::BackupFile {
        name: "patched-boot.img".into(),
        size: 4096,
        sha256: sha,
    });
    h.engine.deps().library.write_manifest(&dir, &m).unwrap();
    root_the_world(&h.world, false);

    let end = reinstall(&h, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.kind, FlowKind::ReinstallBootService);
    no_edl(&h);
    assert!(
        h.world
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Shell { cmd, .. } if cmd == HOOK_CHECK_CMD))
    );
}

#[test]
fn reinstall_hook_present_verify_failure() {
    // The hook is said to be present, but the tablet comes back without a fresh root line
    // (its boot no longer carries the patch).
    let h = harness(stock_world());
    root_the_world(&h.world, true);
    h.world.lock().partitions.insert(
        "boot_b".into(),
        inkroot_lib::fakes::fake_boot_image(4096, 2),
    );
    let end = reinstall(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::VerifyFailed);
    no_edl(&h);
}

#[test]
fn reinstall_hook_absent_switches_kind_to_root() {
    let h = harness(stock_world());
    seed_stock_backup(&h);
    root_the_world(&h.world, false);
    has_cable(&h);
    let end = reinstall(&h, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    assert_eq!(end.kind, FlowKind::Root);
    assert_eq!(
        end.result.as_ref().unwrap().magisk_version.as_deref(),
        Some("30.7")
    );

    let states: Vec<FlowState> = h
        .events
        .states()
        .into_iter()
        .filter(|s| s.flow_id == end.flow_id)
        .collect();
    assert_eq!(states[0].kind, FlowKind::ReinstallBootService);
    let first_root = states
        .iter()
        .position(|s| s.kind == FlowKind::Root)
        .expect("a root snapshot under the same flowId");
    // Once switched, every later snapshot is root, numbered like root (4 steps).
    for s in &states[first_root..] {
        assert_eq!(s.kind, FlowKind::Root, "{s:?}");
    }
    assert!(
        states[first_root + 1..].iter().all(|s| s.step_count == 4),
        "{states:#?}"
    );
    assert!(
        states
            .iter()
            .any(|s| s.kind == FlowKind::Root && s.step_index == 2)
    );
    // Root wrote the patched image over EDL.
    assert!(
        h.world
            .calls()
            .iter()
            .any(|c| matches!(c, Call::EdlWrite { part, .. } if part == "boot_b"))
    );
}

#[test]
fn reinstall_hook_absent_requires_risk_notice() {
    let h = harness_without_risk(stock_world());
    root_the_world(&h.world, false);
    let end = reinstall(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::Internal);
    assert_eq!(e.message, "Please read and accept the risk notice first.");
    assert_eq!(end.kind, FlowKind::ReinstallBootService);
    no_edl(&h);
}

#[test]
fn reinstall_hook_absent_no_backup_suggests_import() {
    let h = harness(stock_world());
    root_the_world(&h.world, false);
    let end = reinstall(&h, &[]);
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::NoStockBackup);
    assert_eq!(e.suggested_action, Some(SuggestedAction::ImportBackup));
    assert_eq!(end.kind, FlowKind::Root);
    no_edl(&h);
}

#[test]
fn reinstall_not_rooted_fails() {
    let h = harness(stock_world());
    let end = reinstall(&h, &[]);
    assert_eq!(expect_failed(&end).code, ErrorCode::NotRooted);
    no_edl(&h);
}

#[cfg(unix)]
#[test]
fn reinstall_push_failure_is_plain_and_suggests_reconnect() {
    use std::os::unix::fs::PermissionsExt;
    let h = harness(stock_world());
    root_the_world(&h.world, true);
    // The script exists (the start gate passes) but can't be read, so `adb push` fails.
    let svc = h.engine.deps().tools.boot_service("bootsvc.sh");
    std::fs::set_permissions(&svc, std::fs::Permissions::from_mode(0o000)).unwrap();
    let end = reinstall(&h, &[]);
    std::fs::set_permissions(&svc, std::fs::Permissions::from_mode(0o644)).unwrap();
    let e = expect_failed(&end);
    assert_eq!(e.code, ErrorCode::Internal);
    assert_eq!(
        e.message,
        "Inkroot couldn't copy files to your tablet. Check the cable and try again."
    );
    assert_eq!(e.suggested_action, Some(SuggestedAction::Reconnect));
    assert!(e.detail.contains("bootsvc.sh"), "{e:?}");
    no_edl(&h);
    assert!(
        !h.world
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Reboot { .. }))
    );
}
