//! Steps shared by several flows. Each is a blocking function over a `FlowCtx`.
//! None of them changes the visible step number; flows call `ctx.step` around them.

use super::{FlowCtx, FlowEnd};
use crate::device::probe;
use crate::image::sha256_file;
use crate::ipc::{
    DeviceInfo, DeviceStatus, ErrorCode, FlowError, Gate, Prompt, PromptKind, SettingsPatch,
    StepId, SuggestedAction,
};
use crate::profile::{DeviceProfile, MIN_BATTERY};
use crate::tools::adb::{AdbState, RebootTarget};
use crate::tools::process::CancelToken;
use crate::tools::usb::EdlUsbInfo;
use crate::tools::{ToolError, not_answering, usb_permission};
use std::path::Path;
use std::time::Duration;

pub const HASH_MISMATCH_MESSAGE: &str = "The copy on your tablet doesn't match yet, so Inkroot didn't restart it. Re-seat the cable and try again, or use Rescue.";
pub const BOOT_TIMEOUT_MESSAGE: &str = "Your tablet hasn't restarted yet.";
pub const EDL_MODE_MESSAGE: &str = "Your tablet is in rescue mode. Use Rescue to bring it back.";

const EDL_POLL: Duration = Duration::from_secs(1);
const BOOT_POLL: Duration = Duration::from_secs(2);
/// How long the 9008 device may linger after `edl reset` before Inkroot tries again.
pub const RESET_SETTLE: Duration = Duration::from_secs(10);
/// `edl reset` attempts per round (the first plus two retries).
pub const RESET_ATTEMPTS: u32 = 3;

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

/// Where `wait_for_edl` starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdlWait {
    /// Rescue: the cable guide is on screen; the `wait_edl` timer starts once the user has
    /// finished it (answers the non-blocking `cableGuide` prompt).
    Guide,
    /// The `wait_edl` timer starts now.
    Timer,
    /// Show the tips (`edlNotFound`) right away.
    Tips,
}

/// What `wait_for_edl` is showing while it polls.
enum Phase {
    Guide(Prompt),
    Timer(Duration),
    Tips(Prompt),
}

/// Polls USB for the EDL device every second until it shows up, however long that takes,
/// and returns it. Nothing here blocks on the user: the cable guide and, once `wait_edl` has
/// run out, the troubleshooting tips (`edlNotFound`) are offered while polling goes on, so the
/// flow continues by itself as soon as the tablet appears. On the tips, `retry` starts the
/// wait over (the guide again for rescue); `cancel` stops the flow.
fn wait_for_edl(ctx: &FlowCtx, from: EdlWait) -> Result<EdlUsbInfo, FlowEnd> {
    let d = ctx.deps();
    let begin = |from: EdlWait| match from {
        EdlWait::Guide => Phase::Guide(ctx.offer(PromptKind::CableGuide)),
        EdlWait::Timer => Phase::Timer(d.clock.now()),
        EdlWait::Tips => Phase::Tips(ctx.offer(PromptKind::EdlNotFound)),
    };
    // Offered after the first look, so a tablet that's already there never sees a prompt.
    let mut phase: Option<Phase> = None;
    loop {
        ctx.check_cancel()?;
        if let Some(u) = d.usb.edl_device() {
            if let Some(Phase::Tips(p)) = &phase {
                // Stop pressed just as the tablet showed up: Stop wins.
                if ctx.poll_answer(p)?.is_some_and(|o| o != "retry") {
                    return Err(FlowEnd::Cancelled);
                }
                ctx.info("Found your tablet");
                ctx.withdraw();
            } else if from == EdlWait::Guide && !matches!(phase, Some(Phase::Timer(_))) {
                // Found before the guide was finished: take it down (step 2 still ends in waitEdl).
                ctx.sub_step(StepId::WaitEdl);
            }
            return Ok(u);
        }
        phase = Some(match phase.take() {
            None => begin(from),
            Some(Phase::Guide(p)) => match ctx.poll_answer(&p)? {
                Some(_) => {
                    ctx.sub_step(StepId::WaitEdl);
                    ctx.info(&format!(
                        "Cable steps done; waiting up to {} s for the tablet in rescue (EDL) mode",
                        d.timeouts.wait_edl.as_secs()
                    ));
                    Phase::Timer(d.clock.now())
                }
                None => Phase::Guide(p),
            },
            Some(Phase::Timer(start)) => {
                if d.clock.now().saturating_sub(start) >= d.timeouts.wait_edl {
                    ctx.info("The tablet didn't show up in rescue mode in time; still looking");
                    Phase::Tips(ctx.offer(PromptKind::EdlNotFound))
                } else {
                    Phase::Timer(start)
                }
            }
            Some(Phase::Tips(p)) => match ctx.poll_answer(&p)?.as_deref() {
                // Never re-issue `adb reboot edl` (adb is gone by now): just look again.
                Some("retry") => {
                    ctx.info("Looking for the tablet again");
                    if from == EdlWait::Guide {
                        ctx.sub_step(StepId::CableGuide);
                        begin(EdlWait::Guide)
                    } else {
                        ctx.withdraw();
                        Phase::Timer(d.clock.now())
                    }
                }
                Some(_) => return Err(FlowEnd::Cancelled),
                None => Phase::Tips(p),
            },
        });
        d.clock.sleep(EDL_POLL);
    }
}

