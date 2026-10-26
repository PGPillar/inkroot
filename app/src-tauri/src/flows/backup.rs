//! Back up (spec §3.1): read boot_a/b and vbmeta_a/b over EDL, verify the active boot by a
//! second read, decide stock, write a verified manifest. Root reuses `take_backup`.

use super::common::{check_device, enter_edl, magisk_stock_check, wait_boot};
use super::{FlowCtx, FlowEnd};
use crate::backup::{FirmwareInfo, Manifest, StockEvidence};
use crate::image::{sha256_file, validate_boot_image};
use crate::ipc::{
    BackupFile, BackupSource, DeviceInfo, ErrorCode, FlowError, FlowResult, StepId, SuggestedAction,
};
use crate::profile::DeviceProfile;
use crate::tools::ToolError;
use std::path::{Path, PathBuf};

pub const READ_FAILED_MESSAGE: &str = "Inkroot lost contact with your tablet while saving its safety copy. Nothing was changed on it.";
pub const VERIFY_FAILED_MESSAGE: &str = "The safety copy didn't match when Inkroot checked it, so it wasn't kept. Nothing was changed on your tablet. Please try again.";

/// The four partitions a backup holds, as (partition name, file name).
fn backup_parts(profile: &DeviceProfile) -> [(String, String); 4] {
    let p = &profile.partitions;
    [
        (p.boot_a.clone(), "boot_a.img".to_string()),
        (p.boot_b.clone(), "boot_b.img".to_string()),
        (p.vbmeta_a.clone(), "vbmeta_a.img".to_string()),
        (p.vbmeta_b.clone(), "vbmeta_b.img".to_string()),
    ]
}

/// The tablet's firmware as recorded in a manifest. The fingerprint comes from the profile's
/// firmware entry (the gates already matched all four fields against the live props).
pub(crate) fn firmware_of(info: &DeviceInfo, profile: &DeviceProfile) -> FirmwareInfo {
    let fingerprint = profile
        .firmware
        .iter()
        .find(|f| {
            f.build_id == info.build_id
                && f.display_id == info.display_id
                && f.incremental == info.incremental
        })
        .map(|f| f.fingerprint.clone())
        .unwrap_or_default();
    FirmwareInfo {
        build_id: info.build_id.clone(),
        display_id: info.display_id.clone(),
        incremental: info.incremental.clone(),
        fingerprint,
    }
}

/// Nothing was written to the tablet, so restarting it out of EDL is always safe here.
fn best_effort_reset(ctx: &FlowCtx) {
    ctx.note_edl_reset();
    ctx.info("Restarting the tablet (nothing was written to it)");
    if let Err(e) = ctx.deps().edl.reset() {
        ctx.info(&format!("Restart command reported a problem: {e}"));
    }
}

/// Removes the new backup folder if nothing ended up in it. A folder that already holds a
/// (verified) manifest is left alone silently.
fn drop_if_empty(ctx: &FlowCtx, dir: &Path) {
    if dir.join("manifest.json").is_file() {
        return;
    }
    if std::fs::remove_dir(dir).is_err() && dir.exists() {
        ctx.info(&format!(
            "The unfinished safety copy was left at {} (not marked as verified)",
            dir.display()
        ));
    }
}

fn read_failed(e: &ToolError, part: &str) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::EdlTimeout,
        message: READ_FAILED_MESSAGE.into(),
        detail: format!("read {part}: {e:?}"),
        suggested_action: Some(SuggestedAction::Reconnect),
    })
}

fn hash_failed(path: &Path, e: std::io::Error) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::Internal,
        message: "Inkroot couldn't read back the safety copy it just saved on this computer."
            .into(),
        detail: format!("hash {}: {e}", path.display()),
        suggested_action: Some(SuggestedAction::Retry),
    })
}

