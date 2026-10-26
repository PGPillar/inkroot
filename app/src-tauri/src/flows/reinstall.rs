//! Reinstall boot service (spec §3.0 item 6). If the boot-service hook is already baked into
//! the tablet's boot image, re-push the scripts and restart (no EDL, no write to boot).
//! Otherwise continue as the full Root flow (same flowId, kind switched to `root`).

use super::common::wait_boot;
use super::root::{BOOTSVC_READ_MESSAGE, bootsvc_lines_before, root_from_step2};
use super::{FlowCtx, FlowEnd, RISK_MESSAGE, lit};
use crate::device::{BOOTSVC_LOG, bootsvc_boot_lines, bootsvc_ok};
use crate::ipc::{DeviceInfo, ErrorCode, FlowError, FlowKind, FlowResult, StepId, SuggestedAction};
use crate::magisk::{DEVICE_BOOTSVC, DEVICE_SERVICE};
use crate::profile::DeviceProfile;
use crate::settings::risk_accepted;
use crate::tools::adb::RebootTarget;
use std::time::Duration;

const STEPS: u8 = 2;
const LOG_TRIES: u32 = 3;
const LOG_RETRY: Duration = Duration::from_secs(2);
const VERIFY_POLL: Duration = Duration::from_secs(5);

pub const REINSTALL_VERIFY_FAILED_MESSAGE: &str =
    "Your tablet restarted, but the boot service didn't start. Nothing else was changed.";

/// The boot-service log text ("" when it doesn't exist), retried on adb errors.
fn read_bootsvc_log(ctx: &FlowCtx, serial: &str) -> Result<String, FlowEnd> {
    let d = ctx.deps();
    let mut last = String::new();
    for attempt in 1..=LOG_TRIES {
        ctx.check_cancel()?;
        match d.adb.shell(serial, BOOTSVC_LOG, d.timeouts.adb_shell) {
            Ok(o) if o.code == 0 => return Ok(o.stdout),
            Ok(o) if o.stderr.contains("No such file") || o.stdout.contains("No such file") => {
                return Ok(String::new());
            }
            Ok(o) => last = format!("exit {}: {}{}", o.code, o.stdout, o.stderr),
            Err(e) => last = e.to_string(),
        }
        ctx.info(&format!(
            "Couldn't read the start-up log (try {attempt} of {LOG_TRIES}): {last}"
        ));
        if attempt < LOG_TRIES {
            d.clock.sleep(LOG_RETRY);
        }
    }
    Err(FlowEnd::Failed(FlowError {
        code: ErrorCode::Internal,
        message: BOOTSVC_READ_MESSAGE.into(),
        detail: format!("{BOOTSVC_LOG}: {last}"),
        suggested_action: Some(SuggestedAction::Reconnect),
    }))
}

fn log_has_root_line(log: &str) -> bool {
    log.lines()
        .any(|l| l.contains("uid=0") && l.contains("u:r:magisk:s0"))
}

/// Whether the newest backup of this tablet holds an Inkroot-patched image whose ramdisk
/// lists the hook. A patched file that no longer matches its manifest is not trusted (and
/// never pushed): that counts as "can't tell", i.e. absent.
fn patched_backup_has_hook(
    ctx: &FlowCtx,
    serial: &str,
    profile: &DeviceProfile,
) -> Result<bool, FlowEnd> {
    let d = ctx.deps();
    let Some(newest) = d.library.list(Some(serial)).into_iter().next() else {
        return Ok(false);
    };
    let Some((dir, m)) = d.library.get(&newest.id) else {
        return Ok(false);
    };
    let Some(patched) = m.patched.clone() else {
        return Ok(false);
    };
    let path = match d.library.verify_file(&dir, &m, &patched.file) {
        Ok(p) => p,
        Err(e) => {
            ctx.info(&format!(
                "The prepared startup file in {} can't be used to check: {}",
                newest.id, e.detail
            ));
            return Ok(false);
        }
    };
    let sha = crate::image::sha256_file(&path).unwrap_or_default();
    if !sha.eq_ignore_ascii_case(&patched.sha256) {
        ctx.info(&format!(
            "The prepared startup file in {} doesn't match its record (sha256 {sha}, expected {})",
            newest.id, patched.sha256
        ));
        return Ok(false);
    }
    let apk = d
        .tools
        .require(&d.tools.magisk_apk(&profile.magisk.version))?;
    let tmp = tempfile::tempdir().map_err(|e| FlowError {
        code: ErrorCode::Internal,
        message: "Inkroot couldn't make a temporary folder on this computer.".into(),
        detail: format!("tempdir: {e}"),
        suggested_action: Some(SuggestedAction::Retry),
    })?;
    let payload = crate::magisk::extract_payload(&apk, &d.magisk_apk_sha256, tmp.path())?;
    let magiskboot = payload
        .iter()
        .find(|(_, n)| *n == "magiskboot")
        .map(|(p, _)| p.clone())
        .ok_or_else(|| FlowError {
            code: ErrorCode::Internal,
            message: "Inkroot couldn't unpack its bundled Magisk package.".into(),
            detail: "magiskboot missing from extracted payload".into(),
            suggested_action: None,
        })?;
    ctx.info(&format!(
        "Checking the prepared startup file in {} for the boot service",
        newest.id
    ));
    Ok(crate::magisk::hook_present(
        d.adb.as_ref(),
        serial,
        &magiskboot,
        &path,
        &d.timeouts,
    )?)
}

