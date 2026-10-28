//! Root (spec §3.2): make sure a verified stock backup of this exact tablet exists, patch its
//! active boot image on the tablet with Magisk (verity kept on) plus the boot-service hook,
//! write it over EDL with a read-back check, then confirm root and the boot service came up.

use super::backup::take_backup;
use super::common::{cable_check, check_device, enter_edl, wait_boot, write_and_verify};
use super::{FlowCtx, FlowEnd};
use crate::backup::{Manifest, PatchedInfo};
use crate::device::{
    BOOTSVC_LOG, MAGISK_CHECK, bootsvc_boot_lines, bootsvc_ok, parse_magisk_version,
    try_bootsvc_line_count,
};
use crate::ipc::{
    BackupFile, DeviceInfo, ErrorCode, FlowError, FlowResult, Slot, StepId, SuggestedAction,
};
use crate::profile::DeviceProfile;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const NO_STOCK_BACKUP_MESSAGE: &str =
    "Inkroot needs an original safety copy of your tablet first.";
pub const ROOT_VERIFY_FAILED_MESSAGE: &str = "Your tablet restarted, but root didn't start. Your safety copy is ready if you want to go back.";
pub const PATCHED_FILE: &str = "patched-boot.img";
pub const BOOTSVC_READ_MESSAGE: &str =
    "Inkroot couldn't read your tablet's start-up log. Check the cable and try again.";

const STEPS: u8 = 4;
const VERIFY_POLL: Duration = Duration::from_secs(5);
const BEFORE_TRIES: u32 = 3;
const BEFORE_RETRY: Duration = Duration::from_secs(2);

pub(crate) fn no_stock_backup(detail: impl Into<String>) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::NoStockBackup,
        message: NO_STOCK_BACKUP_MESSAGE.into(),
        detail: detail.into(),
        suggested_action: Some(SuggestedAction::ImportBackup),
    })
}

fn internal(message: &str, detail: String) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::Internal,
        message: message.into(),
        detail,
        suggested_action: None,
    })
}

pub(crate) fn boot_part(profile: &DeviceProfile, slot: Slot) -> String {
    match slot {
        Slot::A => profile.partitions.boot_a.clone(),
        Slot::B => profile.partitions.boot_b.clone(),
    }
}

/// The Root flow (4 steps).
pub fn run_root(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    ctx.step(StepId::CheckDevice, 1, STEPS);
    let (serial, info, profile) = check_device(ctx, Some(false))?;
    root_from_step2(ctx, &serial, &info, &profile)
}

/// Root from step 2 ("Safety copy") on. Reinstall continues here when the hook is absent.
pub(crate) fn root_from_step2(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
) -> Result<FlowResult, FlowEnd> {
    let (id, dir, manifest) = safety_copy(ctx, serial, info, profile)?;
    let (manifest, before) = patch_and_write(ctx, serial, info, profile, &dir, manifest)?;
    verify_root(ctx, serial, profile, before)?;
    ctx.info("Your tablet is rooted");
    Ok(FlowResult {
        backup: Some(manifest.to_summary(&id, &dir)),
        magisk_version: Some(profile.magisk.version.clone()),
        ..FlowResult::default()
    })
}

/// The stock backup to use: the one named by `opts.backup_id` (which must be a verified
/// original of this tablet, firmware and active slot, else `noStockBackup`), or without it
/// the newest such backup (`None` if there is none).
pub(crate) fn stock_backup(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
) -> Result<Option<(String, PathBuf, Manifest)>, FlowEnd> {
    let d = ctx.deps();
    let Some(id) = ctx.opts().backup_id.clone() else {
        return Ok(d
            .library
            .stock_for(serial, &info.display_id, info.slot, profile));
    };
    let Some((dir, m)) = d.library.get(&id) else {
        return Err(no_stock_backup(format!("backup {id} not found")));
    };
    let want = format!("{}.img", info.slot.boot());
    let mut why = Vec::new();
    if !m.stock {
        why.push("not stock".to_string());
    } else if !m.trusted_stock(profile) {
        why.push(format!(
            "imported file is not a known original ({:?})",
            m.stock_evidence
        ));
    }
    if !m.verified {
        why.push("not verified".to_string());
    }
    if m.serial != serial {
        why.push(format!("serial {} (device {serial})", m.serial));
    }
    if m.firmware.display_id != info.display_id {
        why.push(format!(
            "firmware {} (device {})",
            m.firmware.display_id, info.display_id
        ));
    }
    if m.slot != info.slot {
        why.push(format!("slot {:?} (device {:?})", m.slot, info.slot));
    }
    if m.file(&want).is_none() {
        why.push(format!("{want} not listed"));
    }
    if !why.is_empty() {
        return Err(no_stock_backup(format!(
            "backup {id} can't be used: {}",
            why.join(", ")
        )));
    }
    Ok(Some((id, dir, m)))
}

