//! Magisk payload extraction, patch command, and stock/hook checks.

use crate::image::{sha256_file, validate_boot_image};
use crate::ipc::{ErrorCode, FlowError, SuggestedAction};
use crate::profile::PatchEnv;
use crate::tools::Timeouts;
use crate::tools::adb::Adb;
use std::path::{Path, PathBuf};

pub const MAGISK_APK_SHA256: &str =
    "e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5";
pub const MGK_DIR: &str = "/data/local/tmp/mgk";
pub const MGK_RESET_CMD: &str = "rm -rf /data/local/tmp/mgk && mkdir -p /data/local/tmp/mgk";
pub const APK_PAYLOAD: [(&str, &str); 7] = [
    ("lib/arm64-v8a/libmagiskboot.so", "magiskboot"),
    ("lib/arm64-v8a/libmagiskinit.so", "magiskinit"),
    ("lib/arm64-v8a/libmagisk.so", "magisk"),
    ("lib/arm64-v8a/libinit-ld.so", "init-ld"),
    ("assets/boot_patch.sh", "boot_patch.sh"),
    ("assets/util_functions.sh", "util_functions.sh"),
    ("assets/stub.apk", "stub.apk"),
];
/// spec §3.2 step 2 — byte-for-byte; run with `adb shell '<PATCH_CMD>'` semantics (single shell arg)
pub const PATCH_CMD: &str = "cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh stock_boot.img && ./magiskboot unpack new-boot.img && ./magiskboot cpio ramdisk.cpio \"add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc\" && ./magiskboot cpio ramdisk.cpio \"ls overlay.d\" | grep init.bootsvc && ./magiskboot repack new-boot.img patched-boot.img";
pub const CHECK_DIR: &str = "/data/local/tmp/inkroot-check";
pub const STOCK_CHECK_CMD: &str = "cd /data/local/tmp/inkroot-check && chmod 755 magiskboot && ./magiskboot unpack boot.img && ./magiskboot cpio ramdisk.cpio test";
pub const HOOK_CHECK_CMD: &str = "cd /data/local/tmp/inkroot-check && chmod 755 magiskboot && ./magiskboot unpack boot.img && ./magiskboot cpio ramdisk.cpio \"ls overlay.d\" | grep init.bootsvc";
pub const CHECK_CLEANUP_CMD: &str = "rm -rf /data/local/tmp/inkroot-check";
pub const CHECK_PREP_CMD: &str =
    "rm -rf /data/local/tmp/inkroot-check && mkdir -p /data/local/tmp/inkroot-check";
pub const DEVICE_BOOTSVC: &str = "/data/local/tmp/bootsvc.sh";
pub const DEVICE_SERVICE: &str = "/data/local/tmp/service.sh";

fn internal(message: &str, detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::Internal,
        message: message.to_string(),
        detail,
        suggested_action: None,
    }
}

pub fn patch_command(env: &PatchEnv) -> Result<String, FlowError> {
    if !env.keepverity {
        return Err(internal(
            "Inkroot stopped because a safety setting was changed.",
            "KEEPVERITY must be true".into(),
        ));
    }
    if !env.keepforceencrypt {
        return Err(internal(
            "Inkroot stopped because a safety setting was changed.",
            "KEEPFORCEENCRYPT must be true".into(),
        ));
    }
    let flags = format!(
        "KEEPVERITY={} KEEPFORCEENCRYPT={} PATCHVBMETAFLAG={} RECOVERYMODE={}",
        env.keepverity, env.keepforceencrypt, env.patchvbmetaflag, env.recoverymode
    );
    Ok(format!(
        "cd {MGK_DIR} && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && {flags} sh boot_patch.sh stock_boot.img && ./magiskboot unpack new-boot.img && ./magiskboot cpio ramdisk.cpio \"add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc\" && ./magiskboot cpio ramdisk.cpio \"ls overlay.d\" | grep init.bootsvc && ./magiskboot repack new-boot.img patched-boot.img"
    ))
}

