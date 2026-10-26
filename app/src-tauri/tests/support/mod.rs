#![allow(dead_code)]
//! Shared helpers for integration tests.

use inkroot_lib::events::Emitter;
use inkroot_lib::fakes::{FakeWorld, Mode, World, fake_boot_image, write_fixture_apk};
use inkroot_lib::ipc::{DeviceStatus, FlowState, LogLine, Progress};
use inkroot_lib::profile::{DeviceProfile, builtin_profiles};
use inkroot_lib::tools::ToolPaths;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const NOTE_MAX_SERIAL: &str = "NMX1234567";

/// The Note Max profile with a small boot size and no known-stock hashes.
pub fn test_profile() -> DeviceProfile {
    let mut p = builtin_profiles().remove(0);
    p.boot_size = 4096;
    p.known_stock_boot_sha256 = vec![];
    p
}

/// A stock Note Max: slot `_b`, battery 80, authorized, 4 KiB stock images.
pub fn stock_world() -> FakeWorld {
    let mut w = World {
        serial: NOTE_MAX_SERIAL.into(),
        battery: 80,
        mode: Mode::System { authorized: true },
        ..World::default()
    };
    for (k, v) in [
        ("ro.product.model", "NoteMax"),
        ("ro.product.device", "NoteMax"),
        ("ro.product.manufacturer", "ONYX"),
        ("ro.build.id", "TKQ1.230721.002"),
        ("ro.build.display.id", "TKQ1.230721.002"),
        ("ro.build.version.incremental", "226"),
        (
            "ro.build.fingerprint",
            "ONYX/TabBoox/TabBoox:13/TKQ1.230615.001/GV2.027.SQ83A:user/release-keys",
        ),
        ("ro.board.platform", "msmnile"),
        ("ro.boot.slot_suffix", "_b"),
        ("ro.serialno", NOTE_MAX_SERIAL),
    ] {
        w.props.insert(k.into(), v.into());
    }
    w.partitions
        .insert("boot_a".into(), fake_boot_image(4096, 1));
    w.partitions
        .insert("boot_b".into(), fake_boot_image(4096, 2));
    w.partitions.insert("vbmeta_a".into(), vec![0x11; 4096]);
    w.partitions.insert("vbmeta_b".into(), vec![0x22; 4096]);
    FakeWorld::new(w)
}

#[derive(Default)]
struct Rec {
    devices: Vec<DeviceStatus>,
    states: Vec<FlowState>,
    progress: Vec<Progress>,
    logs: Vec<LogLine>,
}

#[derive(Clone, Default)]
pub struct RecordingEmitter(Arc<Mutex<Rec>>);

impl RecordingEmitter {
    fn rec(&self) -> std::sync::MutexGuard<'_, Rec> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn devices(&self) -> Vec<DeviceStatus> {
        self.rec().devices.clone()
    }
    pub fn states(&self) -> Vec<FlowState> {
        self.rec().states.clone()
    }
    pub fn progress(&self) -> Vec<Progress> {
        self.rec().progress.clone()
    }
    pub fn logs(&self) -> Vec<LogLine> {
        self.rec().logs.clone()
    }
}

impl Emitter for RecordingEmitter {
    fn device(&self, s: &DeviceStatus) {
        self.rec().devices.push(s.clone());
    }
    fn flow_state(&self, s: &FlowState) {
        self.rec().states.push(s.clone());
    }
    fn progress(&self, p: &Progress) {
        self.rec().progress.push(p.clone());
    }
    fn log(&self, l: &LogLine) {
        self.rec().logs.push(l.clone());
    }
}

/// Writes a Magisk-shaped fixture APK; returns its path and SHA-256.
pub fn fixture_apk(dir: &Path) -> (PathBuf, String) {
    let p = dir.join("fixture.apk");
    let sha = write_fixture_apk(&p).expect("write fixture apk");
    (p, sha)
}

