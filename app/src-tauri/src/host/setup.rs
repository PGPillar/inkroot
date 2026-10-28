//! One-time host setup so the computer can talk to the tablet in EDL mode:
//! the WinUSB driver on Windows, a udev rule on Linux, nothing on macOS.

use crate::ipc::FlowError;
#[cfg(any(windows, target_os = "linux"))]
use crate::ipc::{ErrorCode, SuggestedAction};
use crate::tools::ToolPaths;
use crate::tools::usb::EdlUsbInfo;
use std::path::Path;
use std::sync::Arc;

pub const UDEV_RULE: &str = "SUBSYSTEM==\"usb\", ATTRS{idVendor}==\"05c6\", ATTRS{idProduct}==\"9008\", MODE=\"0666\", TAG+=\"uaccess\"\n";

pub trait HostSetup: Send + Sync {
    fn needs_edl_driver(&self, usb: &EdlUsbInfo) -> bool;
    /// Before the tablet is sent into EDL: true when the EDL driver is known to be missing, so
    /// it can be installed first and the tablet's first hello isn't lost on a driverless
    /// device. Only Windows ever says yes; `false` when it can't tell.
    fn needs_edl_driver_before_edl(&self) -> bool {
        false
    }
    fn install_edl_driver(&self) -> Result<(), FlowError>;
    fn needs_udev(&self) -> bool;
    fn install_udev(&self) -> Result<(), FlowError>;
}

/// The display name wdi-simple gives the EDL driver.
pub const EDL_DRIVER_NAME: &str = "Qualcomm HS-USB QDLoader 9008";

/// wdi-simple's argv for the WinUSB EDL driver. `dest` is a fresh writable folder for the
/// driver files: without `--dest` wdi-simple fails to extract them ("Could not allocate
/// resource", exit -11).
pub fn wdi_args(dest: &Path) -> Vec<String> {
    vec![
        "--vid".into(),
        "0x05C6".into(),
        "--pid".into(),
        "0x9008".into(),
        "--type".into(),
        "0".into(),
        "--name".into(),
        EDL_DRIVER_NAME.into(),
        "--dest".into(),
        dest.display().to_string(),
    ]
}

/// Joins argv into one Windows command line (ShellExecuteExW `lpParameters`), quoting with the
/// MSVCRT rules: an argument with spaces, tabs or quotes is wrapped in quotes, inner quotes are
/// escaped and the backslashes before them (or before the closing quote) doubled.
pub fn windows_command_line(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if !a.is_empty() && !a.contains([' ', '\t', '"']) {
                return a.clone();
            }
            let mut out = String::from("\"");
            let mut slashes = 0usize;
            for c in a.chars() {
                match c {
                    '\\' => slashes += 1,
                    '"' => {
                        out.push_str(&"\\".repeat(slashes * 2 + 1));
                        out.push('"');
                        slashes = 0;
                    }
                    _ => {
                        out.push_str(&"\\".repeat(slashes));
                        out.push(c);
                        slashes = 0;
                    }
                }
            }
            out.push_str(&"\\".repeat(slashes * 2));
            out.push('"');
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn driver_is_winusb(driver: Option<&str>) -> bool {
    driver.is_some_and(|d| d.eq_ignore_ascii_case("winusb"))
}

/// Where `install_udev` puts Inkroot's own rule.
pub const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/51-inkroot-edl.rules";

/// The udev rule folders, in udev's lookup order.
pub const UDEV_RULE_DIRS: [&str; 4] = [
    "/etc/udev/rules.d",
    "/run/udev/rules.d",
    "/usr/lib/udev/rules.d",
    "/lib/udev/rules.d",
];

/// The `key{attr}<op>"value"` pairs of one udev rule line, keys and attrs as written.
fn udev_pairs(line: &str) -> Vec<(String, String, String)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r#"([A-Za-z_]+(?:\{[^}]*\})?)\s*(==|!=|\+=|-=|:=|=)\s*"([^"]*)""#)
            .expect("valid regex")
    });
    re.captures_iter(line)
        .map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string()))
        .collect()
}

