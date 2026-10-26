//! Rescue (spec §3.3): bring back a tablet that won't start by writing a known-good boot
//! image over EDL. No adb at all: the tablet is assumed to be unreachable that way.

use super::common::{enter_edl, write_and_verify};
use super::root::boot_part;
use super::{FlowCtx, FlowEnd};
use crate::image::{sha256_file, slot_from_filename, validate_boot_image};
use crate::ipc::{ErrorCode, FlowError, FlowResult, PromptKind, Slot, StepId, SuggestedAction};
use crate::profile::DeviceProfile;
use std::path::PathBuf;

const STEPS: u8 = 4;

pub const NO_IMAGE_MESSAGE: &str =
    "Choose a saved copy or a startup file for Inkroot to put back on your tablet.";
pub const MISSING_BACKUP_MESSAGE: &str =
    "Inkroot couldn't find that saved copy. Choose another one or pick a file.";
pub const MISSING_SLOT_MESSAGE: &str =
    "That saved copy doesn't have the startup file for this slot. Choose another one.";

fn bad_image(message: &str, detail: impl Into<String>) -> FlowEnd {
    FlowEnd::Failed(FlowError {
        code: ErrorCode::BadImage,
        message: message.into(),
        detail: detail.into(),
        suggested_action: None,
    })
}

/// What rescue will write.
enum Source {
    /// A file in the backup library: re-verified against its manifest before the write.
    Library {
        dir: PathBuf,
        manifest: Box<crate::backup::Manifest>,
        name: String,
    },
    /// A loose file the user picked; its hash was computed fresh in step 1.
    Loose { _scratch: tempfile::TempDir },
}

struct Picked {
    source: Source,
    path: PathBuf,
    sha: String,
    slot: Slot,
    scratch: PathBuf,
}

fn ask_slot(ctx: &FlowCtx) -> Result<Slot, FlowEnd> {
    ctx.sub_step(StepId::ChooseSlot);
    ctx.info("The file's name doesn't say which slot it belongs to");
    let slot = match ctx.ask(PromptKind::ChooseSlot)?.as_str() {
        "a" => Slot::A,
        _ => Slot::B,
    };
    Ok(slot)
}

/// Step 1: pick and check the image (and its slot).
fn pick_image(ctx: &FlowCtx, profile: &DeviceProfile) -> Result<Picked, FlowEnd> {
    let d = ctx.deps();
    let opts = ctx.opts().clone();
    if let Some(id) = opts.backup_id.as_deref() {
        let Some((dir, manifest)) = d.library.get(id) else {
            return Err(bad_image(
                MISSING_BACKUP_MESSAGE,
                format!("backup {id} not found"),
            ));
        };
        let slot = opts.slot.unwrap_or(manifest.slot);
        let name = format!("{}.img", slot.boot());
        let Some(entry) = manifest.file(&name) else {
            return Err(bad_image(
                MISSING_SLOT_MESSAGE,
                format!("{name} is not listed in the manifest of {id}"),
            ));
        };
        let sha = entry.sha256.clone();
        let path = d.library.verify_file(&dir, &manifest, &name)?;
        validate_boot_image(&path, profile.boot_size)?;
        ctx.info(&format!(
            "Using {name} from the saved copy {id} (sha256 {sha})"
        ));
        return Ok(Picked {
            scratch: dir.clone(),
            source: Source::Library {
                dir,
                manifest: Box::new(manifest),
                name,
            },
            path,
            sha,
            slot,
        });
    }
    if let Some(p) = opts.image_path.as_deref() {
        let path = PathBuf::from(p);
        validate_boot_image(&path, profile.boot_size)?;
        let sha = sha256_file(&path).map_err(|e| {
            bad_image(
                "Inkroot couldn't read that file.",
                format!("hash {}: {e}", path.display()),
            )
        })?;
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let slot = match opts.slot.or_else(|| slot_from_filename(&fname)) {
            Some(s) => s,
            None => ask_slot(ctx)?,
        };
        let tmp = tempfile::tempdir().map_err(|e| FlowError {
            code: ErrorCode::Internal,
            message: "Inkroot couldn't make a temporary folder on this computer.".into(),
            detail: format!("tempdir: {e}"),
            suggested_action: Some(SuggestedAction::Retry),
        })?;
        ctx.info(&format!(
            "Using {} for slot {} (sha256 {sha})",
            path.display(),
            slot.suffix()
        ));
        return Ok(Picked {
            scratch: tmp.path().to_path_buf(),
            source: Source::Loose { _scratch: tmp },
            path,
            sha,
            slot,
        });
    }
    Err(bad_image(
        NO_IMAGE_MESSAGE,
        "rescue started with neither backupId nor imagePath",
    ))
}

/// The Rescue flow (4 steps).
pub fn run_rescue(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    let d = ctx.deps();
    let profile = d.profiles.first().cloned().ok_or_else(|| FlowError {
        code: ErrorCode::Internal,
        message: "Something went wrong inside Inkroot. Please reinstall it and try again.".into(),
        detail: "no device profiles loaded".into(),
        suggested_action: None,
    })?;

    ctx.step(StepId::PickImage, 1, STEPS);
    let picked = pick_image(ctx, &profile)?;

    ctx.step(StepId::CableGuide, 2, STEPS);
    ctx.sub_step(StepId::WaitEdl);
    ctx.info("Waiting for your tablet in rescue (EDL) mode");
    enter_edl(ctx, None)?;

    ctx.step(StepId::Write, 3, STEPS);
    // Re-check a library file against its manifest right before the write.
    if let Source::Library {
        dir,
        manifest,
        name,
    } = &picked.source
    {
        d.library.verify_file(dir, manifest, name)?;
    }
    write_and_verify(
        ctx,
        &boot_part(&profile, picked.slot),
        &picked.path,
        &picked.sha,
        &picked.scratch,
    )?;

    ctx.step(StepId::Done, 4, STEPS);
    ctx.info(
        "Done. Unplug the deep-flash cable, put the tablet on its normal charger and hold power for about 5 seconds.",
    );
    Ok(FlowResult::default())
}
