//! Foreground child-process runner with line streaming, timeout and tree-kill.

use super::ToolError;
use crate::ipc::LogStream;
use crate::logs::LogStore;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct CmdSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(unix)]
    {
        // The child leads its own process group (process_group(0)), so -pid reaches all descendants.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // Belt and braces (and the only effect if the group kill failed).
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads `r` to EOF, splitting on `\n` and `\r`, sending non-empty lines.
fn pump(mut r: impl Read, stream: LogStream, tx: mpsc::Sender<(LogStream, String)>) {
    let mut buf = [0u8; 4096];
    let mut cur: Vec<u8> = Vec::new();
    let flush = |cur: &mut Vec<u8>| {
        if !cur.is_empty() {
            let s = String::from_utf8_lossy(cur).into_owned();
            cur.clear();
            let _ = tx.send((stream, s));
        }
    };
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        flush(&mut cur);
                    } else {
                        cur.push(b);
                    }
                }
            }
        }
    }
    flush(&mut cur);
}

pub fn run(
    spec: &CmdSpec,
    timeout: Duration,
    cancel: Option<&CancelToken>,
    log: &LogStore,
    on_line: &mut dyn FnMut(&str),
) -> Result<CmdOutput, ToolError> {
    run_filtered(spec, timeout, cancel, log, &|_| true, on_line)
}

/// Like `run`, but only lines for which `keep` returns true reach the log. Every line still
/// reaches `on_line` and the returned output.
pub fn run_filtered(
    spec: &CmdSpec,
    timeout: Duration,
    cancel: Option<&CancelToken>,
    log: &LogStore,
    keep: &dyn Fn(&str) -> bool,
    on_line: &mut dyn FnMut(&str),
) -> Result<CmdOutput, ToolError> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000 | 0x00000200);
    }
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::Missing(spec.program.clone())
        } else {
            ToolError::Spawn(e.to_string())
        }
    })?;

    #[cfg(unix)]
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    let mut readers = Vec::new();
    if let Some(o) = child.stdout.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || pump(o, LogStream::Stdout, tx)));
    }
    if let Some(e) = child.stderr.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || pump(e, LogStream::Stderr, tx)));
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut handle = |item: (LogStream, String), on_line: &mut dyn FnMut(&str)| {
        let (stream, line) = item;
        if keep(&line) {
            log.line(stream, &line);
        }
        on_line(&line);
        let buf = if stream == LogStream::Stderr {
            &mut stderr
        } else {
            &mut stdout
        };
        buf.push_str(&line);
        buf.push('\n');
    };

    let start = Instant::now();
    let outcome: Result<i32, ToolError> = loop {
        while let Ok(item) = rx.try_recv() {
            handle(item, on_line);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.code().unwrap_or(-1)),
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                break Err(ToolError::Spawn(e.to_string()));
            }
        }
        if cancel.is_some_and(|c| c.is_cancelled()) {
            kill_tree(&mut child);
            break Err(ToolError::Cancelled);
        }
        if start.elapsed() >= timeout {
            kill_tree(&mut child);
            break Err(ToolError::Timeout {
                tool: tool_name(spec),
                secs: timeout.as_secs(),
            });
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(item) => handle(item, on_line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Both pipes closed but the child hasn't exited yet: avoid a hot loop.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(50))
            }
        }
    };

    // The direct child is dead and reaped, but a descendant may still hold the pipes open
    // (inherited stdout/stderr, or a daemon that left the group). Drain for a bounded grace
    // period; if the pipes don't close, kill the group and stop waiting rather than hang.
    let grace = if outcome.is_ok() {
        timeout
            .saturating_sub(start.elapsed())
            .min(Duration::from_secs(2))
    } else {
        Duration::from_secs(1)
    };
    let deadline = Instant::now() + grace;
    let mut closed = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(item) => handle(item, on_line),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                closed = true;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
        }
    }
    if closed {
        for r in readers {
            let _ = r.join();
        }
    } else {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
        // Readers are detached: they exit on their own once the pipes close. Take what's buffered.
        while let Ok(item) = rx.try_recv() {
            handle(item, on_line);
        }
        drop(readers);
    }
    outcome.map(|code| CmdOutput {
        code,
        stdout,
        stderr,
    })
}