/// Asks to install the EDL USB driver and installs it.
fn install_driver(ctx: &FlowCtx) -> Result<(), FlowEnd> {
    match ctx.ask(PromptKind::InstallDriver)?.as_str() {
        "install" => {
            ctx.info("Installing the USB driver");
            ctx.deps().host.install_edl_driver()?;
            Ok(())
        }
        _ => Err(FlowEnd::Cancelled),
    }
}

/// Gets the tablet into EDL mode (rebooting it via adb when `serial` is given and it isn't
/// already there) and makes sure the computer may talk to it. Without `serial` (rescue) the
/// user brings it there with the cable guide, and the `wait_edl` timer only starts once they
/// have finished it.
pub fn enter_edl(ctx: &FlowCtx, serial: Option<&str>) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.check_cancel()?;
    if serial.is_some() {
        // From here a failure or cancel before any write restarts the tablet (flows/mod.rs).
        ctx.note_edl_entered();
    }
    let already = d.usb.edl_device().is_some();
    if already {
        ctx.info("The tablet is already in rescue (EDL) mode");
    } else if d.host.needs_udev() {
        // Linux: ask for USB access while the tablet is still in Android (or before the cable
        // guide), like the Windows driver below, so the prompt comes first.
        ctx.info("This computer doesn't let Inkroot use the tablet in rescue mode yet");
        grant_usb_access(ctx)?;
    }
    if let (false, Some(s)) = (already, serial) {
        // Windows: install the driver while the tablet is still in Android, so the tablet's
        // first hello in EDL reaches a driver instead of being lost on a driverless device.
        if d.host.needs_edl_driver_before_edl() {
            ctx.info("The USB driver for rescue mode isn't installed yet");
            install_driver(ctx)?;
        }
        ctx.info("Restarting the tablet into rescue (EDL) mode");
        d.adb
            .reboot(s, RebootTarget::Edl)
            .map_err(|e| FlowEnd::Failed(e.into()))?;
    }
    let usb = wait_for_edl(
        ctx,
        if serial.is_some() {
            EdlWait::Timer
        } else {
            EdlWait::Guide
        },
    )?;
    ctx.info("Found the tablet in rescue (EDL) mode");
    if d.host.needs_edl_driver(&usb) {
        ctx.info(&format!(
            "The tablet's rescue-mode USB driver: {}",
            usb.driver.as_deref().unwrap_or("none")
        ));
        install_driver(ctx)?;
    }
    // `needs_udev` is only ever true on Linux (the macOS/Windows host setups return false).
    // Now that the device is here, it means "this user can't open its node".
    if d.host.needs_udev() {
        ctx.info("This computer doesn't let Inkroot open the tablet in rescue mode");
        grant_usb_access(ctx)?;
    }
    Ok(())
}

/// How long a freshly installed udev rule may take to reach a 9008 device that's plugged in.
pub const UDEV_SETTLE: Duration = Duration::from_secs(5);
const UDEV_POLL: Duration = Duration::from_millis(500);