pub fn extract_payload(
    apk: &Path,
    expected_sha: &str,
    out: &Path,
) -> Result<Vec<(PathBuf, &'static str)>, FlowError> {
    if !apk.exists() {
        return Err(FlowError {
            code: ErrorCode::ToolMissing,
            message: "A file Inkroot needs is missing. Please reinstall Inkroot and try again."
                .into(),
            detail: format!("missing {}", apk.display()),
            suggested_action: None,
        });
    }
    let actual = sha256_file(apk).map_err(|e| {
        internal(
            "Inkroot couldn't read its bundled Magisk package.",
            e.to_string(),
        )
    })?;
    if !actual.eq_ignore_ascii_case(expected_sha) {
        return Err(internal(
            "Inkroot's bundled Magisk package failed its integrity check. Please re-download Inkroot.",
            format!("sha256 {actual}, expected {expected_sha}"),
        ));
    }
    let io_err = |e: &dyn std::fmt::Display| {
        internal(
            "Inkroot couldn't unpack its bundled Magisk package.",
            e.to_string(),
        )
    };
    let file = std::fs::File::open(apk).map_err(|e| io_err(&e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| io_err(&e))?;
    std::fs::create_dir_all(out).map_err(|e| io_err(&e))?;
    let mut result = Vec::with_capacity(APK_PAYLOAD.len());
    for (entry, name) in APK_PAYLOAD {
        let mut f = zip
            .by_name(entry)
            .map_err(|e| io_err(&format!("{entry}: {e}")))?;
        let dest = out.join(name);
        let mut w = std::fs::File::create(&dest).map_err(|e| io_err(&e))?;
        std::io::copy(&mut f, &mut w).map_err(|e| io_err(&e))?;
        result.push((dest, name));
    }
    Ok(result)
}

fn push_failed(detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::Internal,
        message: "Inkroot couldn't copy files to your tablet. Check the cable and try again."
            .into(),
        detail,
        suggested_action: Some(SuggestedAction::Reconnect),
    }
}

pub(crate) fn push_one(
    adb: &dyn Adb,
    serial: &str,
    local: &Path,
    remote: &str,
) -> Result<(), FlowError> {
    adb.push(serial, local, remote)
        .map_err(|e| push_failed(format!("push {} -> {remote}: {e}", local.display())))
}

pub fn push_patch_inputs(
    adb: &dyn Adb,
    serial: &str,
    payload: &[(PathBuf, &str)],
    stock_img: &Path,
    rc: &Path,
    bootsvc: &Path,
    service: &Path,
) -> Result<(), FlowError> {
    let t = Timeouts::default();
    let out = adb.shell(serial, MGK_RESET_CMD, t.adb_shell)?;
    if out.code != 0 {
        return Err(push_failed(format!(
            "reset exit {}: {}{}",
            out.code, out.stdout, out.stderr
        )));
    }
    for (local, name) in payload {
        push_one(adb, serial, local, &format!("{MGK_DIR}/{name}"))?;
    }
    push_one(adb, serial, rc, &format!("{MGK_DIR}/init.bootsvc.rc"))?;
    push_one(adb, serial, stock_img, &format!("{MGK_DIR}/stock_boot.img"))?;
    push_one(adb, serial, bootsvc, DEVICE_BOOTSVC)?;
    push_one(adb, serial, service, DEVICE_SERVICE)?;
    Ok(())
}

pub fn run_patch(
    adb: &dyn Adb,
    serial: &str,
    env: &PatchEnv,
    boot_size: u64,
    out: &Path,
    t: &Timeouts,
) -> Result<String, FlowError> {
    let cmd = patch_command(env)?;
    let res = adb.shell(serial, &cmd, t.patch)?;
    if res.code != 0 {
        return Err(FlowError {
            code: ErrorCode::PatchFailed,
            message:
                "Inkroot couldn't prepare the rooted startup file. Nothing on your tablet was changed."
                    .into(),
            detail: format!("{}{}", res.stdout, res.stderr),
            suggested_action: None,
        });
    }
    adb.pull(serial, &format!("{MGK_DIR}/patched-boot.img"), out)?;
    validate_boot_image(out, boot_size)?;
    sha256_file(out)
        .map_err(|e| internal("Inkroot couldn't read the prepared file.", e.to_string()))
}