/// Short tool name (file stem) used in errors, e.g. "edl" or "adb".
fn tool_name(spec: &CmdSpec) -> String {
    spec.program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::events::Emitter;
    use crate::ipc::{DeviceStatus, ErrorCode, FlowError, FlowState, LogLine, Progress};

    struct Null;
    impl Emitter for Null {
        fn device(&self, _: &DeviceStatus) {}
        fn flow_state(&self, _: &FlowState) {}
        fn progress(&self, _: &Progress) {}
        fn log(&self, _: &LogLine) {}
    }
    fn store() -> LogStore {
        LogStore::new(
            Arc::new(Null),
            Arc::new(FakeClock::new(chrono::Local::now())),
        )
    }
    fn sh(script: &str) -> CmdSpec {
        CmdSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
        }
    }

    #[test]
    fn streams_lines_and_exit_code() {
        let mut seen = Vec::new();
        let out = run(
            &sh("echo one; echo two 1>&2; printf '3%%\\r4%%\\n'; exit 3"),
            Duration::from_secs(10),
            None,
            &store(),
            &mut |l| seen.push(l.to_string()),
        )
        .unwrap();
        assert_eq!(out.code, 3);
        for want in ["one", "two", "3%", "4%"] {
            assert!(seen.iter().any(|l| l == want), "missing {want}: {seen:?}");
        }
        assert!(out.stdout.contains("one") && out.stderr.contains("two"));
    }

    #[test]
    fn filtered_lines_skip_the_log_only() {
        let log = store();
        let mut seen = Vec::new();
        let out = run_filtered(
            &sh("echo keep; echo noise; echo also"),
            Duration::from_secs(10),
            None,
            &log,
            &|l| l != "noise",
            &mut |l| seen.push(l.to_string()),
        )
        .unwrap();
        let text = log.text(None);
        assert!(text.contains("keep") && text.contains("also"), "{text}");
        assert!(!text.contains("noise"), "{text}");
        assert!(seen.iter().any(|l| l == "noise"));
        assert!(out.stdout.contains("noise"));
    }

    #[test]
    fn timeout_kills_tree() {
        let dir = tempfile::tempdir().unwrap();
        let pidf = dir.path().join("pid");
        let script = format!("sleep 30 & echo $! > {}; sleep 30", pidf.display());
        let t = std::time::Instant::now();
        let r = run(
            &sh(&script),
            Duration::from_millis(300),
            None,
            &store(),
            &mut |_| {},
        );
        assert!(matches!(r, Err(ToolError::Timeout { .. })), "{r:?}");
        assert!(t.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(200));
        let pid: i32 = std::fs::read_to_string(&pidf)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "grandchild still alive");
    }

    #[test]
    fn lingering_descendant_does_not_hang() {
        let t = std::time::Instant::now();
        let r = run(
            &sh("sleep 30 & exit 0"),
            Duration::from_secs(5),
            None,
            &store(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.code, 0);
        assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
    }

    #[test]
    fn cancel_kills() {
        let tok = CancelToken::default();
        let t2 = tok.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            t2.cancel();
        });
        let r = run(
            &sh("sleep 30"),
            Duration::from_secs(20),
            Some(&tok),
            &store(),
            &mut |_| {},
        );
        assert!(matches!(r, Err(ToolError::Cancelled)), "{r:?}");
    }

    #[test]
    fn missing_program() {
        let spec = CmdSpec {
            program: "/nonexistent/adb".into(),
            args: vec![],
        };
        let r = run(&spec, Duration::from_secs(5), None, &store(), &mut |_| {});
        let e = r.unwrap_err();
        assert!(matches!(e, ToolError::Missing(_)));
        assert_eq!(FlowError::from(e).code, ErrorCode::ToolMissing);
    }
}
