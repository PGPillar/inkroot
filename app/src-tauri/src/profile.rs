//! Device profiles: matching, safety gates and the verity-safe patch env.

use crate::ipc::{Gate, Slot};
use serde::{Deserialize, Serialize};

pub const MIN_BATTERY: u8 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub id: String,
    pub display_name: String,
    #[serde(rename = "match")]
    pub matcher: MatchRules,
    pub firmware: Vec<FirmwareEntry>,
    pub soc: String,
    pub storage: String,
    pub partitions: Partitions,
    pub boot_size: u64,
    pub edl: EdlConfig,
    pub magisk: MagiskConfig,
    pub bake_boot_service: bool,
    pub cable_guide_notes: Vec<String>,
    /// Boot images known to be original, each tied to the slot and firmware it was read from.
    pub known_stock_boot_sha256: Vec<KnownStockBoot>,
}

/// A known original boot image: its SHA-256, the slot it came from and the firmware
/// (`displayId`) it belongs to. All three must match for an image to count as this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownStockBoot {
    pub sha256: String,
    pub slot: Slot,
    pub display_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRules {
    pub model: String,
    pub device: String,
    pub manufacturer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareEntry {
    pub build_id: String,
    pub display_id: String,
    pub incremental: String,
    pub fingerprint: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Partitions {
    pub boot_a: String,
    pub boot_b: String,
    pub vbmeta_a: String,
    pub vbmeta_b: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdlConfig {
    pub loader: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MagiskConfig {
    pub version: String,
    pub patch_env: PatchEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct PatchEnv {
    pub keepverity: bool,
    pub keepforceencrypt: bool,
    pub patchvbmetaflag: bool,
    pub recoverymode: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceProps {
    pub serial: String,
    pub model: String,
    pub device: String,
    pub manufacturer: String,
    pub build_id: String,
    pub display_id: String,
    pub incremental: String,
    pub fingerprint: String,
    pub slot_suffix: String,
}

/// Rejects profiles that could weaken verity/encryption or are structurally empty.
pub fn validate(p: &DeviceProfile) -> Result<(), String> {
    if !p.magisk.patch_env.keepverity {
        return Err("patchEnv.KEEPVERITY must be true".into());
    }
    if !p.magisk.patch_env.keepforceencrypt {
        return Err("patchEnv.KEEPFORCEENCRYPT must be true".into());
    }
    // Never disable verification, and never patch for recovery mode.
    if p.magisk.patch_env.patchvbmetaflag {
        return Err("patchEnv.PATCHVBMETAFLAG must be false".into());
    }
    if p.magisk.patch_env.recoverymode {
        return Err("patchEnv.RECOVERYMODE must be false".into());
    }
    if p.firmware.is_empty() {
        return Err("firmware list is empty".into());
    }
    if p.boot_size == 0 {
        return Err("bootSize must be non-zero".into());
    }
    for k in &p.known_stock_boot_sha256 {
        if k.sha256.len() != 64 || !k.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "knownStockBootSha256: {:?} is not a SHA-256 (64 hex digits)",
                k.sha256
            ));
        }
        if !p.firmware.iter().any(|f| f.display_id == k.display_id) {
            return Err(format!(
                "knownStockBootSha256: displayId {:?} is not in the firmware list",
                k.display_id
            ));
        }
    }
    Ok(())
}

/// Deserializes and validates a profile from JSON.
pub fn parse_profile(json: &str) -> Result<DeviceProfile, String> {
    let p: DeviceProfile = serde_json::from_str(json).map_err(|e| e.to_string())?;
    validate(&p)?;
    Ok(p)
}

pub fn builtin_profiles() -> Vec<DeviceProfile> {
    let p = parse_profile(include_str!("../profiles/note-max.json"))
        .expect("built-in note-max profile is malformed");
    vec![p]
}

pub fn match_profile<'a>(
    ps: &'a [DeviceProfile],
    props: &DeviceProps,
) -> Option<&'a DeviceProfile> {
    ps.iter().find(|p| {
        p.matcher.model == props.model
            && p.matcher.device == props.device
            && p.matcher.manufacturer == props.manufacturer
    })
}

impl DeviceProfile {
    pub fn firmware_entry(&self, props: &DeviceProps) -> Option<&FirmwareEntry> {
        self.firmware.iter().find(|f| {
            f.build_id == props.build_id
                && f.display_id == props.display_id
                && f.incremental == props.incremental
                && f.fingerprint == props.fingerprint
        })
    }

    /// The known original entries for this image hash and slot (hash compared ignoring
    /// case), whatever their firmware.
    pub fn known_stock_entries<'a>(
        &'a self,
        sha: &'a str,
        slot: Slot,
    ) -> impl Iterator<Item = &'a KnownStockBoot> + 'a {
        self.known_stock_boot_sha256
            .iter()
            .filter(move |k| k.slot == slot && k.sha256.eq_ignore_ascii_case(sha))
    }

    /// True only when `sha` is a known original for exactly this slot and firmware.
    pub fn known_stock(&self, sha: &str, slot: Slot, display_id: &str) -> bool {
        self.known_stock_entries(sha, slot)
            .any(|k| k.display_id == display_id)
    }
}

