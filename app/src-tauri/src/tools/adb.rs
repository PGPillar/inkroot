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
        let spec = CmdSpec {
            program: self.path.clone(),
            args: adb_args(serial, rest),
        };
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
        let out = self.exec(None, &["devices"], self.timeouts.adb_shell)?;
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
}
