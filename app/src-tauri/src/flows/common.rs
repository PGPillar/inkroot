//! Steps shared by several flows. Each is a blocking function over a `FlowCtx`.
//! None of them changes the visible step number; flows call `ctx.step` around them.

use super::{FlowCtx, FlowEnd};
use crate::device::probe;
use crate::image::sha256_file;
use crate::ipc::{
    DeviceInfo, DeviceStatus, ErrorCode, FlowError, Gate, PromptKind, SettingsPatch, StepId,
    SuggestedAction,
};
use crate::profile::{DeviceProfile, MIN_BATTERY};
use crate::tools::adb::{AdbState, RebootTarget};
use crate::tools::process::CancelToken;
use crate::tools::usb::EdlUsbInfo;
use std::path::Path;
use std::time::Duration;

pub const HASH_MISMATCH_MESSAGE: &str = "The copy on your tablet doesn't match yet, so Inkroot didn't restart it. Re-seat the cable and try again, or use Rescue.";
pub const BOOT_TIMEOUT_MESSAGE: &str = "Your tablet hasn't restarted yet.";
pub const EDL_MODE_MESSAGE: &str = "Your tablet is in rescue mode. Use Rescue to bring it back.";

const EDL_POLL: Duration = Duration::from_secs(1);
const BOOT_POLL: Duration = Duration::from_secs(2);

fn fail(
    code: ErrorCode,
    message: &str,
    detail: impl Into<String>,
    action: Option<SuggestedAction>,
) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code,
        message: message.into(),
        detail: detail.into(),
        suggested_action: action,
    })
}

/// Probes the tablet and applies the gates. `require_rooted`: `Some(false)` refuses a rooted
/// tablet, `Some(true)` refuses an unrooted one.
pub fn check_device(
    ctx: &FlowCtx,
    require_rooted: Option<bool>,
) -> Result<(String, DeviceInfo, DeviceProfile), FlowEnd> {
    let d = ctx.deps();
    ctx.info("Looking for your tablet");
    let (status, serial) = probe(d.adb.as_ref(), d.usb.as_ref(), &d.profiles, &d.timeouts);
    let (info, gate) = match status {
        DeviceStatus::None => {
            return Err(fail(
                ErrorCode::NoDevice,
                "Inkroot can't see your tablet. Check that it's plugged in, turned on and has USB debugging on.",
                "probe: no adb device and no EDL device",
                Some(SuggestedAction::Reconnect),
            ));
        }
        DeviceStatus::Unauthorized => {
            return Err(fail(
                ErrorCode::Unauthorized,
                "Your tablet is asking for permission. Tap Allow on its screen, then try again.",
                "adb device state: unauthorized",
                Some(SuggestedAction::Retry),
            ));
        }
        DeviceStatus::Edl => {
            return Err(fail(
                ErrorCode::NoDevice,
                EDL_MODE_MESSAGE,
                "probe: Qualcomm 9008 (EDL) device present",
                Some(SuggestedAction::GoToRescue),
            ));
        }
        DeviceStatus::Adb { info, gate } => (info, gate),
    };
    ctx.info(&format!(
        "Tablet properties: serial={} model={} build={} display={} incremental={} slot={:?} battery={}%",
        info.serial,
        info.model,
        info.build_id,
        info.display_id,
        info.incremental,
        info.slot,
        info.battery
    ));
    if let Some(g) = gate {
        let detail = format!(
            "gate {g:?}: model={} build={} display={} incremental={} slot={:?} battery={}",
            info.model, info.build_id, info.display_id, info.incremental, info.slot, info.battery
        );
        return Err(match g {
            Gate::UnknownModel => fail(
                ErrorCode::UnknownModel,
                "Inkroot only works with the BOOX Note Max, and this tablet doesn't look like one.",
                detail,
                None,
            ),
            Gate::UnknownFirmware => fail(
                ErrorCode::UnknownFirmware,
                "Your tablet's software version isn't one Inkroot knows, so it won't make any changes.",
                detail,
                None,
            ),
            Gate::LowBattery => fail(
                ErrorCode::LowBattery,
                &format!(
                    "Your tablet's battery is low. Charge it to at least {MIN_BATTERY}%, then try again."
                ),
                detail,
                Some(SuggestedAction::ChargeDevice),
            ),
        });
    }
    let serial = serial.unwrap_or_else(|| info.serial.clone());
    let Some(profile) = d
        .profiles
        .iter()
        .find(|p| info.profile_id.as_deref() == Some(p.id.as_str()))
        .cloned()
    else {
        return Err(fail(
            ErrorCode::UnknownModel,
            "Inkroot only works with the BOOX Note Max, and this tablet doesn't look like one.",
            format!("no profile for model {}", info.model),
            None,
        ));
    };
    match require_rooted {
        Some(false) if info.rooted => {
            return Err(fail(
                ErrorCode::AlreadyRooted,
                "Your tablet is already rooted, so there's nothing to do here.",
                format!("magisk {:?} active", info.magisk_version),
                None,
            ));
        }
        Some(true) if !info.rooted => {
            return Err(fail(
                ErrorCode::NotRooted,
                "Your tablet isn't rooted right now.",
                "magisk -c failed",
                None,
            ));
        }
        _ => {}
    }
    ctx.info(&format!(
        "Found {} ({}), firmware {}, slot {:?}, battery {}%, rooted: {}",
        info.display_name, serial, info.firmware_label, info.slot, info.battery, info.rooted
    ));
    Ok((serial, info, profile))
}

