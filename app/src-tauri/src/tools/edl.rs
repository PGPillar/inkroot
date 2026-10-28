//! Real Qualcomm `edl` runner. Every call has a timeout; timeout/cancel kill the process tree.

use super::process::{CancelToken, CmdSpec, Watchdog, run_watched};
use super::{Timeouts, ToolError};
use crate::logs::LogStore;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub trait Edl: Send + Sync {
    fn read(
        &self,
        part: &str,
        out: &Path,
        cancel: &CancelToken,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), ToolError>;
    fn write(&self, part: &str, img: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError>;
    fn reset(&self) -> Result<(), ToolError>;
    /// `reset` for the best-effort paths (after a failure or cancel): a short timeout, so a
    /// tablet that doesn't answer can't keep an `edl` process running for long.
    fn reset_quick(&self) -> Result<(), ToolError> {
        self.reset()
    }
}

#[derive(Debug, Clone)]
pub enum EdlOp {
    Read { part: String, out: PathBuf },
    Write { part: String, img: PathBuf },
    Reset,
}

pub struct RealEdl {
    pub path: PathBuf,
    /// Absolute path to the firehose loader.
    pub loader: PathBuf,
    pub log: Arc<LogStore>,
    pub timeouts: Timeouts,
    pub part_size: u64,
}

/// The argv for one `edl` call. The subcommand comes first, as in every line of edl's docopt
/// usage. `--memory` is accepted only by `r`/`w`: `reset` rejects it with its usage text.
pub fn edl_args(loader: &Path, op: &EdlOp) -> Vec<String> {
    let loader = format!("--loader={}", loader.display());
    match op {
        EdlOp::Read { part, out } => vec![
            "r".into(),
            part.clone(),
            out.display().to_string(),
            "--memory=ufs".into(),
            loader,
        ],
        EdlOp::Write { part, img } => vec![
            "w".into(),
            part.clone(),
            img.display().to_string(),
            "--memory=ufs".into(),
            loader,
        ],
        EdlOp::Reset => vec!["reset".into(), loader],
    }
}

/// edl's start-up notices about optional libraries Inkroot never uses: not worth a log line.
pub fn edl_log_keeps(line: &str) -> bool {
    !matches!(
        line.trim(),
        "Capstone library is missing (optional)." | "Keystone library is missing (optional)."
    )
}

/// True for an edl line showing the tablet answered edl's handshake: the Sahara hello
/// ("Protocol version"), a detected mode, the loader upload, or anything from the firehose
/// stage. "Device detected :)" only means the USB device could be opened; a tablet that is
/// wedged (or whose hello was lost) stops right there.
pub fn edl_handshake_progress(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    // The loader's own path may contain "firehose".
    if l.contains("using loader") {
        return false;
    }
    [
        "protocol version",
        "mode detected",
        "loader successfully uploaded",
        "firehose",
    ]
    .iter()
    .any(|m| l.contains(m))
}

/// True for an edl (pyusb/libusb) line saying this user may not open the USB device, e.g.
/// `usb.core.USBError: [Errno 13] Access denied (insufficient permissions)` or
/// `LIBUSB_ERROR_ACCESS`. A plain file "Permission denied" (errno 13 on a path) doesn't count.
pub fn edl_usb_permission_denied(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("insufficient permissions")
        || l.contains("libusb_error_access")
        || l.contains("access denied")
        || (l.contains("usb") && (l.contains("errno 13") || l.contains("errno=13")))
}

pub fn parse_percent(line: &str) -> Option<f32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(\d{1,3}(?:\.\d+)?)\s*%").expect("valid regex"));
    re.captures(line)?.get(1)?.as_str().parse().ok()
}