/// Creates a tool tree under `dir` with stub files for every required tool.
pub fn fixture_tools(dir: &Path, profile: &DeviceProfile) -> (ToolPaths, String) {
    let paths = ToolPaths {
        root: dir.to_path_buf(),
    };
    let stubs = [
        paths.adb(),
        paths.edl(),
        paths.loader(&profile.edl.loader),
        paths.wdi_simple(),
        paths.boot_service("init.bootsvc.rc"),
        paths.boot_service("bootsvc.sh"),
        paths.boot_service("inkroot-default-service.sh"),
    ];
    for f in &stubs {
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, b"stub").unwrap();
    }
    let apk = paths.magisk_apk(&profile.magisk.version);
    std::fs::create_dir_all(apk.parent().unwrap()).unwrap();
    let sha = write_fixture_apk(&apk).expect("write fixture apk");
    (paths, sha)
}

// ---------------------------------------------------------------------------
// Flow harness (Task 9)
// ---------------------------------------------------------------------------

use inkroot_lib::backup::BackupLibrary;
use inkroot_lib::clock::FakeClock;
use inkroot_lib::fakes::{FakeAdb, FakeAwake, FakeEdl, FakeHost, FakeUsb};
use inkroot_lib::flows::{Deps, FlowCtx, FlowEnd, FlowEngine};
use inkroot_lib::ipc::{
    FlowError, FlowKind, FlowOptions, FlowResult, FlowStatus, Prompt, SettingsPatch,
};
use inkroot_lib::logs::LogStore;
use inkroot_lib::settings::{RISK_NOTICE_VERSION, SettingsStore};
use inkroot_lib::tools::Timeouts;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Wall-clock guard for every wait on a flow: a hung flow fails the test instead of hanging it.
pub const FLOW_GUARD: Duration = Duration::from_secs(10);

pub struct Harness {
    pub engine: Arc<FlowEngine>,
    pub world: FakeWorld,
    pub events: RecordingEmitter,
    pub lib_dir: TempDir,
    pub tools_dir: TempDir,
    pub cfg_dir: TempDir,
    pub host: Arc<FakeHost>,
    pub awake: Arc<FakeAwake>,
    pub settings: Arc<SettingsStore>,
    pub clock: FakeClock,
    pub log: Arc<LogStore>,
    pub magisk_apk_sha256: String,
}

/// Default harness: risk notice accepted, no poller.
pub fn harness(world: FakeWorld) -> Harness {
    harness_opts(world, true, |_| {})
}

/// Harness whose settings have NOT accepted the risk notice.
pub fn harness_without_risk(world: FakeWorld) -> Harness {
    harness_opts(world, false, |_| {})
}

/// Full control: `customize` may swap any dependency before the engine is built.
pub fn harness_opts(
    world: FakeWorld,
    accept_risk: bool,
    customize: impl FnOnce(&mut Deps),
) -> Harness {
    let profile = test_profile();
    let lib_dir = tempfile::tempdir().unwrap();
    let tools_dir = tempfile::tempdir().unwrap();
    let cfg_dir = tempfile::tempdir().unwrap();
    let (tools, magisk_apk_sha256) = fixture_tools(tools_dir.path(), &profile);
    let events = RecordingEmitter::default();
    let clock = FakeClock::new(chrono::Local::now());
    let host = Arc::new(FakeHost::new(world.clone()));
    let awake = Arc::new(FakeAwake::new());
    let settings = Arc::new(SettingsStore::new(cfg_dir.path().join("settings.json")));
    if accept_risk {
        settings
            .apply(SettingsPatch {
                accepted_risk_version: Some(Some(RISK_NOTICE_VERSION)),
                ..SettingsPatch::default()
            })
            .unwrap();
    }
    let log = Arc::new(LogStore::new(
        Arc::new(events.clone()),
        Arc::new(clock.clone()),
    ));
    let mut deps = Deps {
        adb: Arc::new(FakeAdb::new(world.clone())),
        edl: Arc::new(FakeEdl::new(world.clone())),
        usb: Arc::new(FakeUsb::new(world.clone())),
        host: host.clone(),
        awake: awake.clone(),
        clock: Arc::new(clock.clone()),
        emitter: Arc::new(events.clone()),
        log: log.clone(),
        library: BackupLibrary::new(lib_dir.path().to_path_buf()),
        settings: settings.clone(),
        profiles: vec![profile],
        tools,
        timeouts: Timeouts::default(),
        poller: None,
        magisk_apk_sha256: magisk_apk_sha256.clone(),
    };
    customize(&mut deps);
    Harness {
        engine: FlowEngine::new(deps),
        world,
        events,
        lib_dir,
        tools_dir,
        cfg_dir,
        host,
        awake,
        settings,
        clock,
        log,
        magisk_apk_sha256,
    }
}

