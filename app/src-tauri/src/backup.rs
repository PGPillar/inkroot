//! The backup library: <root>/<serial>/<date>/manifest.json plus image files.

use crate::image::sha256_file;
use crate::ipc::{
    BackupFile, BackupSource, BackupSummary, ErrorCode, FlowError, Slot, SuggestedAction,
};
use crate::profile::DeviceProfile;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareInfo {
    pub build_id: String,
    pub display_id: String,
    pub incremental: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StockEvidence {
    KnownHash,
    MagiskbootTest,
    NotStockRooted,
    NotStockFailedTest,
    Unchecked,
    /// An imported file that isn't a known original for its slot and firmware: Rescue only.
    UnknownImport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchedInfo {
    pub file: String,
    pub sha256: String,
    pub magisk_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub manifest_version: u32,
    pub inkroot_version: String,
    pub created_at: String,
    pub serial: String,
    pub model: String,
    pub profile_id: Option<String>,
    pub firmware: FirmwareInfo,
    pub slot: Slot,
    pub stock: bool,
    pub stock_evidence: StockEvidence,
    pub verified: bool,
    pub source: BackupSource,
    pub files: Vec<BackupFile>,
    pub patched: Option<PatchedInfo>,
    pub imported_from: Option<String>,
}

impl Manifest {
    /// Whether Root/Unroot may treat this backup as an original under `profile`. Inkroot's own
    /// backups count when marked stock. An imported file counts only if it was matched by a
    /// known hash **and** the profile's known-stock list still lists that exact
    /// {sha256, slot, displayId}: no other (legacy) evidence is trusted for imports.
    pub fn trusted_stock(&self, profile: &DeviceProfile) -> bool {
        if !self.stock {
            return false;
        }
        if self.source != BackupSource::Imported {
            return true;
        }
        self.stock_evidence == StockEvidence::KnownHash
            && self
                .file(&format!("{}.img", self.slot.boot()))
                .is_some_and(|f| {
                    profile.known_stock(&f.sha256, self.slot, &self.firmware.display_id)
                })
    }

    pub fn to_summary(&self, id: &str, dir: &Path) -> BackupSummary {
        BackupSummary {
            id: id.to_string(),
            serial: self.serial.clone(),
            created_at: self.created_at.clone(),
            model: self.model.clone(),
            firmware: self.firmware.display_id.clone(),
            slot: self.slot,
            stock: self.stock,
            verified: self.verified,
            source: self.source,
            path: dir.to_string_lossy().into_owned(),
            files: self.files.clone(),
        }
    }

    pub fn file(&self, name: &str) -> Option<&BackupFile> {
        self.files.iter().find(|f| f.name == name)
    }
}

fn internal(message: &str, detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::Internal,
        message: message.into(),
        detail,
        suggested_action: Some(SuggestedAction::Retry),
    }
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn subdirs(p: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(p) else {
        return vec![];
    };
    rd.flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .collect()
}

/// The folder name for a device serial: characters that aren't allowed in Windows paths
/// (e.g. the `:` of a Wi-Fi ADB `ip:port` serial, or `/` and `\\`), control characters and a
/// trailing dot or space become `_`. Manifests keep the raw serial.
pub fn serial_dir_name(serial: &str) -> String {
    let mut s: String = serial
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.is_empty() {
        return "_".into();
    }
    // Windows drops trailing dots and spaces (and "." / ".." aren't folder names at all).
    let keep = s.trim_end_matches(['.', ' ']).len();
    let trailing = s.len() - keep;
    s.truncate(keep);
    s.push_str(&"_".repeat(trailing));
    s
}

pub struct BackupLibrary {
    pub root: PathBuf,
}

impl BackupLibrary {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> PathBuf {
        if let Some(v) = std::env::var_os("INKROOT_BACKUP_DIR") {
            return PathBuf::from(v);
        }
        dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Inkroot")
    }

    pub fn create_dir(
        &self,
        serial: &str,
        now: DateTime<Local>,
    ) -> Result<(String, PathBuf), FlowError> {
        const MSG: &str = "Inkroot can't save files in your Documents folder. Check that it exists and isn't read-only.";
        let fail = |e: std::io::Error| internal(MSG, e.to_string());
        let folder = serial_dir_name(serial);
        let serial_dir = self.root.join(&folder);
        std::fs::create_dir_all(&serial_dir).map_err(fail)?;
        let base = now.format("%Y-%m-%d_%H%M").to_string();
        let mut n = 1u32;
        let (name, path) = loop {
            let name = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let path = serial_dir.join(&name);
            match std::fs::create_dir(&path) {
                Ok(()) => break (name, path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(fail(e)),
            }
        };
        let probe = path.join(".inkroot-write-test");
        let probed = std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe));
        if let Err(e) = probed {
            let _ = std::fs::remove_dir(&path);
            return Err(fail(e));
        }
        Ok((format!("{folder}/{name}"), path))
    }

    pub fn write_manifest(&self, dir: &Path, m: &Manifest) -> Result<(), FlowError> {
        let fail = |e: &dyn std::fmt::Display| {
            internal("Inkroot couldn't save the backup details.", e.to_string())
        };
        let json = serde_json::to_string_pretty(m).map_err(|e| fail(&e))?;
        let tmp = dir.join("manifest.json.tmp");
        std::fs::write(&tmp, json).map_err(|e| fail(&e))?;
        std::fs::rename(&tmp, dir.join("manifest.json")).map_err(|e| fail(&e))
    }

    pub fn list(&self, serial: Option<&str>) -> Vec<BackupSummary> {
        let mut rows: Vec<(Option<DateTime<chrono::FixedOffset>>, BackupSummary)> = vec![];
        let folder = serial.map(serial_dir_name);
        for (sname, spath) in subdirs(&self.root) {
            if folder.as_ref().is_some_and(|f| *f != sname) {
                continue;
            }
            for (dname, dpath) in subdirs(&spath) {
                if let Some(m) = read_manifest(&dpath) {
                    let ts = DateTime::parse_from_rfc3339(&m.created_at).ok();
                    rows.push((ts, m.to_summary(&format!("{sname}/{dname}"), &dpath)));
                }
            }
        }
        rows.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.created_at.cmp(&a.1.created_at))
        });
        rows.into_iter().map(|(_, s)| s).collect()
    }

    pub fn get(&self, id: &str) -> Option<(PathBuf, Manifest)> {
        if id.contains("..") || id.starts_with('/') || id.starts_with('\\') {
            return None;
        }
        let parts: Vec<&str> = id.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|p| p.is_empty() || p.contains('\\')) {
            return None;
        }
        let dir = self.root.join(parts[0]).join(parts[1]);
        read_manifest(&dir).map(|m| (dir, m))
    }

    /// The newest verified original of this tablet, firmware and slot that `profile` trusts
    /// (see [`Manifest::trusted_stock`]).
    pub fn stock_for(
        &self,
        serial: &str,
        display_id: &str,
        slot: Slot,
        profile: &DeviceProfile,
    ) -> Option<(String, PathBuf, Manifest)> {
        let want = format!("{}.img", slot.boot());
        // list() is newest first, so the first match is the newest valid one.
        self.list(Some(serial)).into_iter().find_map(|s| {
            let (dir, m) = self.get(&s.id)?;
            (m.trusted_stock(profile)
                && m.verified
                && m.serial == serial
                && m.firmware.display_id == display_id
                && m.slot == slot
                && m.file(&want).is_some())
            .then_some((s.id, dir, m))
        })
    }

    pub fn verify_file(&self, dir: &Path, m: &Manifest, name: &str) -> Result<PathBuf, FlowError> {
        let changed = |detail: String| FlowError {
            code: ErrorCode::BadImage,
            message: "Your saved copy has changed since it was made, so Inkroot won't use it."
                .into(),
            detail,
            suggested_action: Some(SuggestedAction::ImportBackup),
        };
        let entry = m
            .file(name)
            .ok_or_else(|| changed(format!("{name} is not listed in the manifest")))?;
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err(changed(format!("invalid file name {name}")));
        }
        let path = dir.join(name);
        let got = sha256_file(&path).map_err(|e| changed(format!("{}: {e}", path.display())))?;
        if !got.eq_ignore_ascii_case(&entry.sha256) {
            return Err(changed(format!(
                "{name}: sha256 {got} does not match manifest {}",
                entry.sha256
            )));
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::sha256_file;
    use crate::ipc::{BackupFile, BackupSource, ErrorCode, Slot};
    use chrono::TimeZone;

    fn manifest(serial: &str, created: &str) -> Manifest {
        Manifest {
            manifest_version: 1,
            inkroot_version: "0.1.0".into(),
            created_at: created.into(),
            serial: serial.into(),
            model: "NoteMax".into(),
            profile_id: Some("note-max".into()),
            firmware: FirmwareInfo {
                build_id: "B".into(),
                display_id: "D1".into(),
                incremental: "I".into(),
                fingerprint: "F".into(),
            },
            slot: Slot::B,
            stock: true,
            stock_evidence: StockEvidence::KnownHash,
            verified: true,
            source: BackupSource::Inkroot,
            files: vec![BackupFile {
                name: "boot_b.img".into(),
                size: 4,
                sha256: "x".into(),
            }],
            patched: None,
            imported_from: None,
        }
    }

    /// Writes a manifest (and optional boot file) under root/serial/date.
    fn put(lib: &BackupLibrary, serial: &str, date: &str, m: &Manifest) -> PathBuf {
        let dir = lib.root.join(serial).join(date);
        std::fs::create_dir_all(&dir).unwrap();
        lib.write_manifest(&dir, m).unwrap();
        dir
    }

    #[test]
    fn create_dir_names_and_collision() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let now = chrono::Local
            .with_ymd_and_hms(2026, 10, 25, 14, 30, 0)
            .unwrap();
        let (id, path) = lib.create_dir("SER1", now).unwrap();
        assert_eq!(id, "SER1/2026-10-25_1430");
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
        let (id2, _) = lib.create_dir("SER1", now).unwrap();
        assert_eq!(id2, "SER1/2026-10-25_1430-2");
        let (id3, _) = lib.create_dir("SER1", now).unwrap();
        assert_eq!(id3, "SER1/2026-10-25_1430-3");
    }

    #[cfg(unix)]
    #[test]
    fn create_dir_unwritable() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let r = lib.create_dir("SER1", chrono::Local::now());
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        if unsafe { libc::geteuid() } == 0 {
            return; // root ignores permissions
        }
        let e = r.unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(e.message.contains("Documents"));
    }

    #[test]
    fn list_sorted_and_filtered() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        put(
            &lib,
            "SER1",
            "2026-10-01_0900",
            &manifest("SER1", "2026-10-01T09:00:00+00:00"),
        );
        put(
            &lib,
            "SER2",
            "2026-10-03_0900",
            &manifest("SER2", "2026-10-03T09:00:00+00:00"),
        );
        put(
            &lib,
            "SER1",
            "2026-10-02_0900",
            &manifest("SER1", "2026-10-02T09:00:00+00:00"),
        );
        let bad = lib.root.join("SER1").join("2026-10-04_0900");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("manifest.json"), "{ not json").unwrap();
        let all = lib.list(None);
        let ids: Vec<_> = all.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "SER2/2026-10-03_0900",
                "SER1/2026-10-02_0900",
                "SER1/2026-10-01_0900"
            ]
        );
        let s1 = lib.list(Some("SER1"));
        assert_eq!(s1.len(), 2);
        assert!(s1.iter().all(|s| s.serial == "SER1"));
        assert_eq!(s1[0].firmware, "D1");
    }

    #[test]
    fn manifest_json_shape() {
        let v = serde_json::to_value(manifest("S", "2026-10-01T09:00:00+00:00")).unwrap();
        for k in [
            "manifestVersion",
            "inkrootVersion",
            "createdAt",
            "serial",
            "model",
            "profileId",
            "slot",
            "stock",
            "stockEvidence",
            "verified",
            "source",
            "patched",
            "importedFrom",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        for k in ["buildId", "displayId", "incremental", "fingerprint"] {
            assert!(v["firmware"].get(k).is_some(), "missing firmware.{k}");
        }
        assert!(v["files"][0].get("sha256").is_some());
        assert!(v["patched"].is_null() && v["importedFrom"].is_null());
        assert_eq!(v["stockEvidence"], "knownHash");
        let all = [
            (StockEvidence::MagiskbootTest, "magiskbootTest"),
            (StockEvidence::NotStockRooted, "notStockRooted"),
            (StockEvidence::NotStockFailedTest, "notStockFailedTest"),
            (StockEvidence::Unchecked, "unchecked"),
            (StockEvidence::UnknownImport, "unknownImport"),
        ];
        for (e, s) in all {
            assert_eq!(serde_json::to_value(e).unwrap(), s);
        }
    }

    fn prof() -> DeviceProfile {
        crate::profile::builtin_profiles().remove(0)
    }

    #[test]
    fn stock_for_imported_needs_current_known_entry() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let sha = "ab".repeat(32);
        let mut p = prof();
        p.known_stock_boot_sha256 = vec![crate::profile::KnownStockBoot {
            sha256: sha.clone(),
            slot: Slot::B,
            display_id: "D1".into(),
        }];
        p.firmware[0].display_id = "D1".into();
        let imported = |evidence: StockEvidence, file_sha: &str| {
            let mut m = manifest("SER1", "2026-10-01T09:00:00+00:00");
            m.source = BackupSource::Imported;
            m.stock_evidence = evidence;
            m.files[0].sha256 = file_sha.into();
            m
        };
        // Legacy evidence or a hash no longer listed: never trusted.
        for m in [
            imported(StockEvidence::MagiskbootTest, &sha),
            imported(StockEvidence::Unchecked, &sha),
            imported(StockEvidence::KnownHash, &"cd".repeat(32)),
        ] {
            assert!(!m.trusted_stock(&p), "{m:?}");
            put(&lib, "SER1", "2026-10-01_0900", &m);
            assert!(lib.stock_for("SER1", "D1", Slot::B, &p).is_none());
        }
        // A listed hash, but for another slot or firmware.
        let mut m = imported(StockEvidence::KnownHash, &sha);
        m.firmware.display_id = "D2".into();
        assert!(!m.trusted_stock(&p));
        let mut q = p.clone();
        q.known_stock_boot_sha256[0].slot = Slot::A;
        assert!(!imported(StockEvidence::KnownHash, &sha).trusted_stock(&q));
        // Still listed: trusted (case-insensitively).
        let m = imported(StockEvidence::KnownHash, &sha.to_uppercase());
        assert!(m.trusted_stock(&p));
        put(&lib, "SER1", "2026-10-01_0900", &m);
        assert!(lib.stock_for("SER1", "D1", Slot::B, &p).is_some());
        // stock:false is never trusted, whatever the evidence.
        let mut m = imported(StockEvidence::KnownHash, &sha);
        m.stock = false;
        assert!(!m.trusted_stock(&p));
        // Inkroot's own backups don't depend on the list.
        assert!(manifest("SER1", "x").trusted_stock(&prof()));
    }

    #[test]
    fn stock_for_rules() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let mut a = manifest("SER1", "2026-10-01T09:00:00+00:00");
        a.verified = false;
        put(&lib, "SER1", "2026-10-01_0900", &a);
        let mut b = manifest("SER1", "2026-10-02T09:00:00+00:00");
        b.stock = false;
        put(&lib, "SER1", "2026-10-02_0900", &b);
        put(
            &lib,
            "SER2",
            "2026-10-03_0900",
            &manifest("SER2", "2026-10-03T09:00:00+00:00"),
        );
        let mut c = manifest("SER1", "2026-10-04T09:00:00+00:00");
        c.firmware.display_id = "OTHER".into();
        put(&lib, "SER1", "2026-10-04_0900", &c);
        let mut e = manifest("SER1", "2026-10-05T09:00:00+00:00");
        e.files[0].name = "boot_a.img".into();
        put(&lib, "SER1", "2026-10-05_0900", &e);
        assert!(lib.stock_for("SER1", "D1", Slot::B, &prof()).is_none());
        put(
            &lib,
            "SER1",
            "2026-10-06_0900",
            &manifest("SER1", "2026-10-06T09:00:00+00:00"),
        );
        put(
            &lib,
            "SER1",
            "2026-10-07_0900",
            &manifest("SER1", "2026-10-07T09:00:00+00:00"),
        );
        // Newest, but taken while slot A was active: its stock evidence doesn't cover boot_b,
        // even though it holds boot_b.img. Skipped for slot B; older matches still found.
        let mut f = manifest("SER1", "2026-10-08T09:00:00+00:00");
        f.slot = Slot::A;
        f.files.push(BackupFile {
            name: "boot_a.img".into(),
            size: 4,
            sha256: "y".into(),
        });
        put(&lib, "SER1", "2026-10-08_0900", &f);
        let (id, path, m) = lib.stock_for("SER1", "D1", Slot::B, &prof()).unwrap();
        assert_eq!(id, "SER1/2026-10-07_0900");
        assert!(path.ends_with("2026-10-07_0900"));
        assert_eq!(m.created_at, "2026-10-07T09:00:00+00:00");
        // And it is the one used for slot A.
        let (id, _, m) = lib.stock_for("SER1", "D1", Slot::A, &prof()).unwrap();
        assert_eq!(id, "SER1/2026-10-08_0900");
        assert_eq!(m.slot, Slot::A);
    }

    #[test]
    fn verify_file_detects_tamper() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let dir = lib.root.join("SER1").join("2026-10-01_0900");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("boot_b.img");
        std::fs::write(&f, b"abcd").unwrap();
        let mut m = manifest("SER1", "2026-10-01T09:00:00+00:00");
        m.files[0].sha256 = sha256_file(&f).unwrap();
        lib.write_manifest(&dir, &m).unwrap();
        assert_eq!(lib.verify_file(&dir, &m, "boot_b.img").unwrap(), f);
        // not listed in the manifest
        std::fs::write(dir.join("other.img"), b"abcd").unwrap();
        let e = lib.verify_file(&dir, &m, "other.img").unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
        std::fs::write(&f, b"abce").unwrap();
        let e = lib.verify_file(&dir, &m, "boot_b.img").unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
        assert!(e.message.contains("changed since"));
        std::fs::remove_file(&f).unwrap();
        let e = lib.verify_file(&dir, &m, "boot_b.img").unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
    }

    #[test]
    fn serial_folder_is_sanitized() {
        assert_eq!(serial_dir_name("NMX1234567"), "NMX1234567");
        assert_eq!(serial_dir_name("192.168.1.5:5555"), "192.168.1.5_5555");
        assert_eq!(serial_dir_name("a/b\\c*?\"<>|"), "a_b_c______");
        assert_eq!(serial_dir_name(".."), "__");
        assert_eq!(serial_dir_name(""), "_");
        assert_eq!(serial_dir_name("ser. "), "ser__");

        // Wi-Fi ADB serial: the folder is sanitized, the manifest keeps the raw serial, and
        // everything that maps serial -> folder agrees.
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        let raw = "192.168.1.5:5555";
        let now = chrono::Local
            .with_ymd_and_hms(2026, 10, 25, 14, 30, 0)
            .unwrap();
        let (id, path) = lib.create_dir(raw, now).unwrap();
        assert_eq!(id, "192.168.1.5_5555/2026-10-25_1430");
        assert_eq!(
            path,
            lib.root.join("192.168.1.5_5555").join("2026-10-25_1430")
        );
        lib.write_manifest(&path, &manifest(raw, "2026-10-25T14:30:00+00:00"))
            .unwrap();
        let rows = lib.list(Some(raw));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].serial, raw);
        assert!(lib.get(&id).is_some());
        let (sid, _, m) = lib.stock_for(raw, "D1", Slot::B, &prof()).unwrap();
        assert_eq!(sid, id);
        assert_eq!(m.serial, raw);
        // A different device whose serial sanitizes to the same folder is still told apart.
        assert!(
            lib.stock_for("192.168.1.5_5555", "D1", Slot::B, &prof())
                .is_none()
        );
    }

    #[test]
    fn get_rejects_traversal() {
        let d = tempfile::tempdir().unwrap();
        let lib = BackupLibrary::new(d.path().to_path_buf());
        put(
            &lib,
            "SER1",
            "2026-10-01_0900",
            &manifest("SER1", "2026-10-01T09:00:00+00:00"),
        );
        assert!(lib.get("../x").is_none());
        assert!(lib.get("/SER1/2026-10-01_0900").is_none());
        assert!(lib.get("SER1/../SER1").is_none());
        assert!(lib.get("SER1").is_none());
        assert!(lib.get("SER1/a/b").is_none());
        assert!(lib.get("SER1/missing").is_none());
        assert!(lib.get("SER1/2026-10-01_0900").is_some());
    }

    #[test]
    fn default_root_env_override() {
        let _g = crate::env_lock();
        unsafe { std::env::set_var("INKROOT_BACKUP_DIR", "/tmp/inkroot-x") };
        assert_eq!(
            BackupLibrary::default_root(),
            PathBuf::from("/tmp/inkroot-x")
        );
        unsafe { std::env::remove_var("INKROOT_BACKUP_DIR") };
        assert!(BackupLibrary::default_root().ends_with("Inkroot"));
    }
}
