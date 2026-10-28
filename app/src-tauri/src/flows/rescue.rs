//! Rescue (spec §3.3): bring back a tablet that won't start by writing a known-good boot
//! image over EDL. Before the write the tablet is assumed unreachable over adb, so nothing
//! touches adb until the verified write has restarted it; then step 4 quietly watches
//! `adb devices` (and the 9008 device) to tell the user whether it came back.

use super::common::{enter_edl, write_and_verify};
use super::root::boot_part;
use super::{FlowCtx, FlowEnd};
use crate::device::probe;
use crate::image::{sha256_file, slot_from_filename, validate_boot_image};
use crate::ipc::{
    DeviceStatus, ErrorCode, FlowError, FlowResult, PromptKind, RescueOutcome, Slot, StepId,
    SuggestedAction,
};
use crate::profile::DeviceProfile;
use crate::tools::adb::AdbState;
use std::path::PathBuf;
use std::time::Duration;

const STEPS: u8 = 4;
/// How often step 4 looks for the tablet.
const BOOT_POLL: Duration = Duration::from_secs(2);
/// Consecutive sightings of the 9008 device that count as "went back into rescue mode".
const EDL_SIGHTINGS: u32 = 2;

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

    ctx.info(&format!(
        "Written and checked: {} on the tablet matches your safety copy",
        boot_part(&profile, picked.slot)
    ));

    ctx.step(StepId::WaitBoot, 4, STEPS);
    let result = wait_for_tablet(ctx);
    ctx.step(StepId::Done, 4, STEPS);
    Ok(result)
}

/// What one look over USB found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Nothing,
    Adb(AdbState),
    Edl,
}

fn look(ctx: &FlowCtx) -> Seen {
    let d = ctx.deps();
    if d.usb.edl_device().is_some() {
        return Seen::Edl;
    }
    let devices = d.adb.devices().unwrap_or_default();
    let state = |s| devices.iter().any(|x| x.state == s);
    if state(AdbState::Device) {
        Seen::Adb(AdbState::Device)
    } else if state(AdbState::Unauthorized) {
        Seen::Adb(AdbState::Unauthorized)
    } else {
        Seen::Nothing
    }
}

fn outcome(o: RescueOutcome) -> FlowResult {
    FlowResult {
        rescue_outcome: Some(o),
        ..FlowResult::default()
    }
}

/// Step 4: the copy is written and checked and the tablet was restarted. Watch (quietly, only
/// changes are logged) for up to `wait_rescue_boot` for it to come back. Never fails: a tablet
/// with USB debugging off is invisible here, so "not seen" is a normal ending.
fn wait_for_tablet(ctx: &FlowCtx) -> FlowResult {
    let d = ctx.deps();
    let limit = d.timeouts.wait_rescue_boot;
    ctx.info(&format!(
        "Waiting up to {} minutes for the tablet to start (Inkroot only sees it if USB debugging is on)",
        limit.as_secs().div_ceil(60)
    ));
    let start = d.clock.now();
    let mut last: Option<Seen> = None;
    let mut edl_seen = 0u32;
    let mut offered = None;
    loop {
        let seen = look(ctx);
        if last != Some(seen) {
            ctx.info(match seen {
                Seen::Nothing => "Tablet not visible over USB right now",
                Seen::Adb(AdbState::Device) => "Tablet seen: device",
                Seen::Adb(AdbState::Unauthorized) => "Tablet seen: unauthorized",
                Seen::Adb(AdbState::Offline) => "Tablet seen: offline",
                Seen::Edl => "Tablet seen in rescue (EDL) mode",
            });
            last = Some(seen);
        }
        match seen {
            Seen::Adb(AdbState::Device) => {
                let device = match probe(d.adb.as_ref(), d.usb.as_ref(), &d.profiles, &d.timeouts).0
                {
                    DeviceStatus::Adb { info, .. } => {
                        ctx.info(&format!(
                            "The tablet has started ({})",
                            if info.rooted { "rooted" } else { "not rooted" }
                        ));
                        Some(info)
                    }
                    _ => {
                        ctx.info("The tablet has started");
                        None
                    }
                };
                return FlowResult {
                    device,
                    ..outcome(RescueOutcome::TabletBack)
                };
            }
            Seen::Adb(_) => {
                ctx.info(
                    "The tablet has started (it hasn't allowed USB debugging from this computer)",
                );
                return outcome(RescueOutcome::TabletBack);
            }
            Seen::Edl => {
                edl_seen += 1;
                if edl_seen >= EDL_SIGHTINGS {
                    ctx.info("The tablet went back into rescue (EDL) mode instead of starting");
                    return outcome(RescueOutcome::BackInEdl);
                }
            }
            Seen::Nothing => edl_seen = 0,
        }
        // Offered after the first look, so a tablet that's already back never shows it.
        let prompt = offered.get_or_insert_with(|| ctx.offer(PromptKind::TabletStarting));
        match ctx.poll_answer(prompt) {
            Ok(None) => {}
            Ok(Some(_)) => {
                ctx.info("You can see the home screen: done");
                return outcome(RescueOutcome::HomeScreenSeen);
            }
            // The write is verified: stopping the wait is not a failed or cancelled rescue.
            Err(_) => {
                ctx.info("Stopped watching for the tablet");
                return outcome(RescueOutcome::NotSeen);
            }
        }
        if d.clock.now().saturating_sub(start) >= limit {
            ctx.info(
                "Didn't see the tablet over USB; it may still be starting, or USB debugging is off",
            );
            return outcome(RescueOutcome::NotSeen);
        }
        d.clock.sleep(BOOT_POLL);
    }
}
