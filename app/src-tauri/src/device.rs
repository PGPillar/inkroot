//! Device probing: parse adb output into a `DeviceStatus`.

use crate::ipc::{DeviceInfo, DeviceStatus, Gate, Slot};

use crate::profile::{DeviceProfile, DeviceProps, evaluate_gate, match_profile};
use crate::tools::Timeouts;
use crate::tools::adb::{Adb, AdbState};
use crate::tools::usb::UsbProbe;
use std::collections::BTreeMap;

pub const BOOTSVC_LOG: &str = "cat /data/local/tmp/bootsvc.log";
pub const MAGISK_CHECK: &str = "/debug_ramdisk/magisk -c";
const SERVICE_PGREP: &str = "pgrep -f /data/adb/service.sh";

/// Parses `getprop` output (`[key]: [value]` lines).
pub fn parse_getprop(out: &str) -> BTreeMap<String, String> {
    out.lines()
        .filter_map(|l| {
            let l = l.trim();
            let (k, v) = l.split_once("]: [")?;
            let k = k.strip_prefix('[')?;
            let v = v.strip_suffix(']')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

pub fn props_from(map: &BTreeMap<String, String>, serial: &str) -> DeviceProps {
    let g = |k: &str| map.get(k).cloned().unwrap_or_default();
    DeviceProps {
        serial: serial.to_string(),
        model: g("ro.product.model"),
        device: g("ro.product.device"),
        manufacturer: g("ro.product.manufacturer"),
        build_id: g("ro.build.id"),
        display_id: g("ro.build.display.id"),
        incremental: g("ro.build.version.incremental"),
        fingerprint: g("ro.build.fingerprint"),
        slot_suffix: g("ro.boot.slot_suffix"),
    }
}

pub fn parse_battery(dumpsys: &str) -> Option<u8> {
    dumpsys.lines().find_map(|l| {
        l.trim()
            .strip_prefix("level:")
            .and_then(|v| v.trim().parse().ok())
    })
}

/// `"30.7:MAGISK:R"` -> `"30.7"`.
pub fn parse_magisk_version(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    let (ver, rest) = line.split_once(':')?;
    (rest.starts_with("MAGISK") && !ver.is_empty() && ver.chars().any(|c| c.is_ascii_digit()))
        .then(|| ver.to_string())
}

pub fn bootsvc_boot_lines(log: &str) -> usize {
    log.lines().filter(|l| l.starts_with("boot ")).count()
}

/// The last `boot ` line ran as root in the Magisk context.
pub fn bootsvc_ok(log: &str) -> bool {
    log.lines()
        .rfind(|l| l.starts_with("boot "))
        .is_some_and(|l| l.contains("uid=0") && l.contains("u:r:magisk:s0"))
}

pub fn bootsvc_line_count(adb: &dyn Adb, serial: &str, t: &Timeouts) -> usize {
    try_bootsvc_line_count(adb, serial, t).unwrap_or(0)
}

/// Like `bootsvc_line_count`, but only a missing log counts as 0. An adb/tool error, or `cat`
/// failing for any other reason, is an `Err` (with a technical detail).
pub fn try_bootsvc_line_count(adb: &dyn Adb, serial: &str, t: &Timeouts) -> Result<usize, String> {
    match adb.shell(serial, BOOTSVC_LOG, t.adb_shell) {
        Ok(o) if o.code == 0 => Ok(bootsvc_boot_lines(&o.stdout)),
        Ok(o) if o.stderr.contains("No such file") || o.stdout.contains("No such file") => Ok(0),
        Ok(o) => Err(format!(
            "{BOOTSVC_LOG}: exit {}: {}{}",
            o.code, o.stdout, o.stderr
        )),
        Err(e) => Err(format!("{BOOTSVC_LOG}: {e}")),
    }
}

fn read_props(adb: &dyn Adb, serial: &str, t: &Timeouts) -> Option<DeviceProps> {
    // Unlogged: getprop prints ~1000 lines; flows log the parsed fields instead.
    let o = adb.shell_unlogged(serial, "getprop", t.adb_shell).ok()?;
    (o.code == 0).then(|| props_from(&parse_getprop(&o.stdout), serial))
}

fn build_info(
    adb: &dyn Adb,
    props: &DeviceProps,
    profile: Option<&DeviceProfile>,
    t: &Timeouts,
) -> (DeviceStatus, String) {
    let serial = props.serial.clone();
    let battery = adb
        .shell(&serial, "dumpsys battery", t.adb_shell)
        .ok()
        .and_then(|o| parse_battery(&o.stdout))
        .unwrap_or(0);
    let magisk = adb
        .shell(&serial, MAGISK_CHECK, t.adb_shell)
        .ok()
        .filter(|o| o.code == 0);
    let rooted = magisk.is_some();
    let magisk_version = magisk.and_then(|o| parse_magisk_version(&o.stdout));
    let boot_service_running = rooted && {
        let log_ok = adb
            .shell(&serial, BOOTSVC_LOG, t.adb_shell)
            .ok()
            .is_some_and(|o| o.code == 0 && bootsvc_ok(&o.stdout));
        log_ok
            && adb
                .shell(&serial, SERVICE_PGREP, t.adb_shell)
                .ok()
                .is_some_and(|o| o.code == 0)
    };
    let firmware_label = profile
        .and_then(|p| p.firmware_entry(props))
        .map(|f| f.label.clone())
        .unwrap_or_else(|| props.display_id.clone());
    let info = DeviceInfo {
        serial: serial.clone(),
        model: props.model.clone(),
        profile_id: profile.map(|p| p.id.clone()),
        display_name: profile
            .map(|p| p.display_name.clone())
            .unwrap_or_else(|| props.model.clone()),
        build_id: props.build_id.clone(),
        display_id: props.display_id.clone(),
        incremental: props.incremental.clone(),
        firmware_label,
        slot: Slot::from_suffix(&props.slot_suffix).unwrap_or(Slot::A),
        battery,
        rooted,
        magisk_version,
        boot_service_running,
    };
    // A missing or unparseable slot suffix is not safe to act on; block like unknown firmware.
    let gate = evaluate_gate(profile, props, battery).or_else(|| {
        Slot::from_suffix(&props.slot_suffix)
            .is_none()
            .then_some(Gate::UnknownFirmware)
    });
    (DeviceStatus::Adb { info, gate }, serial)
}

/// Works out what is plugged in. Returns the status and the serial of the chosen adb device.
pub fn probe(
    adb: &dyn Adb,
    usb: &dyn UsbProbe,
    profiles: &[DeviceProfile],
    t: &Timeouts,
) -> (DeviceStatus, Option<String>) {
    if usb.edl_device().is_some() {
        return (DeviceStatus::Edl, None);
    }
    let devices = adb.devices().unwrap_or_default();
    let mut fallback: Option<DeviceProps> = None;
    for d in devices.iter().filter(|d| d.state == AdbState::Device) {
        let Some(props) = read_props(adb, &d.serial, t) else {
            continue;
        };
        if let Some(p) = match_profile(profiles, &props) {
            let (st, serial) = build_info(adb, &props, Some(p), t);
            return (st, Some(serial));
        }
        fallback.get_or_insert(props);
    }
    // No authorized device matches a profile. An unauthorized one may be the tablet waiting
    // for "Allow", which matters more than an unrelated authorized device (a phone).
    if devices.iter().any(|d| d.state == AdbState::Unauthorized) {
        return (DeviceStatus::Unauthorized, None);
    }
    if let Some(props) = fallback {
        let (st, serial) = build_info(adb, &props, None, t);
        return (st, Some(serial));
    }
    (DeviceStatus::None, None)
}