pub fn evaluate_gate(
    profile: Option<&DeviceProfile>,
    props: &DeviceProps,
    battery: u8,
) -> Option<Gate> {
    let Some(p) = profile else {
        return Some(Gate::UnknownModel);
    };
    if p.firmware_entry(props).is_none() {
        return Some(Gate::UnknownFirmware);
    }
    if battery < MIN_BATTERY {
        return Some(Gate::LowBattery);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props() -> DeviceProps {
        DeviceProps {
            serial: "S1".into(),
            model: "NoteMax".into(),
            device: "NoteMax".into(),
            manufacturer: "ONYX".into(),
            build_id: "TKQ1.230721.002".into(),
            display_id: "TKQ1.230721.002".into(),
            incremental: "226".into(),
            fingerprint: "ONYX/TabBoox/TabBoox:13/TKQ1.230615.001/GV2.027.SQ83A:user/release-keys"
                .into(),
            slot_suffix: "_a".into(),
        }
    }

    const H1: &str = "2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc";
    const H2_UNTRUSTED: &str = "1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09";

    #[test]
    fn builtin_note_max_values() {
        let ps = builtin_profiles();
        assert_eq!(ps.len(), 1);
        let p = &ps[0];
        assert_eq!(p.id, "note-max");
        assert_eq!(p.display_name, "Note Max");
        assert_eq!(p.matcher.model, "NoteMax");
        assert_eq!(p.matcher.device, "NoteMax");
        assert_eq!(p.matcher.manufacturer, "ONYX");
        assert_eq!(p.firmware.len(), 1);
        let f = &p.firmware[0];
        assert_eq!(f.build_id, "TKQ1.230721.002");
        assert_eq!(f.display_id, "TKQ1.230721.002");
        assert_eq!(f.incremental, "226");
        assert_eq!(
            f.fingerprint,
            "ONYX/TabBoox/TabBoox:13/TKQ1.230615.001/GV2.027.SQ83A:user/release-keys"
        );
        assert_eq!(f.label, "4.2");
        assert_eq!(p.soc, "SM8150");
        assert_eq!(p.storage, "ufs");
        assert_eq!(p.partitions.boot_a, "boot_a");
        assert_eq!(p.partitions.boot_b, "boot_b");
        assert_eq!(p.partitions.vbmeta_a, "vbmeta_a");
        assert_eq!(p.partitions.vbmeta_b, "vbmeta_b");
        assert_eq!(p.boot_size, 100663296);
        assert_eq!(
            p.edl.loader,
            "qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin"
        );
        assert_eq!(p.magisk.version, "30.7");
        let e = &p.magisk.patch_env;
        assert!(e.keepverity && e.keepforceencrypt);
        assert!(!e.patchvbmetaflag && !e.recoverymode);
        assert!(p.bake_boot_service);
        assert_eq!(
            p.cable_guide_notes,
            vec![
                "Plain power-button restart does not reach EDL on this model; use the deep-flash cable."
            ]
        );
        assert_eq!(
            p.known_stock_boot_sha256,
            vec![KnownStockBoot {
                sha256: H1.into(),
                slot: Slot::B,
                display_id: "TKQ1.230721.002".into(),
            }]
        );
        // boot_a's hash (inactive slot, likely the previous update's boot) is not trusted.
        let json = include_str!("../profiles/note-max.json");
        assert!(!json.contains(H2_UNTRUSTED));
        assert!(json.contains("\"slot\": \"b\""));
        assert!(json.contains("\"displayId\": \"TKQ1.230721.002\""));
    }

    #[test]
    fn builtin_validates() {
        assert!(validate(&builtin_profiles()[0]).is_ok());
    }

    #[test]
    fn validate_rejects_verity_off() {
        let mut p = builtin_profiles()[0].clone();
        p.magisk.patch_env.keepverity = false;
        assert!(validate(&p).is_err());
        let mut p = builtin_profiles()[0].clone();
        p.magisk.patch_env.keepforceencrypt = false;
        assert!(validate(&p).is_err());
    }

    #[test]
    fn validate_rejects_vbmeta_flag_and_recovery_mode() {
        let mut p = builtin_profiles()[0].clone();
        p.magisk.patch_env.patchvbmetaflag = true;
        assert!(validate(&p).is_err());
        let mut p = builtin_profiles()[0].clone();
        p.magisk.patch_env.recoverymode = true;
        assert!(validate(&p).is_err());
        let json = include_str!("../profiles/note-max.json")
            .replace("\"PATCHVBMETAFLAG\": false", "\"PATCHVBMETAFLAG\": true");
        assert!(json.contains("\"PATCHVBMETAFLAG\": true"));
        assert!(parse_profile(&json).is_err());
        let json = include_str!("../profiles/note-max.json")
            .replace("\"RECOVERYMODE\": false", "\"RECOVERYMODE\": true");
        assert!(json.contains("\"RECOVERYMODE\": true"));
        assert!(parse_profile(&json).is_err());
    }

    #[test]
    fn validate_rejects_empty_firmware_and_zero_boot() {
        let mut p = builtin_profiles()[0].clone();
        p.firmware.clear();
        assert!(validate(&p).is_err());
        let mut p = builtin_profiles()[0].clone();
        p.boot_size = 0;
        assert!(validate(&p).is_err());
    }

    #[test]
    fn parse_profile_rejects_keepverity_false() {
        let json = include_str!("../profiles/note-max.json")
            .replace("\"KEEPVERITY\": true", "\"KEEPVERITY\": false");
        assert!(json.contains("\"KEEPVERITY\": false"));
        assert!(parse_profile(&json).is_err());
        assert!(parse_profile(include_str!("../profiles/note-max.json")).is_ok());
    }

    #[test]
    fn matching() {
        let ps = builtin_profiles();
        assert!(match_profile(&ps, &props()).is_some());
        let mut o = props();
        o.manufacturer = "Other".into();
        assert!(match_profile(&ps, &o).is_none());
    }

    #[test]
    fn gate_unknown_model() {
        let ps = builtin_profiles();
        let mut p = props();
        p.model = "Tab Ultra".into();
        assert_eq!(
            evaluate_gate(match_profile(&ps, &p), &p, 5),
            Some(Gate::UnknownModel)
        );
    }

    #[test]
    fn gate_unknown_firmware_incremental() {
        let ps = builtin_profiles();
        let mut p = props();
        p.incremental = "227".into();
        assert_eq!(
            evaluate_gate(match_profile(&ps, &p), &p, 100),
            Some(Gate::UnknownFirmware)
        );
    }

    #[test]
    fn gate_unknown_firmware_fingerprint() {
        let ps = builtin_profiles();
        let mut p = props();
        p.fingerprint = "ONYX/TabBoox/TabBoox:13/OTHER:user/release-keys".into();
        assert_eq!(
            evaluate_gate(match_profile(&ps, &p), &p, 100),
            Some(Gate::UnknownFirmware)
        );
    }

    #[test]
    fn gate_low_battery() {
        let ps = builtin_profiles();
        let p = props();
        assert_eq!(
            evaluate_gate(match_profile(&ps, &p), &p, 29),
            Some(Gate::LowBattery)
        );
        assert_eq!(evaluate_gate(match_profile(&ps, &p), &p, 30), None);
    }

    #[test]
    fn known_stock_hash() {
        let p = &builtin_profiles()[0];
        let fw = "TKQ1.230721.002";
        assert!(p.known_stock(H1, Slot::B, fw));
        assert!(p.known_stock(&H1.to_uppercase(), Slot::B, fw));
        assert!(!p.known_stock(H1, Slot::A, fw));
        assert!(!p.known_stock(H1, Slot::B, "TKQ1.999999.001"));
        assert!(!p.known_stock(H2_UNTRUSTED, Slot::A, fw));
        assert!(!p.known_stock(H2_UNTRUSTED, Slot::B, fw));
        assert!(!p.known_stock(&"ab".repeat(32), Slot::B, fw));
    }

    #[test]
    fn validate_rejects_malformed_known_stock_entries() {
        let good = builtin_profiles()[0].known_stock_boot_sha256[0].clone();
        for bad in [
            String::new(),
            "ab".repeat(31),
            "ab".repeat(33),
            format!("{}zz", &H1[..62]),
            format!(" {}", &H1[1..]),
        ] {
            let mut p = builtin_profiles()[0].clone();
            p.known_stock_boot_sha256 = vec![KnownStockBoot {
                sha256: bad.clone(),
                ..good.clone()
            }];
            assert!(validate(&p).is_err(), "{bad:?}");
        }
        // Upper-case hex is fine.
        let mut p = builtin_profiles()[0].clone();
        p.known_stock_boot_sha256[0].sha256 = H1.to_uppercase();
        assert!(validate(&p).is_ok());
        // An entry must name a firmware this profile knows.
        let mut p = builtin_profiles()[0].clone();
        p.known_stock_boot_sha256[0].display_id = "TKQ1.999999.001".into();
        assert!(validate(&p).is_err());
        // The old flat string list no longer parses.
        let json = include_str!("../profiles/note-max.json");
        let start = json.find("\"knownStockBootSha256\"").unwrap();
        let flat = format!("{}\"knownStockBootSha256\": [\"{H1}\"]\n}}", &json[..start]);
        assert!(parse_profile(&flat).is_err());
        // A bad slot doesn't parse either.
        let json = json.replace("\"slot\": \"b\"", "\"slot\": \"c\"");
        assert!(parse_profile(&json).is_err());
    }
}
