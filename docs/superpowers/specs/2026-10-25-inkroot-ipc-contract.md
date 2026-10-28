# Inkroot — backend ⇄ UI contract

*Companion to `2026-10-25-inkroot-design.md`. The Rust backend (plan: app-core) implements this;
the Svelte frontend (plan: app-ui) consumes it. Neither side changes it without updating this
file. JSON field names are camelCase (`#[serde(rename_all = "camelCase")]`); enums serialize as
the string literals below (`#[serde(rename_all = "camelCase")]` on unit variants).*

## Types (TypeScript view; mirrored by Rust structs in `app/src-tauri/src/ipc.rs`)

```ts
type Slot = "a" | "b";

type DeviceStatus =
  | { kind: "none" }
  | { kind: "unauthorized" }                       // adb sees it, user hasn't tapped Allow
  | { kind: "edl" }                                // USB 05c6:9008 present
  | { kind: "adb"; info: DeviceInfo; gate: Gate | null };

type Gate = "unknownModel" | "unknownFirmware" | "lowBattery";

interface DeviceInfo {
  serial: string;
  model: string;            // ro.product.model
  profileId: string | null; // e.g. "note-max" when matched
  displayName: string;      // "Note Max" or the raw model
  buildId: string;          // ro.build.id
  displayId: string;        // ro.build.display.id
  incremental: string;      // ro.build.version.incremental
  firmwareLabel: string;    // "4.2" from the profile, else displayId
  slot: Slot;               // from ro.boot.slot_suffix
  battery: number;          // 0-100
  rooted: boolean;          // /debug_ramdisk/magisk -c succeeded
  magiskVersion: string | null;
  bootServiceRunning: boolean; // fresh uid=0 u:r:magisk:s0 line in /data/local/tmp/bootsvc.log
}

interface BackupFile { name: string; size: number; sha256: string; }
interface BackupSummary {
  id: string;               // "<serial>/<YYYY-MM-DD_HHMM>"
  serial: string;
  createdAt: string;        // ISO 8601
  model: string;
  firmware: string;         // displayId
  slot: Slot;               // active slot when taken / chosen at import
  stock: boolean;
  verified: boolean;
  source: "inkroot" | "imported";
  path: string;             // absolute folder path
  files: BackupFile[];      // boot_a.img, boot_b.img, vbmeta_*.img, patched-boot.img if any
}

type FlowKind = "backup" | "importBackup" | "root" | "rescue" | "unroot" | "reinstallBootService";

interface FlowOptions {
  backupId?: string;        // root/unroot/rescue: which backup to use
  imagePath?: string;       // importBackup/rescue: a loose .img chosen by the user
  slot?: Slot;              // override when the user chose a slot
}

type StepId =
  | "checkDevice" | "enterEdl" | "readPartitions" | "verifyBackup" | "rebootToSystem"
  | "pickImage" | "stockCheck" | "chooseSlot" | "saveImport"
  | "patch" | "cableCheck" | "write" | "verifyWrite" | "reset" | "waitBoot" | "verifyRoot"
  | "cableGuide" | "waitEdl" | "pushScripts" | "done";

type FlowStatus = "running" | "waitingForUser" | "done" | "failed" | "cancelled";

interface PromptOption { id: string; label: string; emphasis: "primary" | "secondary" | "danger"; }
interface Prompt {
  id: string;               // unique per prompt instance
  kind:
    | "cableCheck"          // options: have | whereToGet | continueWithout
    | "cableCheckConfirm"   // options: confirmContinue | back
    | "chooseSlot"          // options: a | b
    | "keepWaitingOrRescue" // options: keepWaiting | goToRescue
    | "installDriver"       // Windows: options: install | cancel
    | "installUdev"         // Linux:   options: install | cancel
    | "edlNotFound"         // options: retry | cancel   (after 60 s waiting for 9008)
    | "retryWrite"          // hash mismatch: options: retry | cancel
    | "connectionLostAfterWrite" // write done, check lost the USB connection for good: options: restartedOk | runAgain | cancel. No edl reset.
    | "stillInEdl"          // edl reset tried 3x, 9008 still present: options: retry | heldPower
    | "tabletStarting";     // rescue step 4 only: options: homeScreen. Non-blocking: the backend keeps
                            // polling while it is shown; answering ends the wait (rescueOutcome "homeScreenSeen")
  options: PromptOption[];
}

type ErrorCode =
  | "noDevice" | "unauthorized" | "unknownModel" | "unknownFirmware" | "lowBattery"
  | "alreadyRooted" | "notRooted" | "noStockBackup" | "badImage" | "notStock"
  | "edlTimeout" | "hashMismatch" | "patchFailed" | "bootTimeout" | "verifyFailed"
  | "toolMissing" | "driverMissing" | "busy" | "internal";

interface FlowError {
  code: ErrorCode;
  message: string;          // plain-language, UI-ready, no jargon
  detail: string;           // technical, shown only in the Details panel
  suggestedAction: "retry" | "goToRescue" | "reconnect" | "chargeDevice" | "importBackup" | null;
}

type RescueOutcome =
  | "tabletBack"            // seen over adb after the restart (authorized or unauthorized)
  | "notSeen"               // not seen within ~5 min (often USB debugging is off); still a success
  | "homeScreenSeen"        // the user answered tabletStarting before Inkroot saw the tablet
  | "backInEdl";            // the 9008 device came back instead of a started tablet

interface FlowResult {
  backup?: BackupSummary;   // backup/importBackup/root (root updates the backup with patched-boot.img)
  magiskVersion?: string;   // root
  rescueOutcome?: RescueOutcome; // rescue (always set when it ends done)
  device?: DeviceInfo;      // rescue + tabletBack, when the tablet allowed USB debugging (e.g. rooted)
}

interface FlowState {
  flowId: string;
  kind: FlowKind;
  step: StepId;
  stepIndex: number;        // 1-based, for "STEP 2 OF 4"
  stepCount: number;
  status: FlowStatus;
  prompt: Prompt | null;    // set iff status === "waitingForUser"
  error: FlowError | null;  // set iff status === "failed"
  result: FlowResult | null;// set iff status === "done"
  writing: boolean;         // true while a partition write is in progress (UI blocks close/cancel)
}

interface Progress { flowId: string; done: number; total: number; label: string; } // bytes or steps
interface LogLine { flowId: string | null; ts: string; stream: "stdout" | "stderr" | "info"; line: string; }

interface AppInfo { version: string; demo: boolean; os: "macos" | "windows" | "linux"; backupRoot: string; }
```

