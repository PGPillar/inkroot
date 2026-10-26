//! Import an existing boot image (spec §3.1b) into the backup library.
//!
//! An imported file counts as stock only when its SHA-256 is a known original in the profile
//! for exactly the slot it is filed under and the firmware it belongs to. Nothing on the tablet
//! is ever used to vouch for an imported file: any other file is kept for Rescue only.

use super::backup::firmware_of;
use super::{FlowCtx, FlowEnd};
use crate::backup::{FirmwareInfo, Manifest, StockEvidence};
use crate::device::probe;
use crate::image::{copy_verified, sha256_file, slot_from_filename, validate_boot_image};
use crate::ipc::{
    BackupFile, BackupSource, DeviceInfo, DeviceStatus, ErrorCode, FlowError, FlowResult,
    PromptKind, Slot, StepId, SuggestedAction,
};
use crate::profile::DeviceProfile;
use std::path::PathBuf;

pub const UNASSIGNED_SERIAL: &str = "unassigned";
/// Shown (and logged) when an imported file isn't a known original: it is kept for Rescue only.
pub const RESCUE_ONLY_MESSAGE: &str = "Inkroot doesn't recognise this file, so it can only use it for Rescue. To root, let Inkroot make its own safety copy from your tablet.";

fn internal(message: &str, detail: String) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::Internal,
        message: message.into(),
        detail,
        suggested_action: Some(SuggestedAction::Retry),
    })
}

/// The connected tablet, if it is an authorized adb device that matches a profile (the
/// safety gates don't need to pass just to import a file).
fn connected_device(ctx: &FlowCtx) -> Option<(String, DeviceInfo, DeviceProfile)> {
    let d = ctx.deps();
    let (status, serial) = probe(d.adb.as_ref(), d.usb.as_ref(), &d.profiles, &d.timeouts);
    let DeviceStatus::Adb { info, .. } = status else {
        return None;
    };
    let profile = d
        .profiles
        .iter()
        .find(|p| info.profile_id.as_deref() == Some(p.id.as_str()))?
        .clone();
    let serial = serial.unwrap_or_else(|| info.serial.clone());
    Some((serial, info, profile))
}

fn no_firmware() -> FirmwareInfo {
    FirmwareInfo {
        build_id: String::new(),
        display_id: String::new(),
        incremental: String::new(),
        fingerprint: String::new(),
    }
}

/// The stock decision for an imported image filed under `slot`: a known original for that
/// slot and for the connected tablet's firmware (with no tablet, the known entry's own
/// firmware). Returns (stock, evidence, firmware recorded in the manifest).
fn decide_stock(
    ctx: &FlowCtx,
    device: Option<&(String, DeviceInfo, DeviceProfile)>,
    profile: &DeviceProfile,
    sha: &str,
    slot: Slot,
) -> (bool, StockEvidence, FirmwareInfo) {
    let known = match device {
        Some((_, info, p)) => p
            .known_stock(sha, slot, &info.display_id)
            .then(|| firmware_of(info, p)),
        None => profile.known_stock_entries(sha, slot).find_map(|k| {
            profile
                .firmware
                .iter()
                .find(|f| f.display_id == k.display_id)
                .map(|f| FirmwareInfo {
                    build_id: f.build_id.clone(),
                    display_id: f.display_id.clone(),
                    incremental: f.incremental.clone(),
                    fingerprint: f.fingerprint.clone(),
                })
        }),
    };
    if let Some(fw) = known {
        ctx.info(&format!(
            "The file matches a known original for {} on {}",
            slot.boot(),
            fw.display_id
        ));
        return (true, StockEvidence::KnownHash, fw);
    }
    ctx.info(RESCUE_ONLY_MESSAGE);
    ctx.info(&format!(
        "sha256 {sha} is not a known original for {}{}",
        slot.boot(),
        device
            .map(|(_, i, _)| format!(" on {}", i.display_id))
            .unwrap_or_default()
    ));
    let fw = device
        .map(|(_, info, p)| firmware_of(info, p))
        .unwrap_or_else(no_firmware);
    (false, StockEvidence::UnknownImport, fw)
}

