//! Persisted user settings (`settings.json`), with atomic writes.

use crate::ipc::{ErrorCode, FlowError, Settings, SettingsPatch, SuggestedAction};
use std::path::PathBuf;
use std::sync::Mutex;

pub const RISK_NOTICE_VERSION: u32 = 1;

pub fn risk_accepted(s: &Settings) -> bool {
    s.accepted_risk_version == Some(RISK_NOTICE_VERSION)
}

fn internal(message: &str, detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::Internal,
        message: message.into(),
        detail,
        suggested_action: Some(SuggestedAction::Retry),
    }
}

pub struct SettingsStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// `INKROOT_CONFIG_DIR` or `<config dir>/Inkroot`, then `settings.json`.
    pub fn default_path() -> PathBuf {
        let dir = match std::env::var_os("INKROOT_CONFIG_DIR") {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => dirs::config_dir().unwrap_or_default().join("Inkroot"),
        };
        dir.join("settings.json")
    }

    fn read(&self) -> Settings {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn get(&self) -> Settings {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.read()
    }

    pub fn apply(&self, patch: SettingsPatch) -> Result<Settings, FlowError> {
        if let Some(Some(v)) = patch.accepted_risk_version
            && v > RISK_NOTICE_VERSION
        {
            return Err(internal(
                "Something went wrong saving your settings. Please try again.",
                format!("acceptedRiskVersion {v} is newer than {RISK_NOTICE_VERSION}"),
            ));
        }
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = self.read();
        if let Some(v) = patch.has_rescue_cable {
            s.has_rescue_cable = v;
        }
        if let Some(v) = patch.accepted_risk_version {
            s.accepted_risk_version = v;
        }
        self.write(&s).map_err(|e| {
            internal(
                "Inkroot couldn't save your settings. Please try again.",
                e.to_string(),
            )
        })?;
        Ok(s)
    }

    fn write(&self, s: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{ErrorCode, SettingsPatch};

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let d = tempfile::tempdir().unwrap();
        let s = SettingsStore::new(d.path().join("nested").join("settings.json"));
        (d, s)
    }

    #[test]
    fn settings_defaults_when_missing() {
        let (_d, s) = store();
        assert_eq!(s.get(), Settings::default());
    }

    #[test]
    fn settings_defaults_when_unparseable() {
        let (d, s) = store();
        std::fs::create_dir_all(d.path().join("nested")).unwrap();
        std::fs::write(d.path().join("nested").join("settings.json"), "{nope").unwrap();
        assert_eq!(s.get(), Settings::default());
    }

    #[test]
    fn settings_patch_absent_keeps_null_clears() {
        let (_d, s) = store();
        let set: SettingsPatch =
            serde_json::from_str(r#"{"hasRescueCable":true,"acceptedRiskVersion":1}"#).unwrap();
        let got = s.apply(set).unwrap();
        assert_eq!(got.has_rescue_cable, Some(true));
        assert_eq!(got.accepted_risk_version, Some(1));
        let empty: SettingsPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(s.apply(empty).unwrap(), got);
        let clear: SettingsPatch = serde_json::from_str(r#"{"hasRescueCable":null}"#).unwrap();
        let after = s.apply(clear).unwrap();
        assert_eq!(after.has_rescue_cable, None);
        assert_eq!(after.accepted_risk_version, Some(1));
        // persisted
        assert_eq!(s.get(), after);
    }

    #[test]
    fn settings_rejects_future_risk_version() {
        let (_d, s) = store();
        let p: SettingsPatch = serde_json::from_str(r#"{"acceptedRiskVersion":2}"#).unwrap();
        let e = s.apply(p).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(s.get(), Settings::default());
    }

    #[test]
    fn settings_write_failure_is_internal() {
        let d = tempfile::tempdir().unwrap();
        let blocker = d.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        let s = SettingsStore::new(blocker.join("settings.json"));
        let p: SettingsPatch = serde_json::from_str(r#"{"hasRescueCable":true}"#).unwrap();
        let e = s.apply(p).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(!e.detail.is_empty());
    }

    #[test]
    fn risk_accepted_only_at_current_version() {
        let mut s = Settings::default();
        assert!(!risk_accepted(&s));
        s.accepted_risk_version = Some(0);
        assert!(!risk_accepted(&s));
        s.accepted_risk_version = Some(RISK_NOTICE_VERSION);
        assert!(risk_accepted(&s));
        s.accepted_risk_version = Some(RISK_NOTICE_VERSION + 1);
        assert!(!risk_accepted(&s));
    }

    #[test]
    fn default_path_honours_env() {
        let _g = crate::env_lock();
        let d = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("INKROOT_CONFIG_DIR", d.path()) };
        let p = SettingsStore::default_path();
        unsafe { std::env::remove_var("INKROOT_CONFIG_DIR") };
        assert_eq!(p, d.path().join("settings.json"));
    }
}
