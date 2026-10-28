//! Backend <-> UI contract types. Mirrors docs/superpowers/specs/2026-10-25-inkroot-ipc-contract.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn suffix(&self) -> &'static str {
        match self {
            Slot::A => "_a",
            Slot::B => "_b",
        }
    }

    pub fn boot(&self) -> String {
        format!("boot{}", self.suffix())
    }

    pub fn from_suffix(s: &str) -> Option<Slot> {
        match s {
            "_a" | "a" => Some(Slot::A),
            "_b" | "b" => Some(Slot::B),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)] // contract shape: `info` stays an unboxed DeviceInfo
pub enum DeviceStatus {
    None,
    Unauthorized,
    Edl,
    Adb {
        info: DeviceInfo,
        gate: Option<Gate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gate {
    UnknownModel,
    UnknownFirmware,
    LowBattery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
    pub profile_id: Option<String>,
    pub display_name: String,
    pub build_id: String,
    pub display_id: String,
    pub incremental: String,
    pub firmware_label: String,
    pub slot: Slot,
    pub battery: u8,
    pub rooted: bool,
    pub magisk_version: Option<String>,
    pub boot_service_running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupSource {
    Inkroot,
    Imported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub id: String,
    pub serial: String,
    pub created_at: String,
    pub model: String,
    pub firmware: String,
    pub slot: Slot,
    pub stock: bool,
    pub verified: bool,
    pub source: BackupSource,
    pub path: String,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowKind {
    Backup,
    ImportBackup,
    Root,
    Rescue,
    Unroot,
    ReinstallBootService,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepId {
    CheckDevice,
    EnterEdl,
    ReadPartitions,
    VerifyBackup,
    RebootToSystem,
    PickImage,
    StockCheck,
    ChooseSlot,
    SaveImport,
    Patch,
    CableCheck,
    Write,
    VerifyWrite,
    Reset,
    WaitBoot,
    VerifyRoot,
    CableGuide,
    WaitEdl,
    PushScripts,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowStatus {
    Running,
    WaitingForUser,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Emphasis {
    Primary,
    Secondary,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOption {
    pub id: String,
    pub label: String,
    pub emphasis: Emphasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptKind {
    CableCheck,
    CableCheckConfirm,
    ChooseSlot,
    KeepWaitingOrRescue,
    InstallDriver,
    InstallUdev,
    EdlNotFound,
    RetryWrite,
    /// `edl reset` didn't get the tablet out of EDL: options `retry` | `heldPower`.
    StillInEdl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub id: String,
    pub kind: PromptKind,
    pub options: Vec<PromptOption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    NoDevice,
    Unauthorized,
    UnknownModel,
    UnknownFirmware,
    LowBattery,
    AlreadyRooted,
    NotRooted,
    NoStockBackup,
    BadImage,
    NotStock,
    EdlTimeout,
    HashMismatch,
    PatchFailed,
    BootTimeout,
    VerifyFailed,
    ToolMissing,
    DriverMissing,
    Busy,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestedAction {
    Retry,
    GoToRescue,
    Reconnect,
    ChargeDevice,
    ImportBackup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: String,
    pub suggested_action: Option<SuggestedAction>,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FlowError {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<BackupSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magisk_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowState {
    pub flow_id: String,
    pub kind: FlowKind,
    pub step: StepId,
    pub step_index: u8,
    pub step_count: u8,
    pub status: FlowStatus,
    pub prompt: Option<Prompt>,
    pub error: Option<FlowError>,
    pub result: Option<FlowResult>,
    pub writing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub flow_id: String,
    pub done: u64,
    pub total: u64,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogStream {
    Stdout,
    Stderr,
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub flow_id: Option<String>,
    pub ts: String,
    pub stream: LogStream,
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostOs {
    Macos,
    Windows,
    Linux,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub demo: bool,
    pub os: HostOs,
    pub backup_root: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub has_rescue_cable: Option<bool>,
    #[serde(default)]
    pub accepted_risk_version: Option<u32>,
}

/// Partial update: absent = unchanged, `null` = clear, value = set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_with::rust::double_option"
    )]
    pub has_rescue_cable: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_with::rust::double_option"
    )]
    pub accepted_risk_version: Option<Option<u32>>,
}