/// True when one udev rule line matches the EDL device (05c6:9008) and gives a user in
/// `groups` read/write access to its node: `TAG+="uaccess"` (the logged-in user), a `MODE`
/// with read/write for others, or a `GROUP` the user is in (with a `MODE` giving that group
/// read/write, or none: udev then uses 0660). Rules that only set environment variables, like
/// ModemManager's `ID_MM_DEVICE_IGNORE`, grant nothing.
pub fn udev_line_grants(line: &str, groups: &[String]) -> bool {
    let line = line.trim();
    if line.starts_with('#') {
        return false;
    }
    let pairs = udev_pairs(line);
    let matches = |attr: &str, want: &str| {
        pairs.iter().any(|(k, op, v)| {
            op == "==" && k.eq_ignore_ascii_case(attr) && v.eq_ignore_ascii_case(want)
        })
    };
    let vendor = matches("ATTRS{idVendor}", "05c6") || matches("ATTR{idVendor}", "05c6");
    let product = matches("ATTRS{idProduct}", "9008") || matches("ATTR{idProduct}", "9008");
    if !(vendor && product) {
        return false;
    }
    let assigned = |key: &str| {
        pairs
            .iter()
            .filter(|(k, op, _)| k == key && matches!(op.as_str(), "=" | ":=" | "+="))
            .map(|(_, _, v)| v.as_str())
            .next_back()
    };
    if pairs
        .iter()
        .any(|(k, op, v)| k == "TAG" && matches!(op.as_str(), "+=" | "=" | ":=") && v == "uaccess")
    {
        return true;
    }
    let mode = assigned("MODE").and_then(|m| u32::from_str_radix(m, 8).ok());
    let rw = |bits: u32| bits & 0o6 == 0o6;
    if mode.is_some_and(rw) {
        return true;
    }
    let in_group = assigned("GROUP").is_some_and(|g| groups.iter().any(|x| x == g));
    in_group && mode.is_none_or(|m| rw(m >> 3))
}

/// True when Inkroot's own rule is installed, or a rule in `dirs` grants a user in `groups`
/// access to the EDL device (see `udev_line_grants`). Only `*.rules` files count, as for udev.
pub fn udev_rules_grant(own_rule: &Path, dirs: &[&Path], groups: &[String]) -> bool {
    own_rule.is_file()
        || dirs.iter().any(|d| {
            std::fs::read_dir(d).is_ok_and(|rd| {
                rd.flatten().any(|e| {
                    e.path().extension().is_some_and(|x| x == "rules")
                        && std::fs::read(e.path()).is_ok_and(|b| {
                            String::from_utf8_lossy(&b)
                                .lines()
                                .any(|l| udev_line_grants(l, groups))
                        })
                })
            })
        })
}

/// The device node of the EDL device (05c6:9008) when it is plugged in: found under
/// `sysfs/bus/usb/devices`, its node is `dev/bus/usb/BBB/DDD` from `busnum`/`devnum`.
pub fn edl_device_node(sysfs: &Path, dev: &Path) -> Option<std::path::PathBuf> {
    let read = |p: &Path, f: &str| {
        std::fs::read_to_string(p.join(f))
            .ok()
            .map(|s| s.trim().to_string())
    };
    std::fs::read_dir(sysfs.join("bus/usb/devices"))
        .ok()?
        .flatten()
        .find_map(|e| {
            let p = e.path();
            let vid = read(&p, "idVendor")?;
            let pid = read(&p, "idProduct")?;
            if !(vid.eq_ignore_ascii_case("05c6") && pid.eq_ignore_ascii_case("9008")) {
                return None;
            }
            let bus: u32 = read(&p, "busnum")?.parse().ok()?;
            let num: u32 = read(&p, "devnum")?.parse().ok()?;
            Some(dev.join(format!("bus/usb/{bus:03}/{num:03}")))
        })
}

/// The names of `gids` in an `/etc/group`-format text.
pub fn group_names(etc_group: &str, gids: &[u32]) -> Vec<String> {
    etc_group
        .lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            let gid: u32 = f.nth(1)?.trim().parse().ok()?;
            gids.contains(&gid).then(|| name.to_string())
        })
        .collect()
}