/// The Reinstall boot service flow (2 steps, or Root from step 2 when the hook is absent).
pub fn run_reinstall(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    ctx.step(StepId::CheckDevice, 1, STEPS);
    let (serial, info, profile) = super::common::check_device(ctx, Some(true))?;

    let log = read_bootsvc_log(ctx, &serial)?;
    let present = if log_has_root_line(&log) {
        ctx.info("The boot service has started as root before, so its hook is in place");
        true
    } else {
        patched_backup_has_hook(ctx, &serial, &profile)?
    };
    if present {
        push_and_restart(ctx, &serial)?;
        return Ok(FlowResult::default());
    }
    continue_as_root(ctx, &serial, &info, &profile)
}

fn continue_as_root(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
) -> Result<FlowResult, FlowEnd> {
    let d = ctx.deps();
    ctx.info(
        "This tablet was rooted without Inkroot's boot service built in, so Inkroot will root it again from your original safety copy",
    );
    // Every Root gate applies before the flow becomes Root.
    if !risk_accepted(&d.settings.get()) {
        return Err(FlowEnd::Failed(FlowError {
            code: ErrorCode::Internal,
            message: RISK_MESSAGE.into(),
            detail: format!(
                "acceptedRiskVersion is not current for {}",
                lit(&FlowKind::Root)
            ),
            suggested_action: None,
        }));
    }
    d.tools.require_for(FlowKind::Root, profile)?;
    ctx.set_kind(FlowKind::Root);
    root_from_step2(ctx, serial, info, profile)
}

/// Hook present: re-push the scripts, restart, and wait for a fresh root boot line.
fn push_and_restart(ctx: &FlowCtx, serial: &str) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.step(StepId::PushScripts, 2, STEPS);
    let bootsvc = d.tools.require(&d.tools.boot_service("bootsvc.sh"))?;
    let service = d
        .tools
        .require(&d.tools.boot_service("inkroot-default-service.sh"))?;
    ctx.info("Copying the boot service to your tablet");
    crate::magisk::push_one(d.adb.as_ref(), serial, &bootsvc, DEVICE_BOOTSVC)?;
    crate::magisk::push_one(d.adb.as_ref(), serial, &service, DEVICE_SERVICE)?;
    let before = bootsvc_lines_before(ctx, serial)?;
    ctx.check_cancel()?;
    ctx.info("Restarting the tablet");
    d.adb
        .reboot(serial, RebootTarget::System)
        .map_err(|e| FlowEnd::Failed(e.into()))?;

    ctx.sub_step(StepId::WaitBoot);
    wait_boot(ctx, serial)?;

    ctx.sub_step(StepId::VerifyRoot);
    ctx.info("Checking that the boot service started");
    let start = d.clock.now();
    loop {
        ctx.check_cancel()?;
        let seen = d.adb.shell(serial, BOOTSVC_LOG, d.timeouts.adb_shell);
        let (lines, ok, err) = match &seen {
            Ok(o) if o.code == 0 => (bootsvc_boot_lines(&o.stdout), bootsvc_ok(&o.stdout), None),
            Ok(o) => (
                0,
                false,
                Some(format!("exit {}: {}", o.code, o.stderr.trim())),
            ),
            Err(e) => (0, false, Some(e.to_string())),
        };
        if lines > before && ok {
            ctx.info("The boot service started as root");
            return Ok(());
        }
        if d.clock.now().saturating_sub(start) >= d.timeouts.wait_bootsvc {
            return Err(FlowEnd::Failed(FlowError {
                code: ErrorCode::VerifyFailed,
                message: REINSTALL_VERIFY_FAILED_MESSAGE.into(),
                detail: format!(
                    "after {}s: bootsvc boot lines {lines} (before {before}), last line root+magisk ctx: {ok}{}",
                    d.timeouts.wait_bootsvc.as_secs(),
                    err.map(|e| format!(", last error: {e}"))
                        .unwrap_or_default()
                ),
                suggested_action: None,
            }));
        }
        d.clock.sleep(VERIFY_POLL);
    }
}
