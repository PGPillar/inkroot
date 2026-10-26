//! Boot image helpers: hashing, sanity checks and verified copies.

use crate::ipc::{ErrorCode, FlowError, Slot};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::path::Path;

pub const BOOT_MAGIC: &[u8; 8] = b"ANDROID!";

pub fn sha256_file(p: &Path) -> io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

pub fn has_boot_magic(p: &Path) -> io::Result<bool> {
    let mut f = std::fs::File::open(p)?;
    let mut b = [0u8; 8];
    match f.read_exact(&mut b) {
        Ok(()) => Ok(&b == BOOT_MAGIC),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

fn bad_image(detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::BadImage,
        message: "This file doesn't look like a startup (boot) file for your tablet.".into(),
        detail,
        suggested_action: None,
    }
}

pub fn validate_boot_image(p: &Path, expected_size: u64) -> Result<(), FlowError> {
    let meta = std::fs::metadata(p)
        .map_err(|e| bad_image(format!("unreadable file {}: {e}", p.display())))?;
    if meta.len() != expected_size {
        return Err(bad_image(format!(
            "size {} bytes, expected {expected_size}",
            meta.len()
        )));
    }
    match has_boot_magic(p) {
        Ok(true) => Ok(()),
        Ok(false) => Err(bad_image("wrong magic (not ANDROID!)".into())),
        Err(e) => Err(bad_image(format!("unreadable file {}: {e}", p.display()))),
    }
}

pub fn slot_from_filename(name: &str) -> Option<Slot> {
    let re = regex::Regex::new(r"(?i)boot_([ab])").expect("static regex");
    let c = re.captures(name)?;
    match c[1].to_ascii_lowercase().as_str() {
        "a" => Some(Slot::A),
        _ => Some(Slot::B),
    }
}

/// Copies `src` to `dst` and confirms the copy hashes the same. Returns the sha.
pub fn copy_verified(src: &Path, dst: &Path) -> io::Result<String> {
    let want = sha256_file(src)?;
    let mut r = std::fs::File::open(src)?;
    let mut w = std::fs::File::create(dst)?;
    io::copy(&mut r, &mut w)?;
    w.flush()?;
    w.sync_all()?;
    drop(w);
    let got = sha256_file(dst)?;
    if got != want {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("copy hash mismatch: {want} vs {got}"),
        ));
    }
    Ok(want)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{ErrorCode, Slot};

    fn boot_file(dir: &Path, name: &str, magic: &[u8], len: usize) -> std::path::PathBuf {
        let mut data = vec![0u8; len];
        data[..8].copy_from_slice(magic);
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    #[test]
    fn magic_and_size() {
        let d = tempfile::tempdir().unwrap();
        let ok = boot_file(d.path(), "ok.img", b"ANDROID!", 8192);
        assert!(validate_boot_image(&ok, 8192).is_ok());
        let bad = boot_file(d.path(), "bad.img", b"NOTANDRO", 8192);
        let e = validate_boot_image(&bad, 8192).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
        assert!(e.detail.contains("magic"));
        let short = boot_file(d.path(), "short.img", b"ANDROID!", 8191);
        let e = validate_boot_image(&short, 8192).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
        assert!(e.detail.contains("size"));
        let e = validate_boot_image(&d.path().join("missing.img"), 8192).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadImage);
    }

    #[test]
    fn slot_from_filename_cases() {
        assert_eq!(slot_from_filename("boot_b.stock.img"), Some(Slot::B));
        assert_eq!(slot_from_filename("BOOT_A.img"), Some(Slot::A));
        assert_eq!(slot_from_filename("stock.img"), None);
    }

    #[test]
    fn sha_known_vector() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("abc");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn copy_verified_copies() {
        let d = tempfile::tempdir().unwrap();
        let s = d.path().join("s");
        let t = d.path().join("t");
        std::fs::write(&s, b"abc").unwrap();
        let sha = copy_verified(&s, &t).unwrap();
        assert_eq!(sha, sha256_file(&t).unwrap());
        assert_eq!(std::fs::read(&t).unwrap(), b"abc");
    }
}