/// Runs prep, pushes, `check_cmd`; cleanup always runs afterwards. Returns the exit code.
fn run_check(
    adb: &dyn Adb,
    serial: &str,
    magiskboot: &Path,
    img: &Path,
    check_cmd: &str,
    t: &Timeouts,
) -> Result<i32, FlowError> {
    let inner = || -> Result<i32, FlowError> {
        adb.shell(serial, CHECK_PREP_CMD, t.adb_shell)?;
        push_one(adb, serial, magiskboot, &format!("{CHECK_DIR}/magiskboot"))?;
        push_one(adb, serial, img, &format!("{CHECK_DIR}/boot.img"))?;
        Ok(adb.shell(serial, check_cmd, t.adb_shell)?.code)
    };
    let res = inner();
    let _ = adb.shell(serial, CHECK_CLEANUP_CMD, t.adb_shell);
    res
}

pub fn stock_check(
    adb: &dyn Adb,
    serial: &str,
    magiskboot: &Path,
    img: &Path,
    t: &Timeouts,
) -> Result<bool, FlowError> {
    match run_check(adb, serial, magiskboot, img, STOCK_CHECK_CMD, t)? {
        0 => Ok(true),
        1 | 2 => Ok(false),
        c => Err(internal(
            "Inkroot couldn't tell whether this startup file is the original.",
            format!("magiskboot cpio test exit {c}"),
        )),
    }
}

pub fn hook_present(
    adb: &dyn Adb,
    serial: &str,
    magiskboot: &Path,
    img: &Path,
    t: &Timeouts,
) -> Result<bool, FlowError> {
    Ok(run_check(adb, serial, magiskboot, img, HOOK_CHECK_CMD, t)? == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::builtin_profiles;
    use std::io::Write;

    fn note_max() -> crate::profile::DeviceProfile {
        builtin_profiles()[0].clone()
    }

    fn fixture_apk(dir: &Path, skip: Option<&str>) -> PathBuf {
        let p = dir.join("m.apk");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (entry, name) in APK_PAYLOAD {
            if skip == Some(entry) {
                continue;
            }
            z.start_file(entry, opts).unwrap();
            z.write_all(name.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        p
    }

    #[test]
    fn patch_cmd_constant_is_spec_sequence() {
        assert!(PATCH_CMD.contains("KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh stock_boot.img"));
        assert!(PATCH_CMD.contains("add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc"));
        assert!(PATCH_CMD.ends_with("./magiskboot repack new-boot.img patched-boot.img"));
        assert_eq!(
            patch_command(&note_max().magisk.patch_env).unwrap(),
            PATCH_CMD
        );
    }

    #[test]
    fn patch_command_refuses_verity_off() {
        let mut env = note_max().magisk.patch_env;
        env.keepverity = false;
        assert_eq!(patch_command(&env).unwrap_err().code, ErrorCode::Internal);
        let mut env = note_max().magisk.patch_env;
        env.keepforceencrypt = false;
        assert_eq!(patch_command(&env).unwrap_err().code, ErrorCode::Internal);
    }

    #[test]
    fn extract_payload_returns_seven_in_order() {
        let d = tempfile::tempdir().unwrap();
        let apk = fixture_apk(d.path(), None);
        let sha = sha256_file(&apk).unwrap().to_uppercase();
        let out = d.path().join("out");
        let got = extract_payload(&apk, &sha, &out).unwrap();
        let names: Vec<&str> = got.iter().map(|(_, n)| *n).collect();
        let want: Vec<&str> = APK_PAYLOAD.iter().map(|(_, n)| *n).collect();
        assert_eq!(names, want);
        assert_eq!(std::fs::read(&got[0].0).unwrap(), b"magiskboot");
    }

    #[test]
    fn extract_payload_rejects_wrong_sha() {
        let d = tempfile::tempdir().unwrap();
        let apk = fixture_apk(d.path(), None);
        let e = extract_payload(&apk, MAGISK_APK_SHA256, &d.path().join("out")).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(e.message.contains("failed its integrity check"));
        assert!(!d.path().join("out").exists());
    }

    #[test]
    fn extract_payload_missing_apk() {
        let d = tempfile::tempdir().unwrap();
        let e = extract_payload(&d.path().join("nope.apk"), "x", d.path()).unwrap_err();
        assert_eq!(e.code, ErrorCode::ToolMissing);
    }

    #[test]
    fn extract_payload_missing_entry() {
        let d = tempfile::tempdir().unwrap();
        let apk = fixture_apk(d.path(), Some("assets/stub.apk"));
        let sha = sha256_file(&apk).unwrap();
        let e = extract_payload(&apk, &sha, &d.path().join("out")).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
    }
}