/// Step 2: the newest verified stock backup of this tablet, else a fresh one (folded into
/// this step). Anything that isn't a verified original refuses with `noStockBackup`.
fn safety_copy(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
) -> Result<(String, PathBuf, Manifest), FlowEnd> {
    if let Some(found) = stock_backup(ctx, serial, info, profile)? {
        ctx.step(StepId::VerifyBackup, 2, STEPS);
        ctx.info(&format!(
            "Using the original safety copy {} ({})",
            found.0,
            found.1.display()
        ));
        return Ok(found);
    }
    if info.rooted {
        // A copy taken now would include root, so it could never be the original.
        return Err(no_stock_backup(format!(
            "no stock backup for {serial} ({}, slot {:?}) and the tablet is rooted",
            info.display_id, info.slot
        )));
    }
    ctx.step(StepId::ReadPartitions, 2, STEPS);
    ctx.info("No original safety copy of this tablet yet, so Inkroot is making one first");
    let (id, dir, m) = take_backup(ctx, serial, info, profile, 2, STEPS)?;
    if !(m.stock && m.verified) {
        return Err(no_stock_backup(format!(
            "new backup {id} is stock={} verified={} ({:?})",
            m.stock, m.verified, m.stock_evidence
        )));
    }
    Ok((id, dir, m))
}

/// Drops any earlier patched image from the manifest (and disk) so a manifest never lists a
/// file that is about to be replaced.
fn forget_patched(ctx: &FlowCtx, dir: &Path, m: &mut Manifest) -> Result<(), FlowEnd> {
    let listed = m.files.iter().any(|f| f.name == PATCHED_FILE);
    if m.patched.is_none() && !listed {
        return Ok(());
    }
    m.patched = None;
    m.files.retain(|f| f.name != PATCHED_FILE);
    ctx.deps().library.write_manifest(dir, m)?;
    let _ = std::fs::remove_file(dir.join(PATCHED_FILE));
    Ok(())
}

/// The boot-service line count before the write, retried a few times. Only a missing log
/// counts as 0: guessing 0 after an adb error would let any old line pass verification.
pub(crate) fn bootsvc_lines_before(ctx: &FlowCtx, serial: &str) -> Result<usize, FlowEnd> {
    let d = ctx.deps();
    let mut last = String::new();
    for attempt in 1..=BEFORE_TRIES {
        ctx.check_cancel()?;
        match try_bootsvc_line_count(d.adb.as_ref(), serial, &d.timeouts) {
            Ok(n) => return Ok(n),
            Err(e) => {
                ctx.info(&format!(
                    "Couldn't read the start-up log (try {attempt} of {BEFORE_TRIES}): {e}"
                ));
                last = e;
            }
        }
        if attempt < BEFORE_TRIES {
            d.clock.sleep(BEFORE_RETRY);
        }
    }
    Err(FlowEnd::Failed(FlowError {
        code: ErrorCode::Internal,
        message: BOOTSVC_READ_MESSAGE.into(),
        detail: format!("bootsvc line count before write: {last}"),
        suggested_action: Some(SuggestedAction::Reconnect),
    }))
}

