//! Real `adb` runner.

use super::process::{CmdOutput, CmdSpec, run};
use super::{Timeouts, ToolError};
use crate::logs::LogStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbState {
    Device,
    Unauthorized,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: AdbState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebootTarget {
    System,
    Edl,
}

pub trait Adb: Send + Sync {
    fn devices(&self) -> Result<Vec<AdbDevice>, ToolError>;
    fn shell(&self, serial: &str, cmd: &str, timeout: Duration) -> Result<CmdOutput, ToolError>;
    fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), ToolError>;
    fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<(), ToolError>;
    fn reboot(&self, serial: &str, target: RebootTarget) -> Result<(), ToolError>;
    fn kill_server(&self);
    /// Like `shell`, but the output doesn't go to the log (e.g. `getprop`'s ~1000 lines; the
    /// caller logs the parsed fields instead).
    fn shell_unlogged(
        &self,
        serial: &str,
        cmd: &str,
        timeout: Duration,
    ) -> Result<CmdOutput, ToolError> {
        self.shell(serial, cmd, timeout)
    }
    /// A copy whose output never reaches the log at all, for the idle device poller. `None`
    /// when this implementation doesn't log anyway.
    fn quiet(&self) -> Option<Arc<dyn Adb>> {
        None
    }
}

pub struct RealAdb {
    pub path: PathBuf,
    pub log: Arc<LogStore>,
    pub timeouts: Timeouts,
}

pub fn adb_args(serial: Option<&str>, rest: &[&str]) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(s) = serial {
        v.push("-s".to_string());
        v.push(s.to_string());
    }
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

/// Environment for every adb spawn. Inkroot only uses USB, so mDNS discovery is off; that keeps
/// adb from opening network listeners (which trigger the Windows Firewall prompt). The server
/// is never started with `-a`, so it stays on its default localhost-only port.
pub fn adb_env() -> Vec<(String, String)> {
    vec![
        ("ADB_MDNS".to_string(), "0".to_string()),
        ("ADB_MDNS_AUTO_CONNECT".to_string(), "0".to_string()),
    ]
}

pub fn adb_spec(program: std::path::PathBuf, serial: Option<&str>, rest: &[&str]) -> CmdSpec {
    CmdSpec {
        program,
        args: adb_args(serial, rest),
        env: adb_env(),
    }
}

/// Parses `adb devices` output. Ignores the header, blank lines and daemon chatter.
pub fn parse_devices(out: &str) -> Vec<AdbDevice> {
    out.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with("List of devices") || l.starts_with('*') {
                return None;
            }
            let (serial, state) = l.split_once('\t')?;
            let state = match state.trim() {
                "device" => AdbState::Device,
                "unauthorized" => AdbState::Unauthorized,
                _ => AdbState::Offline,
            };
            Some(AdbDevice {
                serial: serial.trim().to_string(),
                state,
            })
        })
        .collect()
}

impl RealAdb {
    fn exec(
        &self,
        serial: Option<&str>,
        rest: &[&str],
        timeout: Duration,
    ) -> Result<CmdOutput, ToolError> {
        self.exec_to(&self.log, serial, rest, timeout)
    }

    fn exec_to(
        &self,
        log: &LogStore,
        serial: Option<&str>,
        rest: &[&str],
        timeout: Duration,
    ) -> Result<CmdOutput, ToolError> {
        let spec = adb_spec(self.path.clone(), serial, rest);
        run(&spec, timeout, None, log, &mut |_| {})
    }

    fn exec_ok(&self, serial: &str, rest: &[&str], timeout: Duration) -> Result<(), ToolError> {
        let out = self.exec(Some(serial), rest, timeout)?;
        if out.code != 0 {
            return Err(ToolError::Exit {
                tool: "adb".into(),
                code: out.code,
                stderr: out.stderr,
            });
        }
        Ok(())
    }
}

impl Adb for RealAdb {
    fn devices(&self) -> Result<Vec<AdbDevice>, ToolError> {
        // Polled every few seconds while waiting for the tablet: its raw output ("List of
        // devices attached" …) never goes to the log; callers log state changes instead.
        let out = self.exec_to(
            &LogStore::discard(),
            None,
            &["devices"],
            self.timeouts.adb_shell,
        )?;
        Ok(parse_devices(&out.stdout))
    }