/// The contract literal of a prompt kind (e.g. `"retryWrite"`).
pub fn prompt_literal(p: &Prompt) -> String {
    serde_json::to_value(p.kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

pub fn is_terminal(s: FlowStatus) -> bool {
    matches!(
        s,
        FlowStatus::Done | FlowStatus::Failed | FlowStatus::Cancelled
    )
}

/// The latest emitted snapshot for `flow_id`.
pub fn latest_state(h: &Harness, flow_id: &str) -> Option<FlowState> {
    h.events
        .states()
        .into_iter()
        .rev()
        .find(|s| s.flow_id == flow_id)
}

/// Polls (real time) until `pred` holds for the latest snapshot of `flow_id`; panics after
/// `FLOW_GUARD`.
pub fn wait_state(h: &Harness, flow_id: &str, pred: impl Fn(&FlowState) -> bool) -> FlowState {
    let start = Instant::now();
    loop {
        if let Some(s) = latest_state(h, flow_id)
            && pred(&s)
        {
            return s;
        }
        if start.elapsed() > FLOW_GUARD {
            panic!(
                "flow {flow_id} did not reach the expected state; last: {:?}",
                latest_state(h, flow_id)
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits for a prompt of kind `literal` (and not one of `seen`), returning it.
pub fn wait_prompt(h: &Harness, flow_id: &str, literal: &str) -> Prompt {
    let s = wait_state(h, flow_id, |s| {
        s.status == FlowStatus::WaitingForUser || is_terminal(s.status)
    });
    let p = s
        .prompt
        .clone()
        .unwrap_or_else(|| panic!("expected prompt {literal}, flow ended: {s:?}"));
    assert_eq!(prompt_literal(&p), literal, "unexpected prompt: {s:?}");
    p
}

pub fn wait_terminal(h: &Harness, flow_id: &str) -> FlowState {
    wait_state(h, flow_id, |s| is_terminal(s.status))
}

/// Answers prompts (in order, matched by contract literal) until the flow ends.
pub fn drive(h: &Harness, flow_id: &str, answers: &[(&str, &str)]) -> FlowState {
    let mut answered: Vec<String> = Vec::new();
    let mut next = 0usize;
    loop {
        let s = wait_state(h, flow_id, |s| {
            is_terminal(s.status)
                || (s.status == FlowStatus::WaitingForUser
                    && s.prompt.as_ref().is_some_and(|p| !answered.contains(&p.id)))
        });
        if is_terminal(s.status) {
            assert_eq!(
                next,
                answers.len(),
                "flow ended before all answers were used: {s:?}"
            );
            return s;
        }
        let p = s.prompt.clone().unwrap();
        let lit = prompt_literal(&p);
        let Some((kind, option)) = answers.get(next) else {
            panic!("unexpected prompt {lit} (no answers left): {s:?}");
        };
        assert_eq!(&lit, kind, "unexpected prompt order: {s:?}");
        answered.push(p.id.clone());
        next += 1;
        h.engine.answer(flow_id, &p.id, option);
    }
}

pub fn run_to_end(
    h: &Harness,
    kind: FlowKind,
    opts: FlowOptions,
    answers: &[(&str, &str)],
) -> FlowState {
    let id = h.engine.start(kind, opts).expect("start flow");
    drive(h, &id, answers)
}

pub fn run_flow_with(
    h: &Harness,
    kind: FlowKind,
    opts: FlowOptions,
    flow: impl FnOnce(&FlowCtx) -> Result<FlowResult, FlowEnd> + Send + 'static,
    answers: &[(&str, &str)],
) -> FlowState {
    let id = h
        .engine
        .start_with(kind, opts, Box::new(flow))
        .expect("start flow");
    drive(h, &id, answers)
}

pub fn expect_failed(s: &FlowState) -> &FlowError {
    assert_eq!(s.status, FlowStatus::Failed, "{s:?}");
    s.error.as_ref().expect("failed state carries an error")
}

// ---------------------------------------------------------------------------
// Task 12 helpers (rescue / unroot / reinstall)
// ---------------------------------------------------------------------------

use inkroot_lib::fakes::{Call, PATCH_MARKER};
use inkroot_lib::ipc::BackupSummary;
use inkroot_lib::tools::adb::RebootTarget;

/// Takes a verified stock backup with the Back up flow, then clears the call log.
pub fn seed_stock_backup(h: &Harness) -> BackupSummary {
    let id = h
        .engine
        .start(FlowKind::Backup, FlowOptions::default())
        .expect("start backup");
    let end = drive(h, &id, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let s = end.result.unwrap().backup.unwrap();
    assert!(s.stock && s.verified, "{s:?}");
    h.world.lock().calls.clear();
    s
}

/// A rooted copy of `fake_boot_image(4096, seed)` (marker over the tail).
pub fn patched_boot(seed: u8) -> Vec<u8> {
    let mut img = fake_boot_image(4096, seed);
    let at = img.len() - PATCH_MARKER.len();
    img[at..].copy_from_slice(PATCH_MARKER);
    img
}

/// Roots the simulated tablet in place (active slot `_b`), as if some guide had patched it.
/// `hook_line`: whether the boot-service log already holds a root boot line.
pub fn root_the_world(w: &FakeWorld, hook_line: bool) {
    let mut g = w.lock();
    g.partitions.insert("boot_b".into(), patched_boot(2));
    g.rooted = true;
    g.magisk_version = Some("30.7".into());
    if hook_line {
        g.bootsvc_lines
            .push("boot 2026-10-25T10:00:00 uid=0 ctx=u:r:magisk:s0".into());
        g.service_running = true;
    }
}

pub fn is_adb_call(c: &Call) -> bool {
    matches!(
        c,
        Call::Devices
            | Call::Shell { .. }
            | Call::Push { .. }
            | Call::Pull { .. }
            | Call::Reboot { .. }
    )
}

pub fn is_edl_call(c: &Call) -> bool {
    matches!(
        c,
        Call::EdlRead { .. } | Call::EdlWrite { .. } | Call::EdlReset
    )
}

pub fn is_reboot_edl_call(c: &Call) -> bool {
    matches!(
        c,
        Call::Reboot {
            target: RebootTarget::Edl,
            ..
        }
    )
}

/// Copies backup `id` to a sibling folder `dir_name` (same serial), lets `edit` change the
/// copy's manifest, and returns the new backup id. The copy is dated 2000 (older than any
/// real backup), so `stock_for` would never pick it over the original.
pub fn clone_backup(
    h: &Harness,
    id: &str,
    dir_name: &str,
    edit: impl FnOnce(&mut inkroot_lib::backup::Manifest),
) -> String {
    let lib = &h.engine.deps().library;
    let (src, mut m) = lib.get(id).expect("source backup");
    let dst = src.parent().unwrap().join(dir_name);
    std::fs::create_dir_all(&dst).unwrap();
    for e in std::fs::read_dir(&src).unwrap() {
        let e = e.unwrap();
        if e.file_name() != "manifest.json" {
            std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
        }
    }
    m.created_at = "2000-01-01T00:00:00+00:00".into();
    edit(&mut m);
    lib.write_manifest(&dst, &m).unwrap();
    format!("{}/{dir_name}", m.serial)
}

pub fn with_backup_id(id: &str) -> FlowOptions {
    FlowOptions {
        backup_id: Some(id.to_string()),
        ..FlowOptions::default()
    }
}

/// Explicit backup ids that Root/Unroot must refuse with `noStockBackup`: copies of `id` that
/// differ in one way each (other serial, other firmware, unverified, boot_b.img not listed),
/// plus path-traversal ids. Returns (id, why).
pub fn unusable_backup_ids(h: &Harness, id: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    // The serial in the manifest differs; the folder stays this tablet's.
    clone_backup(h, id, "2000-01-01_0011", |m| {
        m.serial = "OTHER-SERIAL".into()
    });
    out.push((
        format!("{NOTE_MAX_SERIAL}/2000-01-01_0011"),
        "serial mismatch",
    ));
    out.push((
        clone_backup(h, id, "2000-01-01_0012", |m| {
            m.firmware.display_id = "OTHER-FIRMWARE".into()
        }),
        "firmware mismatch",
    ));
    out.push((
        clone_backup(h, id, "2000-01-01_0013", |m| m.verified = false),
        "unverified",
    ));
    out.push((
        clone_backup(h, id, "2000-01-01_0014", |m| {
            m.files.retain(|f| f.name != "boot_b.img")
        }),
        "boot_b.img not listed",
    ));
    out.push(("../x".to_string(), "traversal"));
    out.push((format!("../{id}"), "traversal to a real backup"));
    out.push(("/abs".to_string(), "absolute"));
    out.push((format!("/{id}"), "absolute to a real backup"));
    out
}

// ---------------------------------------------------------------------------
// R44 helpers (imported backups and legacy trust)
// ---------------------------------------------------------------------------

/// The stock world's boot_b as a known original for slot B on its firmware.
pub fn world_boot_b_known() -> inkroot_lib::profile::KnownStockBoot {
    let sha = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(fake_boot_image(
        4096, 2,
    )));
    inkroot_lib::profile::KnownStockBoot {
        sha256: sha,
        slot: inkroot_lib::ipc::Slot::B,
        display_id: "TKQ1.230721.002".into(),
    }
}

/// Imports a copy of the stock world's original boot_b (as `boot_b.img`) with the tablet
/// connected, lets `edit` change the saved manifest, clears the call log and returns the id.
pub fn import_world_boot_b(
    h: &Harness,
    edit: impl FnOnce(&mut inkroot_lib::backup::Manifest),
) -> String {
    let src = tempfile::tempdir().unwrap();
    let p = src.path().join("boot_b.img");
    std::fs::write(&p, fake_boot_image(4096, 2)).unwrap();
    let opts = FlowOptions {
        image_path: Some(p.to_string_lossy().into_owned()),
        ..FlowOptions::default()
    };
    let end = run_to_end(h, FlowKind::ImportBackup, opts, &[]);
    assert_eq!(end.status, FlowStatus::Done, "{end:?}");
    let id = end.result.unwrap().backup.unwrap().id;
    let lib = &h.engine.deps().library;
    let (dir, mut m) = lib.get(&id).unwrap();
    assert_eq!(m.serial, NOTE_MAX_SERIAL);
    edit(&mut m);
    lib.write_manifest(&dir, &m).unwrap();
    h.world.lock().calls.clear();
    id
}

/// Makes an imported manifest look like a legacy "stock" record with `evidence`.
pub fn legacy_stock(
    evidence: inkroot_lib::backup::StockEvidence,
) -> impl FnOnce(&mut inkroot_lib::backup::Manifest) {
    move |m| {
        m.stock = true;
        m.stock_evidence = evidence;
    }
}