/// The Import flow (3 steps).
pub fn run_import(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    let d = ctx.deps();

    // 1. pickImage
    ctx.step(StepId::PickImage, 1, 3);
    let Some(path) = ctx.opts().image_path.clone() else {
        return Err(FlowEnd::Failed(FlowError {
            code: ErrorCode::BadImage,
            message: "Choose your startup (boot) file first.".into(),
            detail: "importBackup started without imagePath".into(),
            suggested_action: None,
        }));
    };
    let src = PathBuf::from(&path);
    ctx.info(&format!("Checking {}", src.display()));
    // The device decides the profile when one is connected; probe once, up front.
    let device = connected_device(ctx);
    let profile = match &device {
        Some((_, _, p)) => p.clone(),
        None => d.profiles.first().cloned().ok_or_else(|| {
            internal(
                "Something went wrong inside Inkroot. Please reinstall it and try again.",
                "no device profiles loaded".into(),
            )
        })?,
    };
    validate_boot_image(&src, profile.boot_size)?;
    let sha = sha256_file(&src).map_err(|e| {
        FlowEnd::Failed(FlowError {
            code: ErrorCode::BadImage,
            message: "Inkroot couldn't read this file.".into(),
            detail: format!("hash {}: {e}", src.display()),
            suggested_action: None,
        })
    })?;
    ctx.info(&format!(
        "The file looks like a startup file (sha256 {sha})"
    ));

    // 2. stockCheck. The decision needs the slot the file is filed under, so it runs here
    // when the slot is already known (options, filename, tablet) and otherwise right after
    // the chooseSlot answer in step 3. Nothing is sent to the tablet either way.
    ctx.step(StepId::StockCheck, 2, 3);
    ctx.check_cancel()?;
    let name_hint = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let known_slot = ctx
        .opts()
        .slot
        .or_else(|| slot_from_filename(&name_hint))
        .or_else(|| device.as_ref().map(|(_, i, _)| i.slot));
    let decide = |slot| decide_stock(ctx, device.as_ref(), &profile, &sha, slot);
    let (slot, (stock, evidence, firmware)) = match known_slot {
        Some(slot) => (slot, decide(slot)),
        None => {
            // 3. chooseSlot (then the stock decision for that slot).
            ctx.step(StepId::ChooseSlot, 3, 3);
            let slot = match ctx.ask(PromptKind::ChooseSlot)?.as_str() {
                "a" => Slot::A,
                _ => Slot::B,
            };
            (slot, decide(slot))
        }
    };

    // 3. saveImport
    ctx.step(StepId::SaveImport, 3, 3);
    ctx.check_cancel()?;
    let serial = device
        .as_ref()
        .map(|(s, ..)| s.clone())
        .unwrap_or_else(|| UNASSIGNED_SERIAL.to_string());
    let (id, dir) = d.library.create_dir(&serial, d.clock.wall())?;
    let file_name = format!("{}.img", slot.boot());
    let dst = dir.join(&file_name);
    let copied = copy_verified(&src, &dst).map_err(|e| {
        let _ = std::fs::remove_file(&dst);
        let _ = std::fs::remove_dir(&dir);
        internal(
            "Inkroot couldn't save a copy of this file in your Documents folder.",
            format!("copy {} -> {}: {e}", src.display(), dst.display()),
        )
    })?;
    if !copied.eq_ignore_ascii_case(&sha) {
        let _ = std::fs::remove_file(&dst);
        let _ = std::fs::remove_dir(&dir);
        return Err(FlowEnd::Failed(FlowError {
            code: ErrorCode::BadImage,
            message: "This file changed while Inkroot was saving it, so it wasn't kept.".into(),
            detail: format!("checked sha256 {sha}, copied {copied}"),
            suggested_action: None,
        }));
    }
    let size = std::fs::metadata(&dst)
        .map_err(|e| internal("Inkroot couldn't read the saved copy.", e.to_string()))?
        .len();
    let model = device
        .as_ref()
        .map(|(_, i, _)| i.model.clone())
        .unwrap_or_else(|| profile.matcher.model.clone());
    let manifest = Manifest {
        manifest_version: 1,
        inkroot_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: d.clock.wall().to_rfc3339(),
        serial,
        model,
        profile_id: Some(profile.id.clone()),
        firmware,
        slot,
        stock,
        stock_evidence: evidence,
        verified: true,
        source: BackupSource::Imported,
        files: vec![BackupFile {
            name: file_name,
            size,
            sha256: copied,
        }],
        patched: None,
        imported_from: Some(path),
    };
    d.library.write_manifest(&dir, &manifest)?;
    ctx.info(&format!("Imported into {}", dir.display()));
    Ok(FlowResult {
        backup: Some(manifest.to_summary(&id, &dir)),
        ..FlowResult::default()
    })
}
