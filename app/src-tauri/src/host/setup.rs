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
    fn install_edl_driver(&self) -> Result<(), FlowError>;
    fn needs_udev(&self) -> bool;
    fn install_udev(&self) -> Result<(), FlowError>;
}

pub fn driver_is_winusb(driver: Option<&str>) -> bool {
    driver.is_some_and(|d| d.eq_ignore_ascii_case("winusb"))
}

/// True when any readable file in `dirs` mentions the EDL product id.
pub fn udev_rule_present(dirs: &[&Path]) -> bool {
    dirs.iter().any(|d| {
        std::fs::read_dir(d).is_ok_and(|rd| {
            rd.flatten().any(|e| {
                std::fs::read(e.path()).is_ok_and(|b| String::from_utf8_lossy(&b).contains("9008"))
            })
        })
    })
}

#[cfg(any(windows, target_os = "linux"))]
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
        Arc::new(WindowsSetup {
            wdi: tools.wdi_simple(),
        })
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
}

#[cfg(windows)]
impl WindowsSetup {
    pub fn new(tools: &ToolPaths) -> Self {
        Self {
            wdi: tools.wdi_simple(),
        }
    }
}

#[cfg(windows)]
impl HostSetup for WindowsSetup {
    fn needs_edl_driver(&self, usb: &EdlUsbInfo) -> bool {
        !driver_is_winusb(usb.driver.as_deref())
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

        let file = wide(self.wdi.as_os_str());
        let verb = wide(std::ffi::OsStr::new("runas"));
        let params = wide(std::ffi::OsStr::new(
            "--vid 0x05C6 --pid 0x9008 --type 0 --name \"Qualcomm HS-USB QDLoader 9008\"",
        ));
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
        !udev_rule_present(&[
            Path::new("/etc/udev/rules.d"),
            Path::new("/usr/lib/udev/rules.d"),
        ])
    }

    fn install_udev(&self) -> Result<(), FlowError> {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new()
            .map_err(|e| driver_missing(format!("temp file: {e}")))?;
        tmp.write_all(UDEV_RULE.as_bytes())
            .and_then(|_| tmp.flush())
            .map_err(|e| driver_missing(format!("write rule: {e}")))?;
        let script = format!(
            "install -m 644 '{}' /etc/udev/rules.d/51-inkroot-edl.rules && udevadm control --reload-rules && udevadm trigger",
            tmp.path().display()
        );
        let out = std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|e| driver_missing(format!("pkexec: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(driver_missing(format!(
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
    fn udev_rule_text_exact() {
        assert_eq!(
            UDEV_RULE,
            "SUBSYSTEM==\"usb\", ATTRS{idVendor}==\"05c6\", ATTRS{idProduct}==\"9008\", MODE=\"0666\", TAG+=\"uaccess\"\n"
        );
    }

    #[test]
    fn udev_needed_scans_dirs() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert!(!udev_rule_present(&[a.path(), b.path()]));
        std::fs::write(a.path().join("70-other.rules"), "nothing here").unwrap();
        assert!(!udev_rule_present(&[a.path(), b.path()]));
        std::fs::write(b.path().join("51-edl.rules"), "ATTRS{idProduct}==\"9008\"").unwrap();
        assert!(udev_rule_present(&[a.path(), b.path()]));
        assert!(!udev_rule_present(&[std::path::Path::new(
            "/nonexistent-dir-x"
        )]));
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
