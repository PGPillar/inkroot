//! Return to stock (spec §3.4): write this tablet's original boot image back over EDL, then
//! confirm Magisk is gone.

use super::common::{check_device, enter_edl, wait_boot, write_and_verify};
use super::root::{boot_part, no_stock_backup, stock_backup};
use super::{FlowCtx, FlowEnd};
use crate::device::MAGISK_CHECK;
use crate::ipc::{ErrorCode, FlowError, FlowResult, StepId};
use std::time::Duration;

const STEPS: u8 = 3;
const CHECK_TRIES: u32 = 3;
const CHECK_RETRY: Duration = Duration::from_secs(5);

pub const UNROOT_VERIFY_FAILED_MESSAGE: &str = "Your tablet restarted, but it still looks rooted. Your safety copy is unchanged, so you can try again.";

/// The Return to stock flow (3 steps).
pub fn run_unroot(ctx: &FlowCtx) -> Result<FlowResult, FlowEnd> {
    let d = ctx.deps();
    ctx.step(StepId::CheckDevice, 1, STEPS);
    let (serial, info, profile) = check_device(ctx, Some(true))?;

    ctx.step(StepId::EnterEdl, 2, STEPS);
    let Some((id, dir, m)) = stock_backup(ctx, &serial, &info, &profile)? else {
        return Err(no_stock_backup(format!(
            "no stock backup for {serial} ({}, slot {:?})",
            info.display_id, info.slot
        )));
    };
    let name = format!("{}.img", info.slot.boot());
    let Some(sha) = m.file(&name).map(|f| f.sha256.clone()) else {
        return Err(no_stock_backup(format!(
            "{name} is not listed in backup {id}"
        )));
    };
    // The original must still match its manifest before the tablet is sent into EDL.
    let file = d.library.verify_file(&dir, &m, &name)?;
    ctx.info(&format!(
        "Using the original safety copy {id}: {name} still matches"
    ));
    enter_edl(ctx, Some(&serial))?;
    write_and_verify(ctx, &boot_part(&profile, info.slot), &file, &sha, &dir)?;

    ctx.step(StepId::WaitBoot, 3, STEPS);
    wait_boot(ctx, &serial)?;
    confirm_not_rooted(ctx, &serial)?;
    ctx.info("Your tablet is back to its original software");
    Ok(FlowResult {
        backup: Some(m.to_summary(&id, &dir)),
        ..FlowResult::default()
    })
}

/// `magisk -c` must now fail. adb errors are retried a few times.
fn confirm_not_rooted(ctx: &FlowCtx, serial: &str) -> Result<(), FlowEnd> {
    let d = ctx.deps();
    ctx.info("Checking that root is gone");
    let mut last = String::new();
    for attempt in 1..=CHECK_TRIES {
        ctx.check_cancel()?;
        match d.adb.shell(serial, MAGISK_CHECK, d.timeouts.adb_shell) {
            Ok(o) if o.code != 0 => return Ok(()),
            Ok(o) => {
                return Err(FlowEnd::Failed(FlowError {
                    code: ErrorCode::VerifyFailed,
                    message: UNROOT_VERIFY_FAILED_MESSAGE.into(),
                    detail: format!("{MAGISK_CHECK} still succeeds: {}", o.stdout.trim()),
                    suggested_action: None,
                }));
            }
            Err(e) => {
                ctx.info(&format!(
                    "Couldn't ask the tablet (try {attempt} of {CHECK_TRIES}): {e}"
                ));
                last = e.to_string();
            }
        }
        if attempt < CHECK_TRIES {
            d.clock.sleep(CHECK_RETRY);
        }
    }
    Err(FlowEnd::Failed(FlowError {
        code: ErrorCode::VerifyFailed,
        message: "Your tablet restarted, but Inkroot couldn't check whether root is gone. Check the cable and look at the tablet's apps.".into(),
        detail: format!("{MAGISK_CHECK}: {last}"),
        suggested_action: Some(crate::ipc::SuggestedAction::Reconnect),
    }))
}