/// Linux: asks for USB access (`installUdev`), installs the rule (one password prompt), then
/// waits up to `UDEV_SETTLE` until Inkroot can open the 9008 device (when it's plugged in).
/// Declining cancels; access still missing afterwards fails with `usbPermission`.
pub fn grant_usb_access(ctx: &FlowCtx) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    match ctx.ask(PromptKind::InstallUdev)?.as_str() {
        "install" => {}
        _ => return Err(FlowEnd::Cancelled),
    }
    ctx.info("Allowing access to the tablet in rescue mode");
    d.host.install_udev()?;
    let start = d.clock.now();
    while d.host.needs_udev() {
        if d.clock.now().saturating_sub(start) >= UDEV_SETTLE {
            ctx.info("Inkroot still can't open the tablet in rescue mode");
            return Err(FlowEnd::Failed(usb_permission(format!(
                "udev rule installed, but the rescue-mode USB device still isn't accessible after {}s",
                UDEV_SETTLE.as_secs()
            ))));
        }
        d.clock.sleep(UDEV_POLL);
    }
    ctx.info("Inkroot may now use the tablet in rescue mode");
    Ok(())
}

/// After edl couldn't open the 9008 device for lack of permission: asks for access once
/// (`asked` remembers it) when the host can grant it, so the caller can try again. Otherwise
/// fails with `usbPermission`.
pub fn regain_usb_access(ctx: &FlowCtx, asked: &mut bool, detail: String) -> Result<(), FlowEnd> {
    if !*asked && ctx.deps().host.needs_udev() {
        *asked = true;
        return grant_usb_access(ctx);
    }
    Err(FlowEnd::Failed(usb_permission(detail)))
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
    // Asked for USB access once already (after edl couldn't open the device).
    let mut asked_access = false;
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
                if wait_for_edl(ctx, EdlWait::Tips).is_err() {
                    return Err(stop(
                        format!("tablet not in EDL before writing {part}"),
                        FlowEnd::Cancelled,
                    ));
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
            let outcome = (|| -> Result<Outcome, String> {
                match d.edl.write(part, img, &|done, total| {
                    ctx.progress(done, total, "Writing")
                }) {
                    Ok(()) => {}
                    // No handshake, so nothing reached the tablet.
                    Err(e @ ToolError::Stalled { .. }) => {
                        return Ok(Outcome::NotAnswering(format!("write {part}: {e:?}")));
                    }
                    // edl couldn't open the device, so nothing reached the tablet.
                    Err(e @ ToolError::UsbPermission { .. }) => {
                        return Ok(Outcome::NoPermission(format!("write {part}: {e:?}")));
                    }
                    Err(e) => return Err(format!("write {part}: {e}")),
                }
                // 4. Read back. Cancel is rejected while writing, so a fresh token. If the
                // connection drops mid-check, wait for the tablet to reappear and check again.
                ctx.sub_step(StepId::VerifyWrite);
                let mut attempts = 0;
                loop {
                    attempts += 1;
                    let read =
                        d.edl
                            .read(part, &verify, &CancelToken::default(), &|done, total| {
                                ctx.progress(done, total, "Checking")
                            });
                    if let Err(e @ ToolError::Stalled { .. }) = &read {
                        // Written, but the tablet stopped answering before the check: as
                        // with a lost connection, never reset; ask.
                        ctx.note_edl_unresponsive();
                        return Ok(Outcome::Lost(format!("read back {part}: {e:?}")));
                    }
                    let read = read.map_err(|e| format!("read back {part}: {e}"));
                    match read {
                        Ok(()) => {
                            let got =
                                sha256_file(&verify).map_err(|e| format!("hash read-back: {e}"))?;
                            return if got.eq_ignore_ascii_case(expected_sha) {
                                Ok(Outcome::Match {
                                    reconnected: attempts > 1,
                                })
                            } else {
                                Err(format!(
                                    "read-back of {part}: sha256 {got}, expected {expected_sha}"
                                ))
                            };
                        }
                        Err(e) if connection_lost(&e, d.usb.edl_device().is_none()) => {
                            ctx.info(&format!("The connection dropped during the check: {e}"));
                            if !wait_for_edl_return(ctx) {
                                return Ok(Outcome::Lost(e));
                            }
                            if attempts >= RECONNECT_ATTEMPTS {
                                return Err(e);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
            })();
            // 5.
            ctx.set_writing(false);
            let _ = std::fs::remove_file(&verify);
            match outcome {
                Ok(Outcome::NotAnswering(detail)) => {
                    ctx.note_edl_unresponsive();
                    return Err(FlowEnd::Failed(not_answering(detail)));
                }
                Ok(Outcome::NoPermission(detail)) => {
                    ctx.info("Inkroot isn't allowed to open the tablet in rescue mode");
                    regain_usb_access(ctx, &mut asked_access, detail)?;
                    continue;
                }
                Ok(Outcome::Lost(detail)) => {
                    // The write finished but the check never could: don't reset, ask.
                    return match ctx.ask(PromptKind::ConnectionLostAfterWrite) {
                        Ok(o) if o == "restartedOk" => {
                            ctx.info("The tablet started normally; the copy wasn't re-checked");
                            Ok(())
                        }
                        _ => Err(fail(
                            ErrorCode::HashMismatch,
                            CONNECTION_LOST_MESSAGE,
                            detail,
                            Some(SuggestedAction::GoToRescue),
                        )),
                    };
                }
                Ok(Outcome::Match { reconnected }) => {
                    if reconnected {
                        ctx.info("Connection dropped during the check — reconnected and verified");
                    }
                    ctx.info(&format!("{part} matches"));
                    ctx.sub_step(StepId::Reset);
                    return restart_from_edl(ctx);
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

/// How long to wait for the tablet to come back after the connection drops during the check.
const RECONNECT_WINDOW: Duration = Duration::from_secs(30);
const RECONNECT_POLL: Duration = Duration::from_secs(1);
/// Read-back attempts in total (one retry after a reconnect).
const RECONNECT_ATTEMPTS: u32 = 2;
pub const CONNECTION_LOST_MESSAGE: &str = "The connection dropped while Inkroot was checking the copy on your tablet. It may already be fixed. Run Rescue again to be sure.";

enum Outcome {
    Match {
        reconnected: bool,
    },
    /// The write finished, but the 9008 device vanished and never came back (or stopped
    /// answering).
    Lost(String),
    /// edl never got past the handshake for the write: nothing was written.
    NotAnswering(String),
    /// edl wasn't allowed to open the USB device for the write: nothing was written.
    NoPermission(String),
}

/// A read-back error that looks like the USB connection went away (and the device is gone).
fn connection_lost(err: &str, device_absent: bool) -> bool {
    let e = err.to_lowercase();
    device_absent
        && [
            "no such device",
            "usb error",
            "usberror",
            "libusb",
            "device not found",
            "disconnected",
            "no qualcomm device",
        ]
        .iter()
        .any(|m| e.contains(m))
}

/// Polls up to `RECONNECT_WINDOW` for the 9008 device to reappear.
fn wait_for_edl_return(ctx: &FlowCtx) -> bool {
    let d = ctx.deps();
    let start = d.clock.now();
    loop {
        if d.usb.edl_device().is_some() {
            return true;
        }
        if d.clock.now().saturating_sub(start) >= RECONNECT_WINDOW {
            return false;
        }
        d.clock.sleep(RECONNECT_POLL);
    }
}

/// Waits up to `RESET_SETTLE` for the 9008 device to go away. `cancellable` lets a cancel
/// request cut the wait short (never on the best-effort paths, which run after a cancel).
fn edl_left(ctx: &FlowCtx, cancellable: bool) -> Result<bool, FlowEnd> {
    let d = ctx.deps();
    let start = d.clock.now();
    loop {
        if cancellable {
            ctx.check_cancel()?;
        }
        if d.usb.edl_device().is_none() {
            return Ok(true);
        }
        if d.clock.now().saturating_sub(start) >= RESET_SETTLE {
            return Ok(false);
        }
        d.clock.sleep(EDL_POLL);
    }
}

/// Sends `edl reset` up to `RESET_ATTEMPTS` times, until the tablet has left EDL. A reset
/// that errors, or one that "succeeds" while the 9008 device is still there `RESET_SETTLE`
/// later, is retried. Returns whether the tablet left EDL.
///
/// `cancellable` is false on the best-effort paths (after a failure or cancel): there each
/// reset has a short timeout, and one that times out isn't retried, so a tablet that doesn't
/// answer never keeps `edl` running for minutes.
fn reset_attempts(ctx: &FlowCtx, cancellable: bool) -> Result<bool, FlowEnd> {
    let d = ctx.deps();
    for attempt in 1..=RESET_ATTEMPTS {
        if attempt > 1 {
            ctx.info(&format!(
                "Trying the restart again (attempt {attempt} of {RESET_ATTEMPTS})"
            ));
        }
        // A tablet that didn't answer edl's handshake gets no restart command: only a
        // restart by hand (the power button) gets it out.
        if ctx.edl_unresponsive() {
            return edl_left(ctx, cancellable);
        }
        let r = if cancellable {
            d.edl.reset()
        } else {
            d.edl.reset_quick()
        };
        if let Err(e) = r {
            ctx.info(&format!("The restart command reported a problem: {e}"));
            if let ToolError::Stalled { .. } = e {
                ctx.note_edl_unresponsive();
                return edl_left(ctx, cancellable);
            }
            if !cancellable && matches!(e, ToolError::Timeout { .. }) {
                return Ok(d.usb.edl_device().is_none());
            }
        }
        if edl_left(ctx, cancellable)? {
            return Ok(true);
        }
        ctx.info("The tablet is still in rescue (EDL) mode");
    }
    Ok(false)
}

/// Restarts the tablet out of EDL after a matching write or a finished backup. Never waits
/// silently: if `edl reset` can't get it out after `RESET_ATTEMPTS` tries, the user is asked to
/// try again or hold the power button.
pub fn restart_from_edl(ctx: &FlowCtx) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.note_edl_reset();
    ctx.info("Restarting the tablet");
    let mut left = reset_attempts(ctx, true)?;
    while !left {
        match ctx.ask(PromptKind::StillInEdl)?.as_str() {
            "retry" => left = reset_attempts(ctx, true)?,
            // "heldPower": the user restarted it by hand. Give it a moment to drop off USB.
            _ => {
                left = edl_left(ctx, true)?;
                if !left {
                    ctx.info("The tablet still shows up in rescue (EDL) mode");
                }
            }
        }
    }
    if d.usb.edl_device().is_none() {
        ctx.info("The tablet has left rescue mode");
    }
    Ok(())
}

/// Best-effort restart out of EDL on a failure path (nothing written, or a verified copy
/// already saved): retries like `restart_from_edl` but never asks and ignores cancel.
pub fn best_effort_restart(ctx: &FlowCtx) {
    if ctx.edl_unresponsive() {
        return;
    }
    ctx.note_edl_reset();
    match reset_attempts(ctx, false) {
        Ok(true) => {}
        _ => ctx.info(
            "The tablet is still in rescue mode. Hold its power button for about 10 seconds to restart it.",
        ),
    }
}

fn adb_state_name(s: AdbState) -> &'static str {
    match s {
        AdbState::Device => "device",
        AdbState::Unauthorized => "unauthorized",
        AdbState::Offline => "offline",
    }
}

/// Waits for `serial` to come back as an authorized adb device.
pub fn wait_boot(ctx: &FlowCtx, serial: &str) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.info("Waiting for the tablet to restart");
    // Only changes are logged: the poll runs every 2 s for minutes.
    let mut seen: Option<AdbState> = None;
    loop {
        let start = d.clock.now();
        loop {
            ctx.check_cancel()?;
            let now = d
                .adb
                .devices()
                .ok()
                .and_then(|v| v.iter().find(|x| x.serial == serial).map(|x| x.state));
            if now != seen {
                match now {
                    Some(st) => ctx.info(&format!("Tablet seen: {}", adb_state_name(st))),
                    None => ctx.info("Tablet not visible over USB right now"),
                }
                seen = now;
            }
            if now == Some(AdbState::Device) {
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