/// Polls USB for the EDL device every second, up to `wait_edl`.
fn poll_edl(ctx: &FlowCtx) -> Result<Option<EdlUsbInfo>, FlowEnd> {
    let d = ctx.deps();
    let start = d.clock.now();
    loop {
        ctx.check_cancel()?;
        if let Some(u) = d.usb.edl_device() {
            return Ok(Some(u));
        }
        if d.clock.now().saturating_sub(start) >= d.timeouts.wait_edl {
            return Ok(None);
        }
        d.clock.sleep(EDL_POLL);
    }
}

/// Gets the tablet into EDL mode (rebooting it via adb when `serial` is given and it isn't
/// already there) and makes sure the computer may talk to it.
pub fn enter_edl(ctx: &FlowCtx, serial: Option<&str>) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.check_cancel()?;
    if serial.is_some() {
        // From here a failure or cancel before any write restarts the tablet (flows/mod.rs).
        ctx.note_edl_entered();
    }
    if d.usb.edl_device().is_some() {
        ctx.info("The tablet is already in rescue (EDL) mode");
    } else if let Some(s) = serial {
        ctx.info("Restarting the tablet into rescue (EDL) mode");
        d.adb
            .reboot(s, RebootTarget::Edl)
            .map_err(|e| FlowEnd::Failed(e.into()))?;
    }
    let usb = loop {
        if let Some(u) = poll_edl(ctx)? {
            break u;
        }
        ctx.info("The tablet didn't show up in rescue mode in time");
        match ctx.ask(PromptKind::EdlNotFound)?.as_str() {
            // Poll again; never re-issue `adb reboot edl` (adb is gone by now).
            "retry" => continue,
            _ => return Err(FlowEnd::Cancelled),
        }
    };
    ctx.info("Found the tablet in rescue (EDL) mode");
    if d.host.needs_edl_driver(&usb) {
        match ctx.ask(PromptKind::InstallDriver)?.as_str() {
            "install" => {
                ctx.info("Installing the USB driver");
                d.host.install_edl_driver()?;
            }
            _ => return Err(FlowEnd::Cancelled),
        }
    }
    // `needs_udev` is only ever true on Linux (the macOS/Windows host setups return false).
    if d.host.needs_udev() {
        match ctx.ask(PromptKind::InstallUdev)?.as_str() {
            "install" => {
                ctx.info("Allowing access to the tablet in rescue mode");
                d.host.install_udev()?;
            }
            _ => return Err(FlowEnd::Cancelled),
        }
    }
    Ok(())
}

/// Holds keep-awake for the duration of `f`.
pub fn with_keep_awake<T>(ctx: &FlowCtx, f: impl FnOnce() -> T) -> T {
    let _guard = ctx.deps().awake.hold("Inkroot is writing to your tablet");
    f()
}