impl RealEdl {
    fn exec(
        &self,
        op: &EdlOp,
        timeout: std::time::Duration,
        cancel: Option<&CancelToken>,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), ToolError> {
        let spec = CmdSpec {
            program: self.path.clone(),
            args: edl_args(&self.loader, op),
            env: vec![],
        };
        let total = self.part_size;
        // A line saying the USB device couldn't be opened for lack of permission: whatever
        // edl does next (exit, or hang until the watchdog), that's the real problem.
        let denied: std::cell::RefCell<Option<String>> = Default::default();
        // A tablet that doesn't answer the handshake leaves edl hanging in Sahara for good:
        // give up after `edl_handshake` rather than the whole read/write timeout.
        let watchdog = Watchdog {
            within: self.timeouts.edl_handshake,
            progressed: &edl_handshake_progress,
        };
        let out = run_watched(
            &spec,
            timeout,
            cancel,
            &self.log,
            &edl_log_keeps,
            Some(watchdog),
            &mut |line| {
                if denied.borrow().is_none() && edl_usb_permission_denied(line) {
                    *denied.borrow_mut() = Some(line.trim().to_string());
                }
                if let Some(pct) = parse_percent(line) {
                    let pct = pct.clamp(0.0, 100.0) as f64;
                    progress(((pct / 100.0) * total as f64) as u64, total);
                }
            },
        );
        let denied = denied.into_inner();
        let out = match (out, denied) {
            (Ok(out), _) if out.code == 0 => return Ok(()),
            (Ok(_) | Err(ToolError::Stalled { .. }), Some(detail)) => {
                return Err(ToolError::UsbPermission {
                    tool: "edl".into(),
                    detail,
                });
            }
            (out, _) => out?,
        };
        if out.code != 0 {
            return Err(ToolError::Exit {
                tool: "edl".into(),
                code: out.code,
                stderr: out.stderr,
            });
        }
        Ok(())
    }
}

impl Edl for RealEdl {
    fn read(
        &self,
        part: &str,
        out: &Path,
        cancel: &CancelToken,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), ToolError> {
        let op = EdlOp::Read {
            part: part.to_string(),
            out: out.to_path_buf(),
        };
        self.exec(&op, self.timeouts.edl_read, Some(cancel), progress)
    }

