//! Real Qualcomm `edl` runner. Every call has a timeout; timeout/cancel kill the process tree.

use super::process::{CancelToken, CmdSpec, run};
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

pub fn edl_args(loader: &Path, op: &EdlOp) -> Vec<String> {
    let mut v = vec![
        "--memory=ufs".to_string(),
        format!("--loader={}", loader.display()),
    ];
    match op {
        EdlOp::Read { part, out } => {
            v.extend(["r".into(), part.clone(), out.display().to_string()]);
        }
        EdlOp::Write { part, img } => {
            v.extend(["w".into(), part.clone(), img.display().to_string()]);
        }
        EdlOp::Reset => v.push("reset".into()),
    }
    v
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
        };
        let total = self.part_size;
        let out = run(&spec, timeout, cancel, &self.log, &mut |line| {
            if let Some(pct) = parse_percent(line) {
                let pct = pct.clamp(0.0, 100.0) as f64;
                progress(((pct / 100.0) * total as f64) as u64, total);
            }
        })?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_exact() {
        let l = Path::new("/L/x.bin");
        assert_eq!(
            edl_args(
                l,
                &EdlOp::Read {
                    part: "boot_b".into(),
                    out: "/o/boot_b.img".into()
                }
            ),
            [
                "--memory=ufs",
                "--loader=/L/x.bin",
                "r",
                "boot_b",
                "/o/boot_b.img"
            ]
        );
        assert_eq!(
            edl_args(
                l,
                &EdlOp::Write {
                    part: "boot_b".into(),
                    img: "/o/boot_b.img".into()
                }
            ),
            [
                "--memory=ufs",
                "--loader=/L/x.bin",
                "w",
                "boot_b",
                "/o/boot_b.img"
            ]
        );
        assert_eq!(
            edl_args(l, &EdlOp::Reset),
            ["--memory=ufs", "--loader=/L/x.bin", "reset"]
        );
    }

    #[test]
    fn parse_percent_cases() {
        assert_eq!(parse_percent("Progress: |████| 45.2% Read"), Some(45.2));
        assert_eq!(parse_percent("100 %"), Some(100.0));
        assert_eq!(parse_percent("no pct"), None);
    }
}