/// Writes `img` to `part` and reads it back until the copy matches `expected_sha`.
/// Re-hashes `img` before every write. Never resets the tablet after a failed or mismatched
/// write; resets once, only after a matching read-back.
pub fn write_and_verify(
    ctx: &FlowCtx,
    part: &str,
    img: &Path,
    expected_sha: &str,
    scratch_dir: &Path,
) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    let verify = scratch_dir.join(format!(".verify-{part}.img"));
    // After an attempt the tablet may hold a bad copy: any stop before the next write points
    // at Rescue instead of claiming the tablet wasn't touched.
    let stop = |detail: String, untouched: FlowEnd| -> FlowEnd {
        if ctx.write_attempted() {
            fail(
                ErrorCode::HashMismatch,
                HASH_MISMATCH_MESSAGE,
                detail,
                Some(SuggestedAction::GoToRescue),
            )
        } else {
            untouched
        }
    };
    with_keep_awake(ctx, || {
        loop {
            ctx.sub_step(StepId::Write);
            // 1. Re-hash the exact file immediately before writing it.
            let sha = sha256_file(img).map_err(|e| {
                stop(
                    format!("hash {}: {e}", img.display()),
                    fail(
                        ErrorCode::BadImage,
                        "Inkroot couldn't read the file it was about to use, so it stopped before touching your tablet.",
                        format!("hash {}: {e}", img.display()),
                        None,
                    ),
                )
            })?;
            if !sha.eq_ignore_ascii_case(expected_sha) {
                let detail = format!("{}: sha256 {sha}, expected {expected_sha}", img.display());
                return Err(stop(
                    detail.clone(),
                    fail(
                        ErrorCode::BadImage,
                        "The file Inkroot was about to use has changed since it was checked, so it stopped before touching your tablet.",
                        detail,
                        None,
                    ),
                ));
            }
            // 2. Never start `edl w` without the tablet in EDL.
            loop {
                if ctx.check_cancel().is_err() {
                    return Err(stop(
                        format!("cancelled before writing {part}"),
                        FlowEnd::Cancelled,
                    ));
                }
                if d.usb.edl_device().is_some() {
                    break;
                }
                ctx.info("The tablet isn't showing in rescue (EDL) mode right now");
                let again = matches!(ctx.ask(PromptKind::EdlNotFound), Ok(o) if o == "retry");
                if !again {
                    return Err(stop(
                        format!("tablet not in EDL before writing {part}"),
                        FlowEnd::Cancelled,
                    ));
                }
                match poll_edl(ctx) {
                    Ok(_) => continue,
                    Err(_) => {
                        return Err(stop(
                            format!("cancelled before writing {part}"),
                            FlowEnd::Cancelled,
                        ));
                    }
                }
            }
            // 3. Write. Once `writing` is set, cancel is rejected; a cancel that landed just
            // before is caught here.
            ctx.set_writing(true);
            if ctx.check_cancel().is_err() {
                ctx.set_writing(false);
                return Err(stop(
                    format!("cancelled before writing {part}"),
                    FlowEnd::Cancelled,
                ));
            }
            ctx.note_write_attempted();
            ctx.info(&format!("Writing {part}"));
            let outcome = (|| -> Result<bool, String> {
                d.edl
                    .write(part, img, &|done, total| {
                        ctx.progress(done, total, "Writing")
                    })
                    .map_err(|e| format!("write {part}: {e}"))?;
                // 4. Read back. Cancel is rejected while writing, so a fresh token.
                ctx.sub_step(StepId::VerifyWrite);
                d.edl
                    .read(part, &verify, &CancelToken::default(), &|done, total| {
                        ctx.progress(done, total, "Checking")
                    })
                    .map_err(|e| format!("read back {part}: {e}"))?;
                let got = sha256_file(&verify).map_err(|e| format!("hash read-back: {e}"))?;
                if got.eq_ignore_ascii_case(expected_sha) {
                    Ok(true)
                } else {
                    Err(format!(
                        "read-back of {part}: sha256 {got}, expected {expected_sha}"
                    ))
                }
            })();
            // 5.
            ctx.set_writing(false);
            let _ = std::fs::remove_file(&verify);
            match outcome {
                Ok(_) => {
                    ctx.info(&format!("{part} matches"));
                    ctx.sub_step(StepId::Reset);
                    ctx.note_edl_reset();
                    if let Err(e) = d.edl.reset() {
                        ctx.info(&format!("Restart command reported a problem: {e}"));
                    }
                    return Ok(());
                }
                Err(detail) => {
                    // 6. No reset: the tablet stays in EDL so a retry or Rescue can fix it.
                    ctx.info(&format!("The copy doesn't match yet: {detail}"));
                    match ctx.ask(PromptKind::RetryWrite) {
                        Ok(o) if o == "retry" => continue,
                        // Cancel (or a cancel request) leaves a bad copy: point at Rescue.
                        _ => {
                            return Err(fail(
                                ErrorCode::HashMismatch,
                                HASH_MISMATCH_MESSAGE,
                                detail,
                                Some(SuggestedAction::GoToRescue),
                            ));
                        }
                    }
                }
            }
        }
    })
}

