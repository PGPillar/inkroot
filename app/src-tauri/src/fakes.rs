//! One simulated BOOX Note Max. Backs every flow test and the shipped demo mode.

use crate::magisk::{
    APK_PAYLOAD, CHECK_CLEANUP_CMD, CHECK_DIR, CHECK_PREP_CMD, HOOK_CHECK_CMD, MGK_DIR,
    MGK_RESET_CMD, PATCH_CMD, STOCK_CHECK_CMD,
};
use crate::tools::ToolError;
use crate::tools::adb::{Adb, AdbDevice, AdbState, RebootTarget};
use crate::tools::edl::Edl;
use crate::tools::process::{CancelToken, CmdOutput};
use crate::tools::usb::{EdlUsbInfo, UsbProbe};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Appended (over the image tail, keeping the size) by the fake patch step.
pub const PATCH_MARKER: &[u8] = b"INKROOT-PATCHED";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Off,
    System {
        authorized: bool,
    },
    Edl,
}

#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// The EDL device never shows up on USB, and every edl call times out.
    pub edl_never_appears: bool,
    pub read_fails_on: Option<String>,
    pub write_corrupts_times: u32,
    /// The next N `edl w` calls fail (time out) with the tablet still in EDL.
    pub write_errors: u32,
    pub patch_fails: bool,
    /// The fake patch writes `patched-boot.img` WITHOUT the marker, so the tablet boots unrooted.
    pub patch_drops_marker: bool,
    pub boot_hangs: bool,
    /// The tablet starts but hasn't allowed USB debugging from this computer.
    pub boots_unauthorized: bool,
    /// Instead of starting, the tablet drops straight back into EDL after a reset.
    pub boot_returns_to_edl: bool,
    pub stock_check_exit: Option<i32>,
    pub hook_check_exit: Option<i32>,
    /// Every read of a partition after the first (counted over the world's lifetime, across
    /// all reads of that partition) returns different bytes, varying with the read count.
    pub verify_read_differs: bool,
    /// The next N `cat /data/local/tmp/bootsvc.log` calls fail with an adb error.
    pub bootsvc_log_errors: u32,
    /// While the tablet is `Off`, it enters EDL this long after the first `edl_device()` poll
    /// (real time). Demo mode's bricked tablet.
    pub edl_appears_after: Option<Duration>,
    /// The next N `edl reset` calls fail (edl exits 1, as with a usage error) and the tablet
    /// stays in EDL. `u32::MAX` for never.
    pub reset_errors: u32,
    /// The next N `edl reset` calls report success but the tablet stays in EDL.
    pub reset_ignored: u32,
}

/// Real-time pauses the fake tools take, so demo mode feels like the real thing. All zero by
/// default (tests never sleep); the demo scales them by its speed factor.
#[derive(Debug, Clone, Copy, Default)]
pub struct Delays {
    /// Per partition read.
    pub read: Duration,
    /// Per partition write.
    pub write: Duration,
    /// From a reset or reboot until the tablet is visible in adb again.
    pub reset_boot: Duration,
    /// The Magisk boot patch.
    pub patch: Duration,
}

/// Sleeps `total` in slices, reporting progress over `size` bytes. No-op for a zero delay.
fn pace(total: Duration, size: u64, progress: &dyn Fn(u64, u64)) {
    if total.is_zero() {
        return;
    }
    const SLICES: u32 = 10;
    for i in 1..SLICES {
        std::thread::sleep(total / SLICES);
        progress(size * u64::from(i) / u64::from(SLICES), size);
    }
    std::thread::sleep(total / SLICES);
}