/// Whether Inkroot must ask for USB access first. With the EDL device plugged in, that's
/// whether this user can open its node (`can_open`); before it is (the tablet is still in
/// Android), whether any rule grants access ahead of time.
pub fn udev_access_needed(
    node: Option<&Path>,
    can_open: &dyn Fn(&Path) -> bool,
    rules_grant: &dyn Fn() -> bool,
) -> bool {
    match node {
        Some(n) => !can_open(n),
        None => !rules_grant(),
    }
}

/// True when this process may open `path` for reading and writing.
#[cfg(unix)]
pub fn can_open_rw(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is a valid NUL-terminated path for the duration of the call.
    unsafe { libc::access(c.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

/// This user's group names (effective and supplementary), from `/etc/group`.
#[cfg(target_os = "linux")]
fn user_groups() -> Vec<String> {
    // SAFETY: getgroups with a zero size returns the count; then fills a buffer that large.
    let mut gids: Vec<libc::gid_t> = unsafe {
        let n = libc::getgroups(0, std::ptr::null_mut());
        let mut v: Vec<libc::gid_t> = vec![0; usize::try_from(n).unwrap_or(0)];
        let got = libc::getgroups(n.max(0), v.as_mut_ptr());
        v.truncate(usize::try_from(got).unwrap_or(0));
        v
    };
    // SAFETY: getegid has no preconditions.
    gids.push(unsafe { libc::getegid() });
    group_names(
        &std::fs::read_to_string("/etc/group").unwrap_or_default(),
        &gids,
    )
}

/// Linux: the rule couldn't be installed (the password prompt was dismissed, or pkexec is
/// missing).
#[cfg(target_os = "linux")]
fn udev_failed(detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::UsbPermission,
        message: UDEV_FAILED_MESSAGE.into(),
        detail,
        suggested_action: Some(SuggestedAction::Retry),
    }
}

pub const UDEV_FAILED_MESSAGE: &str = "Inkroot didn't get permission to use USB. Try again, and enter your password when your computer asks. Nothing was changed on your tablet.";

#[cfg(windows)]
fn driver_missing(detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::DriverMissing,
        message:
            "Inkroot needs permission to install a small USB driver so it can talk to your tablet."
                .into(),
        detail,
        suggested_action: Some(SuggestedAction::Retry),
    }
}

pub fn platform_host_setup(tools: &ToolPaths) -> Arc<dyn HostSetup> {
    #[cfg(windows)]
    {
        Arc::new(WindowsSetup::new(tools))
    }
    #[cfg(not(windows))]
    {
        let _ = tools;
        Arc::new(UnixSetup)
    }
}

#[cfg(windows)]
pub struct WindowsSetup {
    wdi: std::path::PathBuf,
    /// The driver was installed during this run of Inkroot.
    installed: std::sync::atomic::AtomicBool,
}

#[cfg(windows)]
impl WindowsSetup {
    pub fn new(tools: &ToolPaths) -> Self {
        Self {
            wdi: tools.wdi_simple(),
            installed: Default::default(),
        }
    }
}

#[cfg(windows)]
impl HostSetup for WindowsSetup {
    fn needs_edl_driver(&self, usb: &EdlUsbInfo) -> bool {
        !driver_is_winusb(usb.driver.as_deref())
    }

    fn needs_edl_driver_before_edl(&self) -> bool {
        if self.installed.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
        // Windows remembers every device node it has seen, with its bound service: WinUSB on
        // any of them means it will bind WinUSB again when the tablet enters EDL.
        crate::tools::usb::win::edl_nodes(false)
            .is_some_and(|nodes| !crate::tools::usb::winusb_bound_before(&nodes))
    }

    fn install_edl_driver(&self) -> Result<(), FlowError> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, INFINITE, WaitForSingleObject,
        };
        use windows_sys::Win32::UI::Shell::{
            SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
        };

        fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
            s.encode_wide().chain(std::iter::once(0)).collect()
        }

        // A fresh, writable folder for the driver files (removed when this returns; by then
        // the driver is in the Windows driver store).
        let dest = tempfile::Builder::new()
            .prefix("inkroot_usb_driver")
            .tempdir()
            .map_err(|e| driver_missing(format!("temp folder for the driver: {e}")))?;
        let file = wide(self.wdi.as_os_str());
        let verb = wide(std::ffi::OsStr::new("runas"));
        let params = wide(std::ffi::OsStr::new(&windows_command_line(&wdi_args(
            dest.path(),
        ))));
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0; // SW_HIDE
        // SAFETY: `info` and the wide strings it points to outlive the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 || info.hProcess.is_null() {
            return Err(driver_missing(
                "ShellExecuteExW failed or the elevation prompt was declined".into(),
            ));
        }
        let mut code: u32 = 1;
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            let ok = GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            if ok == 0 {
                code = 1;
            }
        }
        if code != 0 {
            return Err(driver_missing(format!(
                "wdi-simple exited with code {code}"
            )));
        }
        self.installed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn needs_udev(&self) -> bool {
        false
    }
    fn install_udev(&self) -> Result<(), FlowError> {
        Ok(())
    }
}