    fn write(&self, part: &str, img: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError> {
        let op = EdlOp::Write {
            part: part.to_string(),
            img: img.to_path_buf(),
        };
        // No cancel token: cancel is rejected while writing.
        self.exec(&op, self.timeouts.edl_write, None, progress)
    }

    fn reset(&self) -> Result<(), ToolError> {
        self.exec(&EdlOp::Reset, self.timeouts.edl_reset, None, &|_, _| {})
    }

    fn reset_quick(&self) -> Result<(), ToolError> {
        self.exec(
            &EdlOp::Reset,
            self.timeouts.edl_reset_quick,
            None,
            &|_, _| {},
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `r`, `w` and `reset` usage lines of bkerler/edl 3.62 (edl.py's docopt `__doc__`),
    /// verbatim.
    const EDL_USAGE: &str = "\
    edl.py r <partitionname> <filename> [--memory=memtype] [--sectorsize==bytes] [--pagesperblock=number] [--lun=lun] [--loader=filename]  [--skipresponse] [--debugmode] [--vid=vid] [--pid=pid] [--skipstorageinit] [--portname=portname] [--serial] [--devicemodel=value]
    edl.py w <partitionname> <filename> [--partitionfilename=filename] [--memory=memtype] [--lun=lun] [--sectorsize==bytes] [--pagesperblock=number] [--skipwrite] [--skipresponse] [--loader=filename] [--debugmode] [--vid=vid] [--pid=pid] [--devicemodel=value] [--skipstorageinit] [--portname=portname] [--serial]
    edl.py reset [--resetmode=mode] [--loader=filename] [--debugmode] [--vid=vid] [--pid=pid] [--portname=portname] [--serial] [--devicemodel=value]
";

    /// A small docopt check for one usage line: the non-option args must match the line's
    /// commands and `<positionals>` in order, and every `--option` must be one the line lists
    /// (with `=value` exactly when the line's option takes one).
    fn matches_usage(usage: &str, argv: &[String]) -> Result<(), String> {
        let mut words = Vec::new();
        let mut opts = std::collections::HashMap::new();
        for tok in usage.split_whitespace().skip(1) {
            if let Some(o) = tok.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
                let (name, takes) = match o.split_once('=') {
                    Some((n, _)) => (n, true),
                    None => (o, false),
                };
                opts.insert(name.to_string(), takes);
            } else {
                words.push(tok);
            }
        }
        let mut pos = Vec::new();
        for a in argv {
            if let Some(o) = a.strip_prefix("--") {
                let (name, has_val) = match o.split_once('=') {
                    Some((n, _)) => (format!("--{n}"), true),
                    None => (a.clone(), false),
                };
                match opts.get(&name) {
                    Some(&takes) if takes == has_val => {}
                    Some(_) => return Err(format!("{a}: wrong value form")),
                    None => return Err(format!("{a}: not in usage")),
                }
            } else {
                pos.push(a.as_str());
            }
        }
        if pos.len() != words.len() {
            return Err(format!("positionals {pos:?} vs {words:?}"));
        }
        for (a, w) in pos.iter().zip(&words) {
            if !(w.starts_with('<') && w.ends_with('>')) && a != w {
                return Err(format!("{a} where usage has {w}"));
            }
        }
        Ok(())
    }

    fn usage_for(cmd: &str) -> &'static str {
        EDL_USAGE
            .lines()
            .find(|l| l.split_whitespace().nth(1) == Some(cmd))
            .expect("usage line")
    }

    fn ops() -> [EdlOp; 3] {
        [
            EdlOp::Read {
                part: "boot_b".into(),
                out: "/o/boot_b.img".into(),
            },
            EdlOp::Write {
                part: "boot_b".into(),
                img: "/o/boot_b.img".into(),
            },
            EdlOp::Reset,
        ]
    }

    #[test]
    fn args_exact() {
        let l = Path::new("/L/x.bin");
        let [r, w, reset] = ops();
        assert_eq!(
            edl_args(l, &r),
            [
                "r",
                "boot_b",
                "/o/boot_b.img",
                "--memory=ufs",
                "--loader=/L/x.bin"
            ]
        );
        assert_eq!(
            edl_args(l, &w),
            [
                "w",
                "boot_b",
                "/o/boot_b.img",
                "--memory=ufs",
                "--loader=/L/x.bin"
            ]
        );
        assert_eq!(edl_args(l, &reset), ["reset", "--loader=/L/x.bin"]);
    }

    #[test]
    fn args_match_edl_docopt_usage() {
        let l = Path::new("/L/x.bin");
        for op in ops() {
            let argv = edl_args(l, &op);
            let usage = usage_for(&argv[0]);
            matches_usage(usage, &argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        }
    }

    #[test]
    fn usage_check_rejects_the_old_reset_argv() {
        // The 0.1.3 argv that edl refused on real hardware.
        let old: Vec<String> = ["--memory=ufs", "--loader=/L/x.bin", "reset"]
            .map(String::from)
            .to_vec();
        assert!(matches_usage(usage_for("reset"), &old).is_err());
        let extra: Vec<String> = ["r", "boot_b", "/o/x.img", "extra"]
            .map(String::from)
            .to_vec();
        assert!(matches_usage(usage_for("r"), &extra).is_err());
        let flag_val: Vec<String> = ["reset", "--serial=1"].map(String::from).to_vec();
        assert!(matches_usage(usage_for("reset"), &flag_val).is_err());
    }

    #[test]
    fn optional_library_notices_stay_out_of_the_log() {
        assert!(!edl_log_keeps("Capstone library is missing (optional)."));
        assert!(!edl_log_keeps("Keystone library is missing (optional).\r"));
        assert!(edl_log_keeps("main - Using loader /L/x.bin ..."));
        assert!(edl_log_keeps("Progress: |██| 45.2% Read"));
    }

    #[test]
    fn handshake_progress_lines() {
        // bkerler/edl 3.62 output up to and past the handshake.
        for l in [
            "main - Mode detected: sahara",
            "main - Mode detected: firehose",
            "sahara - Protocol version: 2, Version supported: 1",
            "sahara - Loader successfully uploaded.",
            "main - Trying to connect to firehose loader ...",
            "firehose - INFO: Binary build date: Nov 15 2019",
        ] {
            assert!(edl_handshake_progress(l), "{l}");
        }
        for l in [
            "main - Using loader /L/qualcomm/x_fhprg.bin ...",
            "main - Using loader C:\\Inkroot\\loaders\\prog_firehose_ddr.elf ...",
            "main - Waiting for the device",
            "main - Device detected :)",
            "Capstone library is missing (optional).",
            "....",
            "Device is in Sahara error state, please reboot the device.",
            "",
        ] {
            assert!(!edl_handshake_progress(l), "{l}");
        }
    }

    #[cfg(unix)]
    fn stuck_edl(dir: &Path, script: &str, handshake: std::time::Duration) -> RealEdl {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("edl");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        struct Null;
        impl crate::events::Emitter for Null {
            fn device(&self, _: &crate::ipc::DeviceStatus) {}
            fn flow_state(&self, _: &crate::ipc::FlowState) {}
            fn progress(&self, _: &crate::ipc::Progress) {}
            fn log(&self, _: &crate::ipc::LogLine) {}
        }
        let log = LogStore::new(
            Arc::new(Null),
            Arc::new(crate::clock::FakeClock::new(chrono::Local::now())),
        );
        RealEdl {
            path,
            loader: dir.join("prog_firehose.bin"),
            log: Arc::new(log),
            timeouts: Timeouts {
                edl_handshake: handshake,
                ..Timeouts::default()
            },
            part_size: 100,
        }
    }

    #[test]
    fn usb_permission_lines() {
        for l in [
            "usb.core.USBError: [Errno 13] Access denied (insufficient permissions)",
            "libusb: error [_get_usbfs_fd] libusb couldn't open USB device /dev/bus/usb/001/007, errno=13",
            "LIBUSB_ERROR_ACCESS",
            "main - Access denied (insufficient permissions)",
        ] {
            assert!(edl_usb_permission_denied(l), "{l}");
        }
        for l in [
            "PermissionError: [Errno 13] Permission denied: '/home/a/boot_a.img'",
            "main - Device detected :)",
            "USB error: No such device",
        ] {
            assert!(!edl_usb_permission_denied(l), "{l}");
        }
    }

    /// Debian report: no udev rule grants this user the 9008 node, so pyusb can't open it.
    #[cfg(unix)]
    #[test]
    fn usb_permission_denied_is_its_own_error() {
        let dir = tempfile::tempdir().unwrap();
        let edl = stuck_edl(
            dir.path(),
            "echo 'main - Waiting for the device'; echo 'usb.core.USBError: [Errno 13] Access denied (insufficient permissions)' >&2; exit 1",
            std::time::Duration::from_secs(20),
        );
        let r = edl.read(
            "boot_a",
            &dir.path().join("o.img"),
            &CancelToken::default(),
            &|_, _| {},
        );
        assert!(
            matches!(&r, Err(ToolError::UsbPermission { tool, detail }) if tool == "edl" && detail.contains("Access denied")),
            "{r:?}"
        );
        // Denied, then hanging: still a permission problem, not a silent tablet.
        let edl = stuck_edl(
            dir.path(),
            "echo 'LIBUSB_ERROR_ACCESS'; sleep 60",
            std::time::Duration::from_millis(300),
        );
        let r = edl.reset_quick();
        assert!(matches!(&r, Err(ToolError::UsbPermission { .. })), "{r:?}");
    }

    /// The Windows report: edl opens the device, then hangs in sahara.connect forever.
    #[cfg(unix)]
    #[test]
    fn stuck_handshake_is_given_up_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let edl = stuck_edl(
            dir.path(),
            "echo 'main - Using loader x_firehose.bin ...'; echo 'main - Waiting for the device'; echo 'main - Device detected :)'; sleep 60",
            std::time::Duration::from_millis(500),
        );
        let t = std::time::Instant::now();
        let r = edl.read(
            "boot_a",
            &dir.path().join("o.img"),
            &CancelToken::default(),
            &|_, _| {},
        );
        assert!(
            matches!(&r, Err(ToolError::Stalled { tool, .. }) if tool == "edl"),
            "{r:?}"
        );
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        let r = edl.write("boot_a", &dir.path().join("o.img"), &|_, _| {});
        assert!(matches!(r, Err(ToolError::Stalled { .. })), "{r:?}");
        let r = edl.reset_quick();
        assert!(matches!(r, Err(ToolError::Stalled { .. })), "{r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn answered_handshake_keeps_running() {
        let dir = tempfile::tempdir().unwrap();
        let edl = stuck_edl(
            dir.path(),
            "echo 'main - Device detected :)'; echo 'main - Mode detected: sahara'; sleep 3; echo 'Progress: 100%'",
            // Generous: only the handshake line must arrive in time, not the whole run.
            std::time::Duration::from_millis(1500),
        );
        let r = edl.read(
            "boot_a",
            &dir.path().join("o.img"),
            &CancelToken::default(),
            &|_, _| {},
        );
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn parse_percent_cases() {
        assert_eq!(parse_percent("Progress: |████| 45.2% Read"), Some(45.2));
        assert_eq!(parse_percent("100 %"), Some(100.0));
        assert_eq!(parse_percent("no pct"), None);
    }
}