/// Reads `part` into `out` over EDL. Any failure resets the tablet (best effort; nothing was
/// written) and ends the flow calmly.
fn read_part(ctx: &FlowCtx, part: &str, out: &Path, label: &str) -> Result<(), FlowEnd> {
    if ctx.check_cancel().is_err() {
        best_effort_reset(ctx);
        return Err(FlowEnd::Cancelled);
    }
    ctx.info(&format!("Reading {part}"));
    let r = ctx
        .deps()
        .edl
        .read(part, out, ctx.cancel_token(), &|done, total| {
            ctx.progress(done, total, label)
        });
    match r {
        Ok(()) => Ok(()),
        Err(e) => {
            ctx.info(&format!("Reading {part} failed: {e}"));
            let _ = std::fs::remove_file(out);
            best_effort_reset(ctx);
            Err(match e {
                ToolError::Cancelled => FlowEnd::Cancelled,
                e => read_failed(&e, part),
            })
        }
    }
}

/// Takes a full, verified backup of the connected tablet.
///
/// Starts by creating (and write-testing) the backup folder, before the tablet is sent into
/// EDL, so an unwritable library fails while the tablet is still in Android. The caller's
/// current step covers that part.
///
/// Step numbering: the three device phases (enterEdl, readPartitions, verifyBackup) are shown
/// as `index_base + 1..=3` when they fit in `count` (the Back up flow: base 1 of 4). When
/// they don't (Root's "Safety copy" group: base 2 of 4), every phase is shown at
/// `index_base`, so the whole backup folds into one visible step.
pub(crate) fn take_backup(
    ctx: &FlowCtx,
    serial: &str,
    info: &DeviceInfo,
    profile: &DeviceProfile,
    index_base: u8,
    count: u8,
) -> Result<(String, PathBuf, Manifest), FlowEnd> {
    let d = ctx.deps();
    let fold = index_base.saturating_add(3) > count;
    let enter = |step: StepId, offset: u8| {
        let idx = if fold {
            index_base
        } else {
            index_base + offset
        };
        ctx.step(step, idx, count);
    };

    // 0. The folder first: never strand the tablet in EDL over a folder we can't write.
    let (id, dir) = d.library.create_dir(serial, d.clock.wall())?;
    ctx.info(&format!("Saving the safety copy to {}", dir.display()));

    let result = (|| -> Result<Manifest, FlowEnd> {
        // 1. EDL.
        enter(StepId::EnterEdl, 1);
        enter_edl(ctx, Some(serial))?;

        // 2. Read the four partitions.
        enter(StepId::ReadPartitions, 2);
        let parts = backup_parts(profile);
        let mut files = Vec::with_capacity(parts.len());
        for (i, (part, name)) in parts.iter().enumerate() {
            let out = dir.join(name);
            let label = format!("Saving {} of {}", i + 1, parts.len());
            read_part(ctx, part, &out, &label)?;
            let measured =
                sha256_file(&out).and_then(|sha| Ok((sha, std::fs::metadata(&out)?.len())));
            let (sha, size) = measured.map_err(|e| {
                best_effort_reset(ctx);
                hash_failed(&out, e)
            })?;
            ctx.info(&format!("Saved {name} ({size} bytes, sha256 {sha})"));
            files.push(BackupFile {
                name: name.clone(),
                size,
                sha256: sha,
            });
        }

        // 3. Verify: read the active boot a second time and compare.
        enter(StepId::VerifyBackup, 3);
        let slot = info.slot;
        let active_name = format!("{}.img", slot.boot());
        let active_part = match slot {
            crate::ipc::Slot::A => profile.partitions.boot_a.clone(),
            crate::ipc::Slot::B => profile.partitions.boot_b.clone(),
        };
        let active = files
            .iter()
            .find(|f| f.name == active_name)
            .cloned()
            .expect("the active boot is one of the four backup files");
        let verify = dir.join(format!(".verify-{active_name}"));
        read_part(ctx, &active_part, &verify, "Checking the copy")?;
        let again = sha256_file(&verify);
        let _ = std::fs::remove_file(&verify);
        let again = again.map_err(|e| {
            best_effort_reset(ctx);
            hash_failed(&verify, e)
        })?;
        if !again.eq_ignore_ascii_case(&active.sha256) {
            ctx.info("The second read of the startup partition didn't match the first");
            best_effort_reset(ctx);
            return Err(FlowEnd::Failed(FlowError {
                code: ErrorCode::VerifyFailed,
                message: VERIFY_FAILED_MESSAGE.into(),
                detail: format!(
                    "{active_part}: first read sha256 {}, second read {again}",
                    active.sha256
                ),
                suggested_action: Some(SuggestedAction::Retry),
            }));
        }
        let active_path = dir.join(&active_name);
        if let Err(e) = validate_boot_image(&active_path, profile.boot_size) {
            best_effort_reset(ctx);
            return Err(e.into());
        }
        ctx.info(&format!("{active_name} matches on a second read"));

        // Evidence known without the tablet booted: rooted at check time, or a known hash.
        let (stock, evidence) = if info.rooted {
            ctx.info("The tablet was rooted when this copy was taken, so it isn't the original");
            (false, StockEvidence::NotStockRooted)
        } else if profile.known_stock(&active.sha256, slot, &info.display_id) {
            ctx.info("The startup file matches a known original");
            (true, StockEvidence::KnownHash)
        } else {
            (false, StockEvidence::Unchecked)
        };
        let mut manifest = Manifest {
            manifest_version: 1,
            inkroot_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: d.clock.wall().to_rfc3339(),
            serial: serial.to_string(),
            model: info.model.clone(),
            profile_id: Some(profile.id.clone()),
            firmware: firmware_of(info, profile),
            slot,
            stock,
            stock_evidence: evidence,
            verified: true,
            source: BackupSource::Inkroot,
            files,
            patched: None,
            imported_from: None,
        };
        // The copy passed its read-back check, so it is saved as verified now: a tablet that
        // is slow to restart can't cost the user a good copy.
        if let Err(e) = d.library.write_manifest(&dir, &manifest) {
            best_effort_reset(ctx);
            return Err(e.into());
        }

        // Back to Android (folded into verifyBackup's tail).
        ctx.sub_step(StepId::RebootToSystem);
        ctx.note_edl_reset();
        if let Err(e) = d.edl.reset() {
            ctx.info(&format!("Restart command reported a problem: {e}"));
        }
        wait_boot(ctx, serial)?;

        // The magiskboot stock test needs the tablet back in Android.
        if manifest.stock_evidence == StockEvidence::Unchecked {
            ctx.sub_step(StepId::VerifyBackup);
            match magisk_stock_check(ctx, serial, profile, &active_path) {
                Ok(true) => {
                    manifest.stock = true;
                    manifest.stock_evidence = StockEvidence::MagiskbootTest;
                }
                Ok(false) => {
                    ctx.info("The startup file is not the original");
                    manifest.stock_evidence = StockEvidence::NotStockFailedTest;
                }
                Err(e) => {
                    // The verified, non-stock (unchecked) manifest written above stays, so the
                    // good copy is never lost; the error still reaches the user.
                    ctx.info(&format!(
                        "Couldn't check whether the startup file is the original: {} ({}). The checked copy was kept at {}, marked as not original.",
                        e.message,
                        e.detail,
                        dir.display()
                    ));
                    return Err(FlowEnd::Failed(e));
                }
            }
            d.library.write_manifest(&dir, &manifest)?;
        }
        ctx.info("Safety copy saved and checked");
        Ok(manifest)
    })();

    match result {
        Ok(m) => Ok((id, dir, m)),
        Err(e) => {
            drop_if_empty(ctx, &dir);
            Err(e)
        }
    }
}

/// The Back up flow (4 steps).
pub fn run_backup(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    ctx.step(StepId::CheckDevice, 1, 4);
    let (serial, info, profile) = check_device(ctx, None)?;
    let (id, dir, m) = take_backup(ctx, &serial, &info, &profile, 1, 4)?;
    Ok(FlowResult {
        backup: Some(m.to_summary(&id, &dir)),
        ..FlowResult::default()
    })
}