    fn shell(&self, serial: &str, cmd: &str, timeout: Duration) -> Result<CmdOutput, ToolError> {
        self.exec(Some(serial), &["shell", cmd], timeout)
    }

    fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), ToolError> {
        let l = local.to_string_lossy();
        self.exec_ok(serial, &["push", &l, remote], self.timeouts.adb_push)
    }

    fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<(), ToolError> {
        let l = local.to_string_lossy();
        self.exec_ok(serial, &["pull", remote, &l], self.timeouts.adb_push)
    }

    fn reboot(&self, serial: &str, target: RebootTarget) -> Result<(), ToolError> {
        let rest: &[&str] = match target {
            RebootTarget::System => &["reboot"],
            RebootTarget::Edl => &["reboot", "edl"],
        };
        self.exec_ok(serial, rest, self.timeouts.adb_shell)
    }

    fn kill_server(&self) {
        let _ = self.exec(None, &["kill-server"], self.timeouts.adb_shell);
    }

    fn shell_unlogged(
        &self,
        serial: &str,
        cmd: &str,
        timeout: Duration,
    ) -> Result<CmdOutput, ToolError> {
        self.exec_to(&LogStore::discard(), Some(serial), &["shell", cmd], timeout)
    }

    fn quiet(&self) -> Option<Arc<dyn Adb>> {
        Some(Arc::new(RealAdb {
            path: self.path.clone(),
            log: Arc::new(LogStore::discard()),
            timeouts: self.timeouts.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_spec_disables_mdns_and_stays_local() {
        let spec = adb_spec("adb".into(), Some("X"), &["devices"]);
        assert!(spec.env.contains(&("ADB_MDNS".into(), "0".into())));
        assert!(
            spec.env
                .contains(&("ADB_MDNS_AUTO_CONNECT".into(), "0".into()))
        );
        assert!(!spec.args.iter().any(|a| a == "-a" || a.contains("0.0.0.0")));
    }

    #[test]
    fn parse_devices_states() {
        let d = parse_devices(
            "* daemon started successfully\nList of devices attached\nABC\tdevice\nDEF\tunauthorized\nGHI\toffline\n\n",
        );
        assert_eq!(
            d,
            vec![
                AdbDevice {
                    serial: "ABC".into(),
                    state: AdbState::Device
                },
                AdbDevice {
                    serial: "DEF".into(),
                    state: AdbState::Unauthorized
                },
                AdbDevice {
                    serial: "GHI".into(),
                    state: AdbState::Offline
                },
            ]
        );
    }

    #[test]
    fn args_with_serial() {
        assert_eq!(
            adb_args(Some("S"), &["shell", "id"]),
            ["-s", "S", "shell", "id"]
        );
        assert_eq!(adb_args(None, &["devices"]), ["devices"]);
    }

    #[cfg(unix)]
    #[test]
    fn devices_output_stays_out_of_the_log() {
        use crate::clock::FakeClock;
        use crate::events::Emitter;
        use crate::ipc::{DeviceStatus, FlowState, LogLine, Progress};
        use std::os::unix::fs::PermissionsExt;
        struct Null;
        impl Emitter for Null {
            fn device(&self, _: &DeviceStatus) {}
            fn flow_state(&self, _: &FlowState) {}
            fn progress(&self, _: &Progress) {}
            fn log(&self, _: &LogLine) {}
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("adb");
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf 'List of devices attached\\nABC\\tdevice\\n\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = Arc::new(LogStore::new(
            Arc::new(Null),
            Arc::new(FakeClock::new(chrono::Local::now())),
        ));
        let adb = RealAdb {
            path,
            log: log.clone(),
            timeouts: Timeouts::default(),
        };
        for _ in 0..3 {
            let d = adb.devices().unwrap();
            assert_eq!(d.len(), 1);
            assert_eq!(d[0].state, AdbState::Device);
        }
        assert_eq!(log.text(None), "");
    }
}
