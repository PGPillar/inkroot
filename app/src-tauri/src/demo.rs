//! Demo mode (`INKROOT_DEMO=1`): every flow runs against a simulated tablet at realistic
//! speed. Used for development, the UI, screenshots and the CI smoke test. Nothing here
//! touches a real device, the real settings file or the real backup folder.

use crate::backup::{BackupLibrary, Manifest, StockEvidence};
use crate::clock::SystemClock;
use crate::events::Emitter;
use crate::fakes::{
    Delays, FakeAdb, FakeAwake, FakeEdl, FakeHost, FakeUsb, FakeWorld, Faults, Mode, PATCH_MARKER,
    World, fake_boot_image, write_fixture_apk,
};
use crate::flows::Deps;
use crate::ipc::{BackupFile, BackupSource, Slot};
use crate::logs::LogStore;
use crate::profile::{DeviceProfile, KnownStockBoot, builtin_profiles};
use crate::settings::SettingsStore;
use crate::tools::{Timeouts, ToolPaths};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub const DEMO_SERIAL: &str = "NMX1234567";
/// Demo images are tiny so a run is instant on disk; the delays supply the realism.
const DEMO_BOOT_SIZE: u64 = 64 * 1024;
const SLOT: Slot = Slot::B;

/// Realistic base times, multiplied by the speed factor.
const READ: Duration = Duration::from_secs(4);
const WRITE: Duration = Duration::from_secs(6);
const RESET_BOOT: Duration = Duration::from_secs(5);
const PATCH: Duration = Duration::from_secs(3);
const BRICKED_EDL_AFTER: Duration = Duration::from_secs(3);