/// Step 3: patch on the tablet, then cable check, EDL, write and read-back.
/// Returns the updated manifest and the boot-service line count before the write.
fn patch_and_write(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
    dir: &Path,
    mut m: Manifest,
) -> Result<(Manifest, usize), FlowEnd> {
    let d = ctx.deps();
    ctx.step(StepId::Patch, 3, STEPS);
    let slot = info.slot;

    // 1. The stock image must still match its manifest before anything touches the tablet.
    let stock_name = format!("{}.img", slot.boot());
    let stock = d.library.verify_file(dir, &m, &stock_name)?;
    ctx.info(&format!("{stock_name} still matches the safety copy"));

    // 2. Boot-service lines before the write: verification wants a fresh one after it.
    let before = bootsvc_lines_before(ctx, serial)?;

    // 3. Inputs: Magisk payload (integrity-checked, unpacked into a temporary folder that is
    // removed right after the push) and the boot-service files.
    let apk = d
        .tools
        .require(&d.tools.magisk_apk(&profile.magisk.version))?;
    let rc = d.tools.require(&d.tools.boot_service("init.bootsvc.rc"))?;
    let bootsvc = d.tools.require(&d.tools.boot_service("bootsvc.sh"))?;
    let service = d
        .tools
        .require(&d.tools.boot_service("inkroot-default-service.sh"))?;
    {
        let tmp = tempfile::tempdir().map_err(|e| FlowError {
            code: ErrorCode::Internal,
            message: "Inkroot couldn't make a temporary folder on this computer.".into(),
            detail: format!("tempdir: {e}"),
            suggested_action: Some(SuggestedAction::Retry),
        })?;
        let payload = crate::magisk::extract_payload(&apk, &d.magisk_apk_sha256, tmp.path())?;
        ctx.check_cancel()?;
        ctx.info("Copying Magisk and the boot service to your tablet");
        crate::magisk::push_patch_inputs(
            d.adb.as_ref(),
            serial,
            &payload,
            &stock,
            &rc,
            &bootsvc,
            &service,
        )?;
    }

    // 4. Patch on the tablet (KEEPVERITY/KEEPFORCEENCRYPT are validated inside).
    ctx.check_cancel()?;
    forget_patched(ctx, dir, &mut m)?;
    let patched = dir.join(PATCHED_FILE);
    ctx.info(&format!(
        "Preparing the rooted startup file with Magisk {}",
        profile.magisk.version
    ));
    let sha = match crate::magisk::run_patch(
        d.adb.as_ref(),
        serial,
        &profile.magisk.patch_env,
        profile.boot_size,
        &patched,
        &d.timeouts,
    ) {
        Ok(sha) => sha,
        Err(e) => {
            let _ = std::fs::remove_file(&patched);
            return Err(e.into());
        }
    };
    let size = std::fs::metadata(&patched)
        .map_err(|e| {
            internal(
                "Inkroot couldn't read the prepared file.",
                format!("{}: {e}", patched.display()),
            )
        })?
        .len();
    ctx.info(&format!(
        "Prepared {PATCHED_FILE} ({size} bytes, sha256 {sha})"
    ));
    m.patched = Some(PatchedInfo {
        file: PATCHED_FILE.into(),
        sha256: sha.clone(),
        magisk_version: profile.magisk.version.clone(),
    });
    m.files.retain(|f| f.name != PATCHED_FILE);
    m.files.push(BackupFile {
        name: PATCHED_FILE.into(),
        size,
        sha256: sha.clone(),
    });
    d.library.write_manifest(dir, &m)?;

    // 5. Rescue cable, EDL, write with read-back (reset only on a match).
    ctx.sub_step(StepId::CableCheck);
    cable_check(ctx)?;
    ctx.sub_step(StepId::EnterEdl);
    enter_edl(ctx, Some(serial))?;
    write_and_verify(ctx, &boot_part(profile, slot), &patched, &sha, dir)?;
    Ok((m, before))
}

/// What the last verification poll saw (for the failure detail).
#[derive(Debug, Default)]
struct Seen {
    magisk: Option<String>,
    lines: usize,
    ok: bool,
    error: Option<String>,
}

fn observe(ctx: &FlowCtx, serial: &str) -> Seen {
    let d = ctx.deps();
    let t = &d.timeouts;
    let mut seen = Seen::default();
    match d.adb.shell(serial, MAGISK_CHECK, t.adb_shell) {
        Ok(o) if o.code == 0 => seen.magisk = parse_magisk_version(&o.stdout),
        Ok(_) => {}
        Err(e) => seen.error = Some(format!("magisk -c: {e}")),
    }
    match d.adb.shell(serial, BOOTSVC_LOG, t.adb_shell) {
        Ok(o) if o.code == 0 => {
            seen.lines = bootsvc_boot_lines(&o.stdout);
            seen.ok = bootsvc_ok(&o.stdout);
        }
        Ok(_) => {}
        Err(e) => seen.error = Some(format!("bootsvc.log: {e}")),
    }
    seen
}

/// Step 4: wait for Android, then poll until Magisk reports the pinned version and the boot
/// service logged a fresh root line. adb errors while polling are retried.
fn verify_root(
    ctx: &FlowCtx,
    serial: &str,
    profile: &DeviceProfile,
    before: usize,
) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.step(StepId::WaitBoot, 4, STEPS);
    wait_boot(ctx, serial)?;
    ctx.sub_step(StepId::VerifyRoot);
    ctx.info("Checking that root and the boot service started");
    let want = &profile.magisk.version;
    let start = d.clock.now();
    loop {
        ctx.check_cancel()?;
        let seen = observe(ctx, serial);
        if seen.magisk.as_deref() == Some(want.as_str()) && seen.lines > before && seen.ok {
            ctx.info(&format!(
                "Magisk {want} is running and the boot service started as root"
            ));
            return Ok(());
        }
        if d.clock.now().saturating_sub(start) >= d.timeouts.wait_bootsvc {
            return Err(FlowEnd::Failed(FlowError {
                code: ErrorCode::VerifyFailed,
                message: ROOT_VERIFY_FAILED_MESSAGE.into(),
                detail: format!(
                    "after {}s: magisk {:?} (want {want}), bootsvc boot lines {} (before {before}), last line root+magisk ctx: {}{}",
                    d.timeouts.wait_bootsvc.as_secs(),
                    seen.magisk,
                    seen.lines,
                    seen.ok,
                    seen.error
                        .map(|e| format!(", last adb error: {e}"))
                        .unwrap_or_default()
                ),
                suggested_action: None,
            }));
        }
        d.clock.sleep(VERIFY_POLL);
    }
}
