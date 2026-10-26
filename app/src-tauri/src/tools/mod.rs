//! Bundled-tool plumbing: errors, timeouts, resource paths and runners.

pub mod adb;
pub mod edl;
pub mod process;
pub mod usb;

use crate::ipc::{ErrorCode, FlowError, FlowKind, SuggestedAction};
use crate::profile::DeviceProfile;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    Missing(PathBuf),
    #[error("could not start tool: {0}")]
    Spawn(String),
    #[error("{tool} timed out after {secs}s")]
    Timeout { tool: String, secs: u64 },
    #[error("{tool} exited with code {code}: {stderr}")]
    Exit {
        tool: String,
        code: i32,
        stderr: String,
    },
    #[error("cancelled")]
    Cancelled,
}

impl From<ToolError> for FlowError {
    fn from(e: ToolError) -> FlowError {
        let detail = format!("{e:?}");
        let (code, message, suggested_action) = match &e {
            ToolError::Missing(_) => (
                ErrorCode::ToolMissing,
                "A file Inkroot needs is missing. Please reinstall Inkroot and try again.",
                None,
            ),
            ToolError::Timeout { tool, .. } if tool == "edl" => (
                ErrorCode::EdlTimeout,
                "The tablet stopped responding. Check the cable, then reconnect and try again.",
                Some(SuggestedAction::Reconnect),
            ),
            _ => (
                ErrorCode::Internal,
                "Something went wrong while talking to the tablet. Please try again.",
                None,
            ),
        };
        FlowError {
            code,
            message: message.to_string(),
            detail,
            suggested_action,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub adb_shell: Duration,
    pub adb_push: Duration,
    pub patch: Duration,
    pub edl_read: Duration,
    pub edl_write: Duration,
    pub edl_reset: Duration,
    pub wait_edl: Duration,
    pub wait_boot: Duration,
    pub wait_bootsvc: Duration,
    pub poll: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            adb_shell: Duration::from_secs(60),
            adb_push: Duration::from_secs(120),
            patch: Duration::from_secs(180),
            edl_read: Duration::from_secs(600),
            edl_write: Duration::from_secs(900),
            edl_reset: Duration::from_secs(60),
            wait_edl: Duration::from_secs(60),
            wait_boot: Duration::from_secs(180),
            wait_bootsvc: Duration::from_secs(90),
            poll: Duration::from_millis(1500),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub root: PathBuf,
}

impl ToolPaths {
    /// `INKROOT_TOOLS_DIR` wins; otherwise `<resource_dir>/resources`.
    pub fn resolve(resource_dir: Option<PathBuf>) -> ToolPaths {
        let root = match std::env::var_os("INKROOT_TOOLS_DIR") {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => resource_dir.unwrap_or_default().join("resources"),
        };
        ToolPaths { root }
    }

    pub fn adb(&self) -> PathBuf {
        self.root
            .join("tools")
            .join(if cfg!(windows) { "adb.exe" } else { "adb" })
    }
    pub fn edl(&self) -> PathBuf {
        self.root
            .join("tools")
            .join("edl")
            .join(if cfg!(windows) { "edl.exe" } else { "edl" })
    }
    pub fn loader(&self, rel: &str) -> PathBuf {
        self.root.join("tools").join("loaders").join(rel)
    }
    pub fn wdi_simple(&self) -> PathBuf {
        self.root.join("tools").join("wdi-simple.exe")
    }
    pub fn magisk_apk(&self, version: &str) -> PathBuf {
        self.root
            .join("magisk")
            .join(format!("Magisk-v{version}.apk"))
    }
    pub fn boot_service(&self, name: &str) -> PathBuf {
        self.root.join("boot-service").join(name)
    }

    pub fn require(&self, p: &Path) -> Result<PathBuf, FlowError> {
        if p.is_file() {
            Ok(p.to_path_buf())
        } else {
            Err(FlowError {
                code: ErrorCode::ToolMissing,
                message: "A file Inkroot needs is missing. Please reinstall Inkroot and try again."
                    .into(),
                detail: p.display().to_string(),
                suggested_action: None,
            })
        }
    }

    pub fn require_for(&self, kind: FlowKind, p: &DeviceProfile) -> Result<(), FlowError> {
        let loader = self.loader(&p.edl.loader);
        let boot_all = [
            "init.bootsvc.rc",
            "bootsvc.sh",
            "inkroot-default-service.sh",
        ];
        let list: Vec<PathBuf> = match kind {
            FlowKind::Backup | FlowKind::Unroot => vec![self.adb(), self.edl(), loader],
            FlowKind::Rescue => vec![self.edl(), loader],
            FlowKind::Root => {
                let mut v = vec![
                    self.adb(),
                    self.edl(),
                    loader,
                    self.magisk_apk(&p.magisk.version),
                ];
                v.extend(boot_all.iter().map(|n| self.boot_service(n)));
                v
            }
            FlowKind::ReinstallBootService => vec![
                self.adb(),
                self.boot_service("bootsvc.sh"),
                self.boot_service("inkroot-default-service.sh"),
            ],
            FlowKind::ImportBackup => vec![],
        };
        for f in &list {
            self.require(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;

    #[test]
    fn paths_layout() {
        let p = ToolPaths { root: "/r".into() };
        #[cfg(unix)]
        {
            assert_eq!(p.adb(), PathBuf::from("/r/tools/adb"));
            assert_eq!(p.edl(), PathBuf::from("/r/tools/edl/edl"));
        }
        assert_eq!(
            p.loader("qualcomm/x.bin"),
            PathBuf::from("/r/tools/loaders/qualcomm/x.bin")
        );
        assert_eq!(p.wdi_simple(), PathBuf::from("/r/tools/wdi-simple.exe"));
        assert_eq!(
            p.magisk_apk("30.7"),
            PathBuf::from("/r/magisk/Magisk-v30.7.apk")
        );
        assert_eq!(
            p.boot_service("bootsvc.sh"),
            PathBuf::from("/r/boot-service/bootsvc.sh")
        );
    }

    #[test]
    fn env_override() {
        let _g = crate::env_lock();
        unsafe { std::env::set_var("INKROOT_TOOLS_DIR", "/t") };
        assert_eq!(
            ToolPaths::resolve(Some("/res".into())).root,
            PathBuf::from("/t")
        );
        unsafe { std::env::remove_var("INKROOT_TOOLS_DIR") };
        assert_eq!(
            ToolPaths::resolve(Some("/res".into())).root,
            PathBuf::from("/res/resources")
        );
    }

    #[test]
    fn timeouts_defaults() {
        let t = Timeouts::default();
        assert_eq!(t.edl_read, Duration::from_secs(600));
        assert_eq!(t.edl_write, Duration::from_secs(900));
        assert_eq!(t.poll, Duration::from_millis(1500));
    }

    #[test]
    fn tool_error_mapping() {
        let e = FlowError::from(ToolError::Timeout {
            tool: "edl".into(),
            secs: 5,
        });
        assert_eq!(e.code, ErrorCode::EdlTimeout);
        assert_eq!(e.suggested_action, Some(SuggestedAction::Reconnect));
        let e = FlowError::from(ToolError::Timeout {
            tool: "adb".into(),
            secs: 5,
        });
        assert_eq!(e.code, ErrorCode::Internal);
    }

    #[test]
    fn require_for_root_lists_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ToolPaths {
            root: dir.path().to_path_buf(),
        };
        let prof = crate::profile::builtin_profiles().remove(0);
        let e = paths.require_for(FlowKind::Root, &prof).unwrap_err();
        assert_eq!(e.code, ErrorCode::ToolMissing);
        assert!(paths.require_for(FlowKind::ImportBackup, &prof).is_ok());
        let mut all = vec![
            paths.adb(),
            paths.edl(),
            paths.loader(&prof.edl.loader),
            paths.magisk_apk(&prof.magisk.version),
        ];
        for n in [
            "init.bootsvc.rc",
            "bootsvc.sh",
            "inkroot-default-service.sh",
        ] {
            all.push(paths.boot_service(n));
        }
        for f in &all {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"x").unwrap();
        }
        std::fs::write(dir.path().join("extra.txt"), b"x").unwrap();
        for k in [
            FlowKind::Root,
            FlowKind::Backup,
            FlowKind::Rescue,
            FlowKind::Unroot,
            FlowKind::ReinstallBootService,
        ] {
            assert!(paths.require_for(k, &prof).is_ok(), "{k:?}");
        }
    }
}