/// Waits for `serial` to come back as an authorized adb device.
pub fn wait_boot(ctx: &FlowCtx, serial: &str) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.info("Waiting for the tablet to restart");
    loop {
        let start = d.clock.now();
        loop {
            ctx.check_cancel()?;
            let up = d.adb.devices().is_ok_and(|v| {
                v.iter()
                    .any(|x| x.serial == serial && x.state == AdbState::Device)
            });
            if up {
                ctx.info("The tablet has restarted");
                return Ok(());
            }
            if d.clock.now().saturating_sub(start) >= d.timeouts.wait_boot {
                break;
            }
            d.clock.sleep(BOOT_POLL);
        }
        match ctx.ask(PromptKind::KeepWaitingOrRescue)?.as_str() {
            "keepWaiting" => continue,
            _ => {
                return Err(fail(
                    ErrorCode::BootTimeout,
                    BOOT_TIMEOUT_MESSAGE,
                    format!(
                        "{serial} not back in adb after {}s",
                        d.timeouts.wait_boot.as_secs()
                    ),
                    Some(SuggestedAction::GoToRescue),
                ));
            }
        }
    }
}

fn persist_cable(ctx: &FlowCtx, has: bool) {
    let patch = SettingsPatch {
        has_rescue_cable: Some(Some(has)),
        ..SettingsPatch::default()
    };
    if let Err(e) = ctx.deps().settings.apply(patch) {
        // Not worth failing the flow over; we'll just ask again next time.
        ctx.info(&format!("Couldn't save the cable answer: {}", e.detail));
    }
}

/// Asks whether the user has a deep-flash (rescue) cable, unless they already said yes.
pub fn cable_check(ctx: &FlowCtx) -> Result<(), FlowEnd> {
    if ctx.deps().settings.get().has_rescue_cable == Some(true) {
        ctx.info("Rescue cable: already confirmed");
        return Ok(());
    }
    loop {
        match ctx.ask(PromptKind::CableCheck)?.as_str() {
            "have" => {
                persist_cable(ctx, true);
                return Ok(());
            }
            "continueWithout" => match ctx.ask(PromptKind::CableCheckConfirm)?.as_str() {
                "confirmContinue" => {
                    persist_cable(ctx, false);
                    return Ok(());
                }
                _ => continue,
            },
            // whereToGet: the UI normally handles it locally; ask again.
            _ => continue,
        }
    }
}

/// Runs the §3.1b stock check on `img` with the connected tablet: extracts `magiskboot` from
/// the bundled (integrity-checked) Magisk APK into a temporary folder that is removed
/// afterwards, then runs `magiskboot cpio test` on the tablet. `Ok(true)` means stock.
pub fn magisk_stock_check(
    ctx: &FlowCtx,
    serial: &str,
    profile: &DeviceProfile,
    img: &Path,
) -> Result<bool, FlowError> {
    let d = ctx.deps();
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
    ctx.info("Checking that the startup file is the original");
    crate::magisk::stock_check(d.adb.as_ref(), serial, &magiskboot, img, &d.timeouts)
}