#[cfg(not(windows))]
pub struct UnixSetup;

#[cfg(target_os = "linux")]
impl HostSetup for UnixSetup {
    fn needs_edl_driver(&self, _usb: &EdlUsbInfo) -> bool {
        false
    }
    fn install_edl_driver(&self) -> Result<(), FlowError> {
        Ok(())
    }

    fn needs_udev(&self) -> bool {
        let node = edl_device_node(Path::new("/sys"), Path::new("/dev"));
        udev_access_needed(node.as_deref(), &can_open_rw, &|| {
            let dirs: Vec<&Path> = UDEV_RULE_DIRS.iter().map(Path::new).collect();
            udev_rules_grant(Path::new(UDEV_RULE_PATH), &dirs, &user_groups())
        })
    }

    fn install_udev(&self) -> Result<(), FlowError> {
        use std::io::Write;
        let mut tmp =
            tempfile::NamedTempFile::new().map_err(|e| udev_failed(format!("temp file: {e}")))?;
        tmp.write_all(UDEV_RULE.as_bytes())
            .and_then(|_| tmp.flush())
            .map_err(|e| udev_failed(format!("write rule: {e}")))?;
        let script = format!(
            "install -m 644 '{}' {UDEV_RULE_PATH} && udevadm control --reload-rules && udevadm trigger --subsystem-match=usb --action=change",
            tmp.path().display()
        );
        let out = std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|e| udev_failed(format!("pkexec: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(udev_failed(format!(
                "pkexec exited with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )))
        }
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
impl HostSetup for UnixSetup {
    fn needs_edl_driver(&self, _usb: &EdlUsbInfo) -> bool {
        false
    }
    fn install_edl_driver(&self) -> Result<(), FlowError> {
        Ok(())
    }
    fn needs_udev(&self) -> bool {
        false
    }
    fn install_udev(&self) -> Result<(), FlowError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::{Call, FakeHost, FakeWorld};
    use crate::tools::usb::EdlUsbInfo;

    #[test]
    fn windows_driver_needed_unless_winusb() {
        assert!(!driver_is_winusb(None));
        assert!(!driver_is_winusb(Some("usbser")));
        assert!(driver_is_winusb(Some("WinUSB")));
        assert!(driver_is_winusb(Some("winusb")));
    }

    #[test]
    fn wdi_argv_always_has_a_dest_folder() {
        let dest = Path::new(r"C:\Users\Ann Lee\AppData\Local\Temp\inkroot_usb_driverX1");
        assert_eq!(
            wdi_args(dest),
            [
                "--vid",
                "0x05C6",
                "--pid",
                "0x9008",
                "--type",
                "0",
                "--name",
                "Qualcomm HS-USB QDLoader 9008",
                "--dest",
                r"C:\Users\Ann Lee\AppData\Local\Temp\inkroot_usb_driverX1",
            ]
        );
        assert_eq!(
            windows_command_line(&wdi_args(dest)),
            r#"--vid 0x05C6 --pid 0x9008 --type 0 --name "Qualcomm HS-USB QDLoader 9008" --dest "C:\Users\Ann Lee\AppData\Local\Temp\inkroot_usb_driverX1""#
        );
    }

    #[test]
    fn command_line_quoting() {
        let q =
            |v: &[&str]| windows_command_line(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(q(&["a", "b c", ""]), r#"a "b c" """#);
        assert_eq!(q(&[r"C:\a b\"]), r#""C:\a b\\""#);
        assert_eq!(q(&[r#"say "hi""#]), r#""say \"hi\"""#);
        assert_eq!(q(&[r"C:\plain\dir"]), r"C:\plain\dir");
    }

    #[test]
    fn udev_rule_text_exact() {
        assert_eq!(
            UDEV_RULE,
            "SUBSYSTEM==\"usb\", ATTRS{idVendor}==\"05c6\", ATTRS{idProduct}==\"9008\", MODE=\"0666\", TAG+=\"uaccess\"\n"
        );
    }

    const MM_RULE: &str =
        r#"ATTRS{idVendor}=="05c6", ATTRS{idProduct}=="9008", ENV{ID_MM_DEVICE_IGNORE}="1""#;

    fn groups(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn modem_manager_rule_grants_nothing() {
        assert!(!udev_line_grants(MM_RULE, &groups(&["plugdev", "dialout"])));
    }

    #[test]
    fn our_rule_grants() {
        assert!(udev_line_grants(UDEV_RULE, &[]));
    }

    #[test]
    fn rule_line_cases() {
        let none: Vec<String> = vec![];
        let g = groups(&["henry", "plugdev"]);
        let l = |extra: &str| {
            format!(
                r#"SUBSYSTEM=="usb", ATTRS{{idVendor}}=="05C6", ATTRS{{idProduct}}=="9008", {extra}"#
            )
        };
        // uaccess: the logged-in user gets an ACL.
        assert!(udev_line_grants(&l(r#"TAG+="uaccess""#), &none));
        // MODE: other read/write, or not.
        assert!(udev_line_grants(&l(r#"MODE="0666""#), &none));
        assert!(udev_line_grants(&l(r#"MODE:="0666""#), &none));
        assert!(!udev_line_grants(&l(r#"MODE="0664""#), &none));
        assert!(!udev_line_grants(&l(r#"MODE="0660""#), &none));
        // GROUP: only for a group the user is in, and with group read/write.
        assert!(udev_line_grants(&l(r#"MODE="0660", GROUP="plugdev""#), &g));
        assert!(udev_line_grants(&l(r#"GROUP="plugdev""#), &g));
        assert!(!udev_line_grants(
            &l(r#"MODE="0660", GROUP="plugdev""#),
            &none
        ));
        assert!(!udev_line_grants(&l(r#"MODE="0640", GROUP="plugdev""#), &g));
        assert!(!udev_line_grants(&l(r#"GROUP="adbusers""#), &g));
        // ENV-only, or a different device, or commented out: nothing.
        assert!(!udev_line_grants(
            &l(r#"ENV{ID_MM_DEVICE_IGNORE}="1""#),
            &none
        ));
        assert!(!udev_line_grants(
            r#"SUBSYSTEM=="usb", ATTRS{idVendor}=="05c6", ATTRS{idProduct}=="9025", MODE="0666""#,
            &none
        ));
        assert!(!udev_line_grants(
            r#"SUBSYSTEM=="usb", ATTRS{idVendor}=="18d1", ATTRS{idProduct}=="9008", MODE="0666""#,
            &none
        ));
        assert!(!udev_line_grants(
            &format!("# {}", l(r#"MODE="0666""#)),
            &none
        ));
        // A comparison isn't an assignment.
        assert!(!udev_line_grants(&l(r#"MODE=="0666""#), &none));
    }

    #[test]
    fn rules_grant_scans_dirs() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let own = a.path().join("51-inkroot-edl.rules");
        let dirs = [a.path(), b.path()];
        assert!(!udev_rules_grant(&own, &dirs, &[]));
        // Debian's ModemManager blacklist mentions 9008 but grants nothing.
        std::fs::write(
            b.path().join("77-mm-qdl-device-blacklist.rules"),
            format!("# ModemManager\n{MM_RULE}\n"),
        )
        .unwrap();
        assert!(!udev_rules_grant(&own, &dirs, &[]));
        // A granting line in a non-.rules file doesn't count.
        std::fs::write(b.path().join("51-edl.rules.bak"), UDEV_RULE).unwrap();
        assert!(!udev_rules_grant(&own, &dirs, &[]));
        std::fs::write(b.path().join("51-edl.rules"), UDEV_RULE).unwrap();
        assert!(udev_rules_grant(&own, &dirs, &[]));
        // Inkroot's own rule file is enough by itself.
        let c = tempfile::tempdir().unwrap();
        std::fs::write(c.path().join("51-inkroot-edl.rules"), "").unwrap();
        assert!(udev_rules_grant(
            &c.path().join("51-inkroot-edl.rules"),
            &[Path::new("/nonexistent-dir-x")],
            &[]
        ));
    }

    fn fake_usb_device(sysfs: &Path, name: &str, vid: &str, pid: &str, bus: &str, dev: &str) {
        let d = sysfs.join("bus/usb/devices").join(name);
        std::fs::create_dir_all(&d).unwrap();
        for (f, v) in [
            ("idVendor", vid),
            ("idProduct", pid),
            ("busnum", bus),
            ("devnum", dev),
        ] {
            std::fs::write(d.join(f), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn edl_node_from_sysfs() {
        let sys = tempfile::tempdir().unwrap();
        let dev = Path::new("/dev");
        assert_eq!(edl_device_node(sys.path(), dev), None);
        fake_usb_device(sys.path(), "1-1", "18d1", "4ee7", "1", "5");
        // An interface folder has no idVendor.
        std::fs::create_dir_all(sys.path().join("bus/usb/devices/1-1:1.0")).unwrap();
        assert_eq!(edl_device_node(sys.path(), dev), None);
        fake_usb_device(sys.path(), "3-2", "05C6", "9008", "3", "17");
        assert_eq!(
            edl_device_node(sys.path(), dev),
            Some(std::path::PathBuf::from("/dev/bus/usb/003/017"))
        );
    }

    #[test]
    fn group_names_from_etc_group() {
        let text = "root:x:0:\nplugdev:x:46:henry\nhenry:x:1000:\nbroken\n";
        assert_eq!(group_names(text, &[1000, 46]), ["plugdev", "henry"]);
        assert!(group_names(text, &[7]).is_empty());
    }

    #[test]
    fn access_needed_decision() {
        let node = Path::new("/dev/bus/usb/003/017");
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        // Plugged in: only whether the node opens matters, whatever the rules say.
        assert!(udev_access_needed(Some(node), &no, &|| true));
        assert!(!udev_access_needed(Some(node), &yes, &|| false));
        // Not plugged in yet: the rules decide.
        assert!(udev_access_needed(None, &yes, &|| false));
        assert!(!udev_access_needed(None, &no, &|| true));
    }

    #[cfg(unix)]
    #[test]
    fn can_open_rw_checks_access() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("node");
        std::fs::write(&f, "").unwrap();
        assert!(can_open_rw(&f));
        assert!(!can_open_rw(&d.path().join("missing")));
        // root ignores mode bits.
        if unsafe { libc::geteuid() } != 0 {
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o444)).unwrap();
            assert!(!can_open_rw(&f));
        }
    }

    #[test]
    fn fake_host_records_and_clears() {
        let w = FakeWorld::default();
        let h = FakeHost::new(w.clone());
        h.set_needs_driver(true);
        h.set_needs_udev(true);
        let usb = EdlUsbInfo { driver: None };
        assert!(h.needs_edl_driver(&usb));
        assert!(h.needs_udev());
        h.install_edl_driver().unwrap();
        h.install_udev().unwrap();
        assert!(!h.needs_edl_driver(&usb));
        assert!(!h.needs_udev());
        assert_eq!(w.calls(), vec![Call::DriverInstall, Call::UdevInstall]);
    }

    #[test]
    fn fake_awake_counts() {
        use crate::fakes::FakeAwake;
        use crate::host::keep_awake::KeepAwake;
        let a = FakeAwake::new();
        let g = a.hold("x");
        assert_eq!((a.holds(), a.active()), (1, 1));
        drop(g);
        assert_eq!((a.holds(), a.active()), (1, 0));
    }

    #[test]
    fn platform_setup_constructs() {
        let t = crate::tools::ToolPaths {
            root: std::path::PathBuf::from("x"),
        };
        let h = platform_host_setup(&t);
        let _ = h.needs_udev();
    }
}