fn nap(d: Duration) {
    if !d.is_zero() {
        std::thread::sleep(d);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Devices,
    Shell {
        serial: String,
        cmd: String,
    },
    Push {
        local_name: String,
        remote: String,
    },
    Pull {
        remote: String,
    },
    Reboot {
        serial: String,
        target: RebootTarget,
    },
    EdlRead {
        part: String,
    },
    EdlWrite {
        part: String,
        sha: String,
    },
    EdlReset,
    DriverInstall,
    UdevInstall,
}

#[derive(Debug, Default)]
pub struct World {
    pub serial: String,
    pub mode: Mode,
    pub props: BTreeMap<String, String>,
    pub battery: u8,
    pub partitions: HashMap<String, Vec<u8>>,
    pub files: HashMap<String, Vec<u8>>,
    pub rooted: bool,
    pub magisk_version: Option<String>,
    pub bootsvc_lines: Vec<String>,
    pub service_running: bool,
    pub other_devices: Vec<(String, AdbState, BTreeMap<String, String>)>,
    pub faults: Faults,
    pub calls: Vec<Call>,
    /// A reboot is in flight; the next `devices()` poll completes it.
    pub pending_boot: bool,
    pub read_counts: HashMap<String, u32>,
    pub delays: Delays,
    /// When `FakeUsb::edl_device` was first polled (drives `Faults::edl_appears_after`).
    pub edl_poll_started: Option<Instant>,
}

impl World {
    fn boot(&mut self) {
        self.pending_boot = false;
        let suffix = self
            .props
            .get("ro.boot.slot_suffix")
            .cloned()
            .unwrap_or_else(|| "_a".into());
        let rooted = self
            .partitions
            .get(&format!("boot{suffix}"))
            .is_some_and(|b| has_marker(b));
        self.mode = Mode::System {
            authorized: !self.faults.boots_unauthorized,
        };
        self.rooted = rooted;
        if rooted {
            self.magisk_version = Some("30.7".into());
            self.service_running = true;
            let n = self
                .bootsvc_lines
                .iter()
                .filter(|l| l.starts_with("boot "))
                .count()
                + 1;
            self.bootsvc_lines.push(format!(
                "boot 2026-10-26T04:{:02}:00 uid=0 ctx=u:r:magisk:s0",
                n % 60
            ));
        } else {
            self.magisk_version = None;
            self.service_running = false;
        }
    }

    fn adb_reachable(&self) -> Result<(), ToolError> {
        match self.mode {
            Mode::System { authorized: true } => Ok(()),
            Mode::System { authorized: false } => Err(adb_err("device unauthorized.")),
            _ => Err(adb_err("no devices/emulators found")),
        }
    }
}

/// If `c` is `PATCH_CMD` with any single-word input handed to `boot_patch.sh`, that input.
fn patch_input(c: &str) -> Option<&str> {
    const CALL: &str = "sh boot_patch.sh ";
    let at = PATCH_CMD.find(CALL)? + CALL.len();
    let post = PATCH_CMD[at..].strip_prefix(crate::magisk::PATCH_INPUT)?;
    let mid = c.strip_prefix(&PATCH_CMD[..at])?.strip_suffix(post)?;
    (!mid.is_empty() && !mid.contains(char::is_whitespace)).then_some(mid)
}

fn has_marker(b: &[u8]) -> bool {
    b.windows(PATCH_MARKER.len()).any(|w| w == PATCH_MARKER)
}

fn adb_err(msg: &str) -> ToolError {
    ToolError::Exit {
        tool: "adb".into(),
        code: 1,
        stderr: format!("error: {msg}"),
    }
}

fn edl_err(msg: &str) -> ToolError {
    ToolError::Exit {
        tool: "edl".into(),
        code: 1,
        stderr: msg.into(),
    }
}

fn edl_timeout() -> ToolError {
    ToolError::Timeout {
        tool: "edl".into(),
        secs: 60,
    }
}

fn out(code: i32, stdout: &str, stderr: &str) -> CmdOutput {
    CmdOutput {
        code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

fn getprop_text(props: &BTreeMap<String, String>) -> String {
    props
        .iter()
        .map(|(k, v)| format!("[{k}]: [{v}]\n"))
        .collect()
}

/// A shared handle to the simulated tablet.
#[derive(Clone, Default)]
pub struct FakeWorld(Arc<Mutex<World>>);

impl FakeWorld {
    pub fn new(w: World) -> Self {
        Self(Arc::new(Mutex::new(w)))
    }
    pub fn lock(&self) -> MutexGuard<'_, World> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn calls(&self) -> Vec<Call> {
        self.lock().calls.clone()
    }
}

pub fn fake_boot_image(size: usize, seed: u8) -> Vec<u8> {
    let mut v = b"ANDROID!".to_vec();
    v.truncate(size);
    for i in v.len()..size {
        v.push(
            seed.wrapping_mul(131)
                .wrapping_add((i.wrapping_mul(31) ^ (i >> 3)) as u8),
        );
    }
    v
}

/// Writes a zip holding the seven `APK_PAYLOAD` entries; returns its SHA-256 (hex).
pub fn write_fixture_apk(path: &Path) -> std::io::Result<String> {
    let io = |e: zip::result::ZipError| std::io::Error::other(e.to_string());
    let mut z = zip::ZipWriter::new(std::fs::File::create(path)?);
    let opts = zip::write::SimpleFileOptions::default();
    for (entry, name) in APK_PAYLOAD {
        z.start_file(entry, opts).map_err(io)?;
        z.write_all(format!("fixture {name}").as_bytes())?;
    }
    z.finish().map_err(io)?;
    crate::image::sha256_file(path)
}

pub struct FakeAdb {
    world: FakeWorld,
}

impl FakeAdb {
    pub fn new(world: FakeWorld) -> Self {
        Self { world }
    }
}

fn sha_hex(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

impl Adb for FakeAdb {
    fn devices(&self) -> Result<Vec<AdbDevice>, ToolError> {
        let mut w = self.world.lock();
        w.calls.push(Call::Devices);
        if w.pending_boot && w.mode == Mode::Off && w.faults.boot_returns_to_edl {
            w.pending_boot = false;
            w.mode = Mode::Edl;
        } else if w.pending_boot && w.mode == Mode::Off && !w.faults.boot_hangs {
            w.boot();
        }
        let mut v = Vec::new();
        match w.mode {
            Mode::System { authorized } => v.push(AdbDevice {
                serial: w.serial.clone(),
                state: if authorized {
                    AdbState::Device
                } else {
                    AdbState::Unauthorized
                },
            }),
            Mode::Off | Mode::Edl => {}
        }
        for (s, st, _) in &w.other_devices {
            v.push(AdbDevice {
                serial: s.clone(),
                state: *st,
            });
        }
        Ok(v)
    }

    fn shell(&self, serial: &str, cmd: &str, _timeout: Duration) -> Result<CmdOutput, ToolError> {
        if cmd == PATCH_CMD {
            nap(self.world.lock().delays.patch);
        }
        let mut w = self.world.lock();
        w.calls.push(Call::Shell {
            serial: serial.into(),
            cmd: cmd.into(),
        });
        if serial != w.serial {
            let Some((_, st, props)) = w.other_devices.iter().find(|(s, ..)| s == serial) else {
                return Err(adb_err(&format!("device '{serial}' not found")));
            };
            if *st != AdbState::Device {
                return Err(adb_err("device unauthorized."));
            }
            return Ok(match cmd {
                "getprop" => out(0, &getprop_text(props), ""),
                _ => out(127, "", "/system/bin/sh: not found"),
            });
        }
        w.adb_reachable()?;
        Ok(match cmd {
            "getprop" => out(0, &getprop_text(&w.props), ""),
            "dumpsys battery" => out(
                0,
                &format!(
                    "Current Battery Service state:\n  AC powered: false\n  USB powered: true\n  Wireless powered: false\n  status: 2\n  health: 2\n  present: true\n  level: {}\n  scale: 100\n  voltage: 4100\n  temperature: 270\n  technology: Li-poly\n",
                    w.battery
                ),
                "",
            ),
            "/debug_ramdisk/magisk -c" => {
                if w.rooted {
                    let v = w.magisk_version.clone().unwrap_or_else(|| "30.7".into());
                    out(0, &format!("{v}:MAGISK:R\n"), "")
                } else {
                    out(
                        127,
                        "",
                        "/system/bin/sh: /debug_ramdisk/magisk: inaccessible or not found",
                    )
                }
            }
            "cat /data/local/tmp/bootsvc.log" => {
                if w.faults.bootsvc_log_errors > 0 {
                    w.faults.bootsvc_log_errors -= 1;
                    return Err(adb_err("closed"));
                }
                if w.bootsvc_lines.is_empty() {
                    out(
                        1,
                        "",
                        "cat: /data/local/tmp/bootsvc.log: No such file or directory",
                    )
                } else {
                    out(0, &format!("{}\n", w.bootsvc_lines.join("\n")), "")
                }
            }
            "pgrep -f /data/adb/service.sh" => {
                if w.service_running {
                    out(0, "1234\n", "")
                } else {
                    out(1, "", "")
                }
            }
            c if c == MGK_RESET_CMD => {
                w.files.retain(|k, _| !k.starts_with(MGK_DIR));
                out(0, "", "")
            }
            c if patch_input(c).is_some() => {
                let input = patch_input(c).unwrap_or_default().to_string();
                let in_path = format!("{MGK_DIR}/{input}");
                let backup = format!("{MGK_DIR}/stock_boot.img");
                // Magisk 30.7 boot_patch.sh, stock image: `cat $BOOTIMAGE > stock_boot.img`
                // in its cwd. When the input IS stock_boot.img the redirection truncates it
                // before cat reads it, so the later repack mmaps an empty file and fails.
                let stock = w.files.get(&in_path).cloned().map(|img| {
                    if has_marker(&img) {
                        return img;
                    }
                    let copy = if in_path == backup { Vec::new() } else { img };
                    w.files.insert(backup.clone(), copy.clone());
                    copy
                });
                match stock {
                    _ if w.faults.patch_fails => out(1, "", "boot_patch.sh: patch failed"),
                    None => out(1, "", &format!("{input}: No such file or directory")),
                    Some(img) if img.is_empty() => out(
                        1,
                        "",
                        &format!(
                            "mmap '{input}': Invalid argument\n! Unable to repack boot image\n"
                        ),
                    ),
                    Some(mut img) => {
                        if !w.faults.patch_drops_marker {
                            // Overwrite the tail so the patched image keeps its size.
                            if img.len() >= PATCH_MARKER.len() {
                                let at = img.len() - PATCH_MARKER.len();
                                img[at..].copy_from_slice(PATCH_MARKER);
                            } else {
                                img.extend_from_slice(PATCH_MARKER);
                            }
                        }
                        w.files.insert(format!("{MGK_DIR}/patched-boot.img"), img);
                        out(0, "- Repacking boot image\n", "")
                    }
                }
            }
            c if c == CHECK_PREP_CMD || c == CHECK_CLEANUP_CMD => {
                w.files.retain(|k, _| !k.starts_with(CHECK_DIR));
                out(0, "", "")
            }
            c if c == STOCK_CHECK_CMD || c == HOOK_CHECK_CMD => {
                let stock = c == STOCK_CHECK_CMD;
                let marked = w
                    .files
                    .get(&format!("{CHECK_DIR}/boot.img"))
                    .is_some_and(|b| has_marker(b));
                let default = match (stock, marked) {
                    (true, m) => i32::from(m),
                    (false, m) => i32::from(!m),
                };
                let over = if stock {
                    w.faults.stock_check_exit
                } else {
                    w.faults.hook_check_exit
                };
                out(over.unwrap_or(default), "", "")
            }
            _ => out(127, "", "/system/bin/sh: not found"),
        })
    }

    fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), ToolError> {
        let mut w = self.world.lock();
        w.calls.push(Call::Push {
            local_name: local
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            remote: remote.into(),
        });
        if serial != w.serial {
            return Err(adb_err(&format!("device '{serial}' not found")));
        }
        w.adb_reachable()?;
        let bytes = std::fs::read(local)
            .map_err(|e| adb_err(&format!("cannot stat '{}': {e}", local.display())))?;
        w.files.insert(remote.into(), bytes);
        Ok(())
    }

    fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<(), ToolError> {
        let mut w = self.world.lock();
        w.calls.push(Call::Pull {
            remote: remote.into(),
        });
        if serial != w.serial {
            return Err(adb_err(&format!("device '{serial}' not found")));
        }
        w.adb_reachable()?;
        let Some(bytes) = w.files.get(remote) else {
            return Err(adb_err(&format!("remote object '{remote}' does not exist")));
        };
        std::fs::write(local, bytes).map_err(|e| adb_err(&e.to_string()))
    }

    fn reboot(&self, serial: &str, target: RebootTarget) -> Result<(), ToolError> {
        if target == RebootTarget::System {
            nap(self.world.lock().delays.reset_boot);
        }
        let mut w = self.world.lock();
        w.calls.push(Call::Reboot {
            serial: serial.into(),
            target,
        });
        if serial != w.serial {
            return Err(adb_err(&format!("device '{serial}' not found")));
        }
        w.adb_reachable()?;
        match target {
            RebootTarget::System => {
                w.mode = Mode::Off;
                w.pending_boot = true;
            }
            RebootTarget::Edl => w.mode = Mode::Edl,
        }
        Ok(())
    }

    fn kill_server(&self) {}
}

pub struct FakeEdl {
    world: FakeWorld,
}

impl FakeEdl {
    pub fn new(world: FakeWorld) -> Self {
        Self { world }
    }
}

impl Edl for FakeEdl {
    fn read(
        &self,
        part: &str,
        out: &Path,
        _cancel: &CancelToken,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), ToolError> {
        let (bytes, delay) = {
            let mut w = self.world.lock();
            w.calls.push(Call::EdlRead { part: part.into() });
            if w.mode != Mode::Edl {
                return Err(edl_err("no Qualcomm device in EDL mode"));
            }
            if w.faults.edl_never_appears {
                return Err(edl_timeout());
            }
            if w.faults.read_fails_on.as_deref() == Some(part) {
                return Err(ToolError::Timeout {
                    tool: "edl".into(),
                    secs: 600,
                });
            }
            let n = {
                let c = w.read_counts.entry(part.into()).or_insert(0);
                *c += 1;
                *c
            };
            let mut b = w
                .partitions
                .get(part)
                .cloned()
                .ok_or_else(|| edl_err(&format!("partition {part} not found")))?;
            if w.faults.verify_read_differs && n >= 2 && !b.is_empty() {
                let mid = b.len() / 2;
                b[mid] = b[mid].wrapping_add((n - 1) as u8);
            }
            (b, w.delays.read)
        };
        pace(delay, bytes.len() as u64, progress);
        std::fs::write(out, &bytes).map_err(|e| edl_err(&e.to_string()))?;
        progress(bytes.len() as u64, bytes.len() as u64);
        Ok(())
    }

    fn write(&self, part: &str, img: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError> {
        let mut bytes = std::fs::read(img).map_err(|e| edl_err(&e.to_string()))?;
        let len = bytes.len() as u64;
        let delay = {
            let mut w = self.world.lock();
            w.calls.push(Call::EdlWrite {
                part: part.into(),
                sha: sha_hex(&bytes),
            });
            if w.faults.edl_never_appears {
                return Err(edl_timeout());
            }
            if w.mode != Mode::Edl {
                return Err(edl_err("no Qualcomm device in EDL mode"));
            }
            if w.faults.write_errors > 0 {
                w.faults.write_errors -= 1;
                return Err(edl_timeout());
            }
            if w.faults.write_corrupts_times > 0 {
                w.faults.write_corrupts_times -= 1;
                if !bytes.is_empty() {
                    let mid = bytes.len() / 2;
                    bytes[mid] ^= 0xFF;
                }
            }
            w.partitions.insert(part.into(), bytes);
            w.delays.write
        };
        pace(delay, len, progress);
        progress(len, len);
        Ok(())
    }

    fn reset(&self) -> Result<(), ToolError> {
        nap(self.world.lock().delays.reset_boot);
        let mut w = self.world.lock();
        w.calls.push(Call::EdlReset);
        if w.faults.edl_never_appears {
            return Err(edl_timeout());
        }
        if w.mode != Mode::Edl {
            return Err(edl_err("no Qualcomm device in EDL mode"));
        }
        if w.faults.reset_errors > 0 {
            w.faults.reset_errors = w.faults.reset_errors.saturating_sub(1);
            return Err(edl_err(
                "Usage:\n    edl.py reset [--resetmode=mode] [--loader=filename]",
            ));
        }
        if w.faults.reset_ignored > 0 {
            w.faults.reset_ignored = w.faults.reset_ignored.saturating_sub(1);
            return Ok(());
        }
        w.mode = Mode::Off;
        w.pending_boot = true;
        Ok(())
    }
}

pub struct FakeUsb {
    world: FakeWorld,
}

impl FakeUsb {
    pub fn new(world: FakeWorld) -> Self {
        Self { world }
    }
}

impl UsbProbe for FakeUsb {
    fn edl_device(&self) -> Option<EdlUsbInfo> {
        let mut w = self.world.lock();
        if let (Mode::Off, Some(after)) = (w.mode, w.faults.edl_appears_after) {
            let started = *w.edl_poll_started.get_or_insert_with(Instant::now);
            if started.elapsed() >= after {
                w.mode = Mode::Edl;
                w.faults.edl_appears_after = None;
            }
        }
        (w.mode == Mode::Edl && !w.faults.edl_never_appears).then_some(EdlUsbInfo { driver: None })
    }
}

/// Fake host setup: records installs on the world's call log and clears the need.
pub struct FakeHost {
    world: FakeWorld,
    needs_driver: std::sync::atomic::AtomicBool,
    needs_udev: std::sync::atomic::AtomicBool,
}

impl FakeHost {
    pub fn new(world: FakeWorld) -> Self {
        Self {
            world,
            needs_driver: Default::default(),
            needs_udev: Default::default(),
        }
    }
    pub fn set_needs_driver(&self, v: bool) {
        self.needs_driver
            .store(v, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn set_needs_udev(&self, v: bool) {
        self.needs_udev
            .store(v, std::sync::atomic::Ordering::SeqCst);
    }
}

impl crate::host::setup::HostSetup for FakeHost {
    fn needs_edl_driver(&self, _usb: &EdlUsbInfo) -> bool {
        self.needs_driver.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn install_edl_driver(&self) -> Result<(), crate::ipc::FlowError> {
        self.world.lock().calls.push(Call::DriverInstall);
        self.set_needs_driver(false);
        Ok(())
    }
    fn needs_udev(&self) -> bool {
        self.needs_udev.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn install_udev(&self) -> Result<(), crate::ipc::FlowError> {
        self.world.lock().calls.push(Call::UdevInstall);
        self.set_needs_udev(false);
        Ok(())
    }
}

/// Fake keep-awake: counts holds and currently-active guards.
#[derive(Default)]
pub struct FakeAwake {
    holds: std::sync::atomic::AtomicUsize,
    active: Arc<std::sync::atomic::AtomicUsize>,
}

struct FakeAwakeGuard(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for FakeAwakeGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl FakeAwake {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn holds(&self) -> usize {
        self.holds.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn active(&self) -> usize {
        self.active.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl crate::host::keep_awake::KeepAwake for FakeAwake {
    fn hold(&self, _why: &str) -> Box<dyn std::any::Any> {
        self.holds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.active
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::new(FakeAwakeGuard(self.active.clone()))
    }
}
