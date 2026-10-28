//! Real Qualcomm `edl` runner. Every call has a timeout; timeout/cancel kill the process tree.

use super::process::{CancelToken, CmdSpec, run_filtered};
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
        let out = run_filtered(
            &spec,
            timeout,
            cancel,
            &self.log,
            &edl_log_keeps,
            &mut |line| {
                if let Some(pct) = parse_percent(line) {
                    let pct = pct.clamp(0.0, 100.0) as f64;
                    progress(((pct / 100.0) * total as f64) as u64, total);
                }
            },
        )?;
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
    fn parse_percent_cases() {
        assert_eq!(parse_percent("Progress: |████| 45.2% Read"), Some(45.2));
        assert_eq!(parse_percent("100 %"), Some(100.0));
        assert_eq!(parse_percent("no pct"), None);
    }
}