pub fn demo_enabled() -> bool {
    std::env::var("INKROOT_DEMO").is_ok_and(|v| v == "1")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoScenario {
    /// Stock, unrooted tablet, no backups.
    Fresh,
    /// Rooted tablet with a verified stock backup.
    Rooted,
    /// Tablet that won't start (off, then EDL), with a verified stock backup.
    Bricked,
    /// Rooted tablet and no backup.
    RootedNoBackup,
}

impl DemoScenario {
    /// `INKROOT_DEMO_SCENARIO`: `fresh` (default) | `rooted` | `bricked` | `rooted-no-backup`.
    pub fn from_env() -> Self {
        match std::env::var("INKROOT_DEMO_SCENARIO").as_deref() {
            Ok("rooted") => Self::Rooted,
            Ok("bricked") => Self::Bricked,
            Ok("rooted-no-backup") => Self::RootedNoBackup,
            _ => Self::Fresh,
        }
    }
}

/// The built-in Note Max profile with small images, and the demo's stock boot image listed
/// as a known original (so importing that file counts as stock).
pub fn demo_profile() -> DeviceProfile {
    let mut p = builtin_profiles().remove(0);
    p.boot_size = DEMO_BOOT_SIZE;
    let display_id = p.firmware[0].display_id.clone();
    p.known_stock_boot_sha256 = vec![KnownStockBoot {
        sha256: sha_hex(&stock_image(SLOT)),
        slot: SLOT,
        display_id,
    }];
    p
}

fn sha_hex(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

fn stock_image(slot: Slot) -> Vec<u8> {
    let seed = if slot == Slot::A { 1 } else { 2 };
    fake_boot_image(DEMO_BOOT_SIZE as usize, seed)
}

fn patched_image(slot: Slot) -> Vec<u8> {
    let mut img = stock_image(slot);
    let at = img.len() - PATCH_MARKER.len();
    img[at..].copy_from_slice(PATCH_MARKER);
    img
}

fn demo_world(scenario: DemoScenario, profile: &DeviceProfile, speed: f32) -> FakeWorld {
    let fw = &profile.firmware[0];
    let mut w = World {
        serial: DEMO_SERIAL.into(),
        battery: 80,
        mode: Mode::System { authorized: true },
        ..World::default()
    };
    for (k, v) in [
        ("ro.product.model", profile.matcher.model.as_str()),
        ("ro.product.device", profile.matcher.device.as_str()),
        (
            "ro.product.manufacturer",
            profile.matcher.manufacturer.as_str(),
        ),
        ("ro.build.id", fw.build_id.as_str()),
        ("ro.build.display.id", fw.display_id.as_str()),
        ("ro.build.version.incremental", fw.incremental.as_str()),
        ("ro.build.fingerprint", fw.fingerprint.as_str()),
        ("ro.board.platform", "msmnile"),
        ("ro.boot.slot_suffix", "_b"),
        ("ro.serialno", DEMO_SERIAL),
    ] {
        w.props.insert(k.into(), v.into());
    }
    w.partitions.insert("boot_a".into(), stock_image(Slot::A));
    w.partitions.insert("boot_b".into(), stock_image(Slot::B));
    w.partitions.insert("vbmeta_a".into(), vec![0x11; 4096]);
    w.partitions.insert("vbmeta_b".into(), vec![0x22; 4096]);
    if matches!(
        scenario,
        DemoScenario::Rooted | DemoScenario::RootedNoBackup
    ) {
        w.partitions.insert("boot_b".into(), patched_image(SLOT));
        w.rooted = true;
        w.magisk_version = Some("30.7".into());
        w.service_running = true;
        w.bootsvc_lines
            .push("boot 2026-10-25T10:00:00 uid=0 ctx=u:r:magisk:s0".into());
    }
    let mut delays = Delays {
        read: READ,
        write: WRITE,
        reset_boot: RESET_BOOT,
        patch: PATCH,
    };
    let scale = |d: Duration| {
        if speed > 0.0 {
            d.mul_f32(speed)
        } else {
            Duration::ZERO
        }
    };
    delays.read = scale(delays.read);
    delays.write = scale(delays.write);
    delays.reset_boot = scale(delays.reset_boot);
    delays.patch = scale(delays.patch);
    w.delays = delays;
    if scenario == DemoScenario::Bricked {
        w.mode = Mode::Off;
        w.faults = Faults {
            edl_appears_after: Some(scale(BRICKED_EDL_AFTER)),
            ..Faults::default()
        };
    }
    FakeWorld::new(w)
}

/// Writes a verified stock backup of the demo tablet into `library`.
fn seed_backup(library: &BackupLibrary, profile: &DeviceProfile) {
    let fw = &profile.firmware[0];
    let (_, dir) = library
        .create_dir(
            DEMO_SERIAL,
            chrono::Local::now() - chrono::Duration::days(2),
        )
        .expect("create demo backup folder");
    let images = [
        ("boot_a.img", stock_image(Slot::A)),
        ("boot_b.img", stock_image(Slot::B)),
        ("vbmeta_a.img", vec![0x11; 4096]),
        ("vbmeta_b.img", vec![0x22; 4096]),
    ];
    let mut files = vec![];
    for (name, bytes) in &images {
        std::fs::write(dir.join(name), bytes).expect("write demo backup image");
        files.push(BackupFile {
            name: (*name).into(),
            size: bytes.len() as u64,
            sha256: sha_hex(bytes),
        });
    }
    let manifest = Manifest {
        manifest_version: 1,
        inkroot_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: (chrono::Local::now() - chrono::Duration::days(2)).to_rfc3339(),
        serial: DEMO_SERIAL.into(),
        model: profile.matcher.model.clone(),
        profile_id: Some(profile.id.clone()),
        firmware: crate::backup::FirmwareInfo {
            build_id: fw.build_id.clone(),
            display_id: fw.display_id.clone(),
            incremental: fw.incremental.clone(),
            fingerprint: fw.fingerprint.clone(),
        },
        slot: SLOT,
        stock: true,
        stock_evidence: StockEvidence::KnownHash,
        verified: true,
        source: BackupSource::Inkroot,
        files,
        patched: None,
        imported_from: None,
    };
    library
        .write_manifest(&dir, &manifest)
        .expect("write demo manifest");
}

/// Stub files for every tool the flows require, plus a fixture Magisk APK.
fn stub_tools(root: &Path, profile: &DeviceProfile) -> (ToolPaths, String) {
    let paths = ToolPaths {
        root: root.to_path_buf(),
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
        std::fs::create_dir_all(f.parent().expect("tool path has a parent"))
            .expect("create demo tools dir");
        std::fs::write(f, b"stub").expect("write demo tool stub");
    }
    let apk = paths.magisk_apk(&profile.magisk.version);
    std::fs::create_dir_all(apk.parent().expect("apk path has a parent"))
        .expect("create demo tools dir");
    let sha = write_fixture_apk(&apk).expect("write fixture apk");
    (paths, sha)
}

/// Everything a flow needs, backed by a simulated tablet. `speed` scales the realistic
/// delays (1.0 = real pace, 0.0 = instant).
///
/// Each call makes a fresh folder `<temp>/inkroot-demo/<pid>-<uuid>/` holding the backup
/// `library`, stub `tools` and `config`. `poller` is `None`; the app wires a real one over
/// the same fake adb/usb.
///
/// `Deps::magisk_apk_sha256` here is the SHA-256 of a generated fixture APK, NOT the pinned
/// Magisk hash. This is the only place in shipped code where a non-pinned hash is used; it
/// lets the demo run the real extraction checks without bundling Magisk.
pub fn demo_deps(scenario: DemoScenario, emitter: Arc<dyn Emitter>, speed: f32) -> Deps {
    let profile = demo_profile();
    let root = std::env::temp_dir().join("inkroot-demo").join(format!(
        "{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let (tools, magisk_apk_sha256) = stub_tools(&root.join("tools"), &profile);
    let library = BackupLibrary::new(root.join("library"));
    std::fs::create_dir_all(&library.root).expect("create demo library");
    if matches!(scenario, DemoScenario::Rooted | DemoScenario::Bricked) {
        seed_backup(&library, &profile);
    }
    let world = demo_world(scenario, &profile, speed);
    let clock = Arc::new(SystemClock::new());
    Deps {
        adb: Arc::new(FakeAdb::new(world.clone())),
        edl: Arc::new(FakeEdl::new(world.clone())),
        usb: Arc::new(FakeUsb::new(world.clone())),
        host: Arc::new(FakeHost::new(world.clone())),
        awake: Arc::new(FakeAwake::new()),
        clock: clock.clone(),
        emitter: emitter.clone(),
        log: Arc::new(LogStore::new(emitter, clock)),
        library,
        settings: Arc::new(SettingsStore::new(
            root.join("config").join("settings.json"),
        )),
        profiles: vec![profile],
        tools,
        timeouts: Timeouts::default(),
        poller: None,
        magisk_apk_sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_parsing() {
        let _g = crate::env_lock();
        unsafe {
            std::env::remove_var("INKROOT_DEMO");
            std::env::remove_var("INKROOT_DEMO_SCENARIO");
        }
        assert!(!demo_enabled());
        assert_eq!(DemoScenario::from_env(), DemoScenario::Fresh);
        unsafe { std::env::set_var("INKROOT_DEMO", "1") };
        assert!(demo_enabled());
        unsafe { std::env::set_var("INKROOT_DEMO", "true") };
        assert!(!demo_enabled());
        for (v, want) in [
            ("fresh", DemoScenario::Fresh),
            ("rooted", DemoScenario::Rooted),
            ("bricked", DemoScenario::Bricked),
            ("rooted-no-backup", DemoScenario::RootedNoBackup),
            ("nonsense", DemoScenario::Fresh),
        ] {
            unsafe { std::env::set_var("INKROOT_DEMO_SCENARIO", v) };
            assert_eq!(DemoScenario::from_env(), want, "{v}");
        }
        unsafe {
            std::env::remove_var("INKROOT_DEMO");
            std::env::remove_var("INKROOT_DEMO_SCENARIO");
        }
    }

    #[test]
    fn demo_profile_is_valid_and_lists_demo_stock() {
        let p = demo_profile();
        crate::profile::validate(&p).unwrap();
        let fw = &p.firmware[0].display_id;
        assert_eq!(p.known_stock_boot_sha256.len(), 1);
        assert!(p.known_stock(&sha_hex(&stock_image(SLOT)), SLOT, fw));
        assert!(!p.known_stock(&sha_hex(&stock_image(SLOT)), Slot::A, fw));
        assert!(!p.known_stock(&sha_hex(&patched_image(SLOT)), SLOT, fw));
    }
}