## Commands (`#[tauri::command]`, invoked with `invoke(name, args)`)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_app_info` | — | `AppInfo` | |
| `get_device` | — | `DeviceStatus` | Same value as the latest `device://status` event. |
| `list_backups` | `{ serial?: string }` | `BackupSummary[]` | Newest first. |
| `start_flow` | `{ kind: FlowKind, options: FlowOptions }` | `string` (flowId) | Rejects with `FlowError{code:"busy"}` if a flow is active. |
| `answer_prompt` | `{ flowId: string, promptId: string, optionId: string }` | `void` | Ignored if promptId is stale. |
| `cancel_flow` | `{ flowId: string }` | `void` | Rejected while `writing` is true. |
| `get_log` | `{ flowId?: string }` | `string` | Full text for "Copy log". |
| `reveal_path` | `{ path: string }` | `void` | Show in Finder/Explorer/file manager. |
| `get_settings` / `set_settings` | `{ settings: { hasRescueCable: boolean \| null } }` | `Settings` | Persisted per machine (cable answer). |

## Events (`app.emit`)

| Event | Payload | When |
|---|---|---|
| `device://status` | `DeviceStatus` | On change; backend polls ~every 1.5 s while no flow is active. |
| `flow://state` | `FlowState` | Every state change (full snapshot, not a diff). |
| `flow://progress` | `Progress` | During reads/writes/pushes. |
| `flow://log` | `LogLine` | Every line from adb/edl plus backend info lines. |

## Step sequences (stepCount / stepIndex the UI shows)

- **backup** (4): checkDevice → enterEdl → readPartitions → verifyBackup (+ rebootToSystem folded into verifyBackup's tail) → done
- **importBackup** (3): pickImage → stockCheck → chooseSlot/saveImport → done
- **root** (4 visible groups): 1 checkDevice · 2 "Safety copy" (runs the backup sub-flow if no verified stock backup; else instant ✓) · 3 patch → cableCheck → enterEdl → write → verifyWrite → reset · 4 waitBoot → verifyRoot → done
- **rescue** (4): 1 pickImage/chooseSlot · 2 cableGuide → waitEdl · 3 write → verifyWrite → reset · 4 waitBoot → done. No adb call before the verified write's reset. In waitBoot ("written and checked") the backend quietly polls `adb devices` and the 9008 device every 2 s for up to 5 min and shows the non-blocking `tabletStarting` prompt; it never fails here (a cancel also ends as `notSeen`). The result carries `rescueOutcome` (+ `device` for an authorized `tabletBack`).
- **unroot** (3): checkDevice → enterEdl → write → verifyWrite → reset → waitBoot (confirms `rooted:false`) → done
- **reinstallBootService** (2): checkDevice → pushScripts (hook present) → waitBoot → verifyRoot → done. If the hook is absent, the same flowId changes `kind` to `"root"` and continues from root step 2 using the stock backup (the UI just keeps rendering FlowState). If no stock backup exists it fails with `noStockBackup` / `suggestedAction:"importBackup"`.

## Legal acknowledgement

`get_settings` also returns `acceptedRiskVersion: number | null`. `start_flow` for `root`, `unroot`, `rescue` rejects with `FlowError{code:"internal", message:"Please read and accept the risk notice first."}` unless `acceptedRiskVersion` equals the backend constant `RISK_NOTICE_VERSION` (start at `1`). The UI sets it via `set_settings({ settings: { acceptedRiskVersion } })` after the user ticks the acknowledgement (design §7b).
