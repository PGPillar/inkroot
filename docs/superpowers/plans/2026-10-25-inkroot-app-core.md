# Inkroot app-core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Tauri 2 scaffold and the complete Rust backend of Inkroot (device detection, gates, backup library, Back up / Import / Root / Rescue / Unroot / Reinstall-boot-service flows, demo mode), implementing the backend⇄UI contract exactly.

**Architecture:** One Rust crate at `app/src-tauri` (`inkroot_lib`). Every external effect sits behind a trait (`Adb`, `Edl`, `UsbProbe`, `HostSetup`, `KeepAwake`, `Clock`, `Emitter`). Real implementations spawn the bundled tools as foreground child processes. Fakes simulate one tablet in a `FakeWorld` and back both `cargo test` and `INKROOT_DEMO=1`. Each flow is a blocking state machine on its own thread. It talks to the UI only through `FlowState`/`Progress`/`LogLine` events, and it gets answers back through `answer_prompt`.

**Tech Stack:** Rust (stable), Tauri 2, `tauri-plugin-opener` 2, `tauri-plugin-dialog` 2, serde/serde_json/serde_with, sha2+hex, zip, nusb 0.1, chrono, uuid, regex, thiserror, dirs, tempfile, libc (unix), windows-sys (windows). Frontend scaffold: Svelte 5 + TypeScript + Vite.

**Spec:** `docs/superpowers/specs/2026-10-25-inkroot-design.md`. The contract it implements verbatim is `docs/superpowers/specs/2026-10-25-inkroot-ipc-contract.md`. Read both before starting any task. The device-operation sources of truth are `scripts/patch-boot.sh`, `scripts/edl-restore.sh`, `boot-service/*` and `docs/01–03`.

## Global Constraints

- App identifier `io.github.n0dal.inkroot`, productName `Inkroot`, crate lib name `inkroot_lib`. Window 880×640, min 760×560, resizable.
- Type, field, command, event, step, prompt, option and error-code names in `ipc.rs`/`commands.rs`/`events.rs` match the contract **character for character**. JSON is camelCase, and unit enums serialize as the contract's string literals.
- `KEEPVERITY=true KEEPFORCEENCRYPT=true` is hard-coded and validated. No setting, env var or UI path can change it. A profile whose patchEnv differs is rejected at load.
- Magisk is pinned to `Magisk-v30.7.apk`, SHA-256 `e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5`, checked before every extraction. (The spec says "30.1" in §3.2/§5 in two places. §2 overrides that with 30.7.)
- Nothing is ever written to the device unless that exact file's SHA-256 was re-verified against its manifest (or computed fresh for a loose rescue file) immediately before the write.
- **Never `edl reset` after a failed or mismatched write.** After a write, reset happens only on a read-back hash match.
- Only one flow at a time. The device poller is paused while a flow is active.
- adb/edl run as foreground children. stdout/stderr go line by line (split on `\n` and `\r`) into the log. On Windows children use `CREATE_NO_WINDOW`. Every edl call has a timeout, and on timeout or cancel the whole process tree is killed.
- Backups live at `dirs::document_dir()/Inkroot/<serial>/<YYYY-MM-DD_HHMM>/`. Override with env `INKROOT_BACKUP_DIR`. Nothing is ever uploaded, and the app makes no network calls.
- Tools resolve from `<resource_dir>/resources/` (see Task 4), overridden by env `INKROOT_TOOLS_DIR`. A missing tool gives `FlowError{code:"toolMissing"}`.
- `RISK_NOTICE_VERSION = 1`. `start_flow` for `root`/`unroot`/`rescue` is rejected unless `acceptedRiskVersion == 1`.
- Battery gate threshold: `< 30` %.
- User-facing `FlowError.message` text is plain, warm and jargon-free (spec §6 voice). Technical text goes only in `detail`.
- All verification runs from `app/src-tauri`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. `app/dist` must exist first (`npm run build` in `app/`), because `tauri::generate_context!` requires it.
- Out of scope: Svelte UI beyond the placeholder, fetching/building binaries, CI workflows, signing, README/site/docs edits.

## Review Focus

1. **Backup changed on disk after it was taken** (a file edited, truncated or swapped). Expect: Root/Unroot/Rescue re-hash it, refuse with `badImage`, and make no edl call. Pinned in Task 12 (`unroot_refuses_tampered_backup`).
2. **A second Android device plugged in** (a phone plus the tablet). Expect: the poller picks the device whose model matches a profile, and flows pin that serial through every reboot. Pinned in Task 7 (`probe_prefers_profile_match_among_many`).
3. **Cable pulled or edl crash during a backup read.** Expect: the flow fails calmly (`edlTimeout`, suggestedAction `reconnect`), a best-effort `edl reset` follows (nothing was written), and no manifest marks it verified. Pinned in Task 10 (`backup_read_failure_resets_and_fails_cleanly`).
4. **Backup folder not writable** (permissions, or a OneDrive-redirected Documents that's offline). Expect: failure at `checkDevice` before `adb reboot edl`, so the tablet isn't stranded in EDL. Pinned in Task 10 (`backup_unwritable_root_fails_before_edl`).
5. **User closes the window or quits mid-write.** Expect: close is blocked while `writing` is true. Pinned in Task 14 (`close_blocked_while_writing`).

---

## File structure (all under `app/`)

| Path | Responsibility |
|---|---|
| `package.json`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json`, `index.html`, `src/main.ts`, `src/App.svelte` | Scaffold. Core owns `package.json` + `vite.config.ts`. The rest are placeholders the UI plan replaces |
| `src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `icons/*` | Tauri crate config |
| `src-tauri/resources/README.md` | Stub documenting the layout contract (the directory is otherwise git-ignored) |
| `src-tauri/profiles/note-max.json` | The Note Max profile |
| `src-tauri/src/main.rs` | `inkroot_lib::run()` |
| `src-tauri/src/lib.rs` | Module tree, Tauri builder, setup, close guard |
| `src-tauri/src/ipc.rs` | Contract types |
| `src-tauri/src/events.rs` | Event-name constants, `Emitter` trait, `TauriEmitter` |
| `src-tauri/src/commands.rs` | `#[tauri::command]` handlers + `AppState` |
| `src-tauri/src/profile.rs` | Profile schema, loading, matching, gates |
| `src-tauri/src/clock.rs` | `Clock` trait, `SystemClock`, `FakeClock` |
| `src-tauri/src/logs.rs` | `LogStore` (buffer, tagging, forwarding) |
| `src-tauri/src/tools/mod.rs` | `ToolPaths`, `ToolError`, mapping to `FlowError` |
| `src-tauri/src/tools/process.rs` | Spawn, stream, timeout, cancel, kill tree |
| `src-tauri/src/tools/adb.rs` | `Adb` trait + `RealAdb` |
| `src-tauri/src/tools/edl.rs` | `Edl` trait + `RealEdl` + progress parsing |
| `src-tauri/src/tools/usb.rs` | `UsbProbe` trait + `NusbProbe` |
| `src-tauri/src/fakes.rs` | `FakeWorld`, `FakeAdb`, `FakeEdl`, `FakeUsb`, `FakeHost`, `FakeAwake` |
| `src-tauri/src/device.rs` | Prop parsing, probe → `DeviceStatus`, bootsvc/magisk checks |
| `src-tauri/src/poller.rs` | Background device poller |
| `src-tauri/src/image.rs` | Hashing, boot magic/size validation, slot-from-filename |
| `src-tauri/src/backup.rs` | `Manifest`, `BackupLibrary` |
| `src-tauri/src/magisk.rs` | APK verify/extract, `PATCH_CMD`, stock check, hook check |
| `src-tauri/src/settings.rs` | `SettingsStore`, `RISK_NOTICE_VERSION` |
| `src-tauri/src/host/keep_awake.rs` | `KeepAwake` trait + per-OS impls |
| `src-tauri/src/host/setup.rs` | `HostSetup` trait (WinUSB driver / udev rule) + per-OS impls |
| `src-tauri/src/flows/mod.rs` | `Deps`, `FlowEngine`, `FlowCtx`, `Interrupt`, prompt tables |
| `src-tauri/src/flows/common.rs` | Shared steps: check_device, enter_edl, write_and_verify, wait_boot, cable_check |
| `src-tauri/src/flows/{backup,import,root,rescue,unroot,reinstall}.rs` | One flow each |
| `src-tauri/src/demo.rs` | Demo `Deps` builder and scenarios |
| `src-tauri/tests/support/mod.rs` | Test harness |
| `src-tauri/tests/*.rs` | Flow + contract tests |
| `../boot-service/inkroot-default-service.sh` | Default harmless service (repo root) |

---

### Task 1: Tauri 2 + Svelte 5 scaffold

**Files:**
- Create: `app/package.json`, `app/vite.config.ts`, `app/svelte.config.js`, `app/tsconfig.json`, `app/index.html`, `app/src/main.ts`, `app/src/App.svelte`
- Create (via `tauri init`, then edit): `app/src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `icons/*`, `src/main.rs`, `src/lib.rs`
- Create: `app/src-tauri/resources/README.md`
- Modify: `.gitignore` (repo root)
- Test: `app/src-tauri/src/lib.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Produces: `inkroot_lib::run()`, plus the npm scripts `dev`, `build`, `check` (svelte-check), `tauri`, `check:rust` (`cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`).

- [ ] **Step 1: Write `app/package.json`.** `"name":"inkroot","private":true,"type":"module"`. deps: `@tauri-apps/api@^2`, `@tauri-apps/plugin-dialog@^2`. devDeps: `svelte@^5`, `@sveltejs/vite-plugin-svelte`, `vite`, `typescript`, `svelte-check`, `@tsconfig/svelte`, `@tauri-apps/cli@^2`. Include the scripts above. `vite.config.ts`: svelte plugin, `clearScreen:false`, `server:{port:1420,strictPort:true,watch:{ignored:["**/src-tauri/**"]}}`, `envPrefix:["VITE_","TAURI_ENV_"]`. `App.svelte` placeholder: on mount, `invoke("get_device")` and `listen("device://status")`, then render the `DeviceStatus` JSON in a `<pre>`, headed "Inkroot (placeholder — UI plan replaces this file)".
- [ ] **Step 2: Generate the crate.** Run `cd app && npm install && npx tauri init --ci --app-name Inkroot --window-title Inkroot --frontend-dist ../dist --dev-url http://localhost:1420 --before-dev-command "npm run dev" --before-build-command "npm run build"`. Then edit:
  - `tauri.conf.json`: `"identifier":"io.github.n0dal.inkroot"`, `"productName":"Inkroot"`, window `{title:"Inkroot",width:880,height:640,minWidth:760,minHeight:560,resizable:true}`, `"bundle":{"active":true,"targets":"all","resources":["resources/**/*"]}`, `"app":{"security":{"csp":"default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'"}}`.
  - `Cargo.toml`: `[lib] name="inkroot_lib" crate-type=["staticlib","cdylib","rlib"]`. Add all deps from the Tech Stack, `[target.'cfg(unix)'.dependencies] libc`, and `[target.'cfg(windows)'.dependencies] windows-sys` with features `Win32_System_Power`, `Win32_UI_Shell`, `Win32_Foundation`, `Win32_System_Threading`. dev-deps: `tempfile`.
  - `capabilities/default.json`: `core:default`, `dialog:allow-open`.
- [ ] **Step 3: Write `resources/README.md`.** List the exact layout from Task 4's `ToolPaths` table. Add a root `.gitignore`: `app/node_modules/`, `app/dist/`, `app/src-tauri/target/`, `app/src-tauri/gen/`, `app/src-tauri/resources/*`, `!app/src-tauri/resources/README.md`.
- [ ] **Step 4: Write the failing test** `lib.rs::tests::conf_identity`. It parses `tauri.conf.json` with `serde_json` and asserts the identifier `io.github.n0dal.inkroot`, productName `Inkroot`, window width 880, height 640, minWidth 760, minHeight 560, and `bundle.resources == ["resources/**/*"]`.
- [ ] **Step 5: Run.** `cd app && npm run build && cd src-tauri && cargo test conf_identity`. Expected: PASS once the config is edited (FAIL before Step 2's edits).
- [ ] **Step 6: Smoke-run.** `cd app && npx tauri dev`. Expected: the window opens and the placeholder shows `{"kind":"none"}` once Task 14 lands. Until then the `invoke` error is acceptable.
- [ ] **Step 7: Commit.** `git add app .gitignore && git commit -m "feat(app): scaffold Tauri 2 + Svelte 5 app"`

---

### Task 2: Contract types (`ipc.rs`) and event names (`events.rs`)

**Files:**
- Create: `app/src-tauri/src/ipc.rs`, `app/src-tauri/src/events.rs`
- Test: `app/src-tauri/tests/contract.rs`

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, `#[serde(rename_all="camelCase")]`):
  - `enum Slot { A, B }` (serde `"a"|"b"`), `impl Slot { fn suffix(&self)->&'static str; fn boot(&self)->String /* "boot_b" */; fn from_suffix(s:&str)->Option<Slot> /* "_b"|"b" */ }`
  - `#[serde(tag="kind")] enum DeviceStatus { None, Unauthorized, Edl, Adb{ info: DeviceInfo, gate: Option<Gate> } }`. `gate` serializes as `null`, never omitted.
  - `enum Gate { UnknownModel, UnknownFirmware, LowBattery }`
  - `struct DeviceInfo { serial, model, profile_id: Option<String>, display_name, build_id, display_id, incremental, firmware_label, slot: Slot, battery: u8, rooted: bool, magisk_version: Option<String>, boot_service_running: bool }`
  - `BackupFile { name, size: u64, sha256 }`, `enum BackupSource { Inkroot, Imported }`, `BackupSummary { id, serial, created_at, model, firmware, slot, stock, verified, source, path, files: Vec<BackupFile> }`
  - `enum FlowKind { Backup, ImportBackup, Root, Rescue, Unroot, ReinstallBootService }`
  - `FlowOptions { backup_id: Option<String>, image_path: Option<String>, slot: Option<Slot> }`. Every field has `#[serde(default, skip_serializing_if="Option::is_none")]`.
  - `enum StepId` with all 20 contract variants. `enum FlowStatus { Running, WaitingForUser, Done, Failed, Cancelled }`.
  - `enum Emphasis { Primary, Secondary, Danger }`, `PromptOption { id, label, emphasis }`, `enum PromptKind` (8 contract kinds), `Prompt { id, kind: PromptKind, options }`
  - `enum ErrorCode` (19 contract codes), `enum SuggestedAction { Retry, GoToRescue, Reconnect, ChargeDevice, ImportBackup }`, `FlowError { code, message, detail, suggested_action: Option<SuggestedAction> }` (`suggestedAction: null` when none), plus `impl std::error::Error`
  - `FlowResult { backup: Option<BackupSummary>, magisk_version: Option<String> }` (skip if none)
  - `FlowState { flow_id, kind, step, step_index: u8, step_count: u8, status, prompt: Option<Prompt>, error: Option<FlowError>, result: Option<FlowResult>, writing: bool }`
  - `Progress { flow_id, done: u64, total: u64, label }`, `enum LogStream { Stdout, Stderr, Info }`, `LogLine { flow_id: Option<String>, ts, stream, line }`
  - `enum HostOs { Macos, Windows, Linux }` (serde `"macos"|"windows"|"linux"`), `AppInfo { version, demo, os, backup_root }`
  - `Settings { has_rescue_cable: Option<bool>, accepted_risk_version: Option<u32> }`, and `SettingsPatch` with the same fields as `Option<Option<T>>` via `serde_with::rust::double_option` (absent = unchanged, null = clear)
- Produces in `events.rs`: `const EVT_DEVICE="device://status"; EVT_FLOW_STATE="flow://state"; EVT_PROGRESS="flow://progress"; EVT_LOG="flow://log";` and `trait Emitter: Send+Sync { fn device(&self,s:&DeviceStatus); fn flow_state(&self,s:&FlowState); fn progress(&self,p:&Progress); fn log(&self,l:&LogLine); }`. `TauriEmitter` comes in Task 14.

- [ ] **Step 1: Write failing tests in `tests/contract.rs`.** Each one serializes with `serde_json::to_value` and compares against a `json!` literal:
  - `device_status_adb_shape`: `Adb{info, gate:None}` → keys `kind:"adb"`, `info.profileId`, `info.displayName`, `info.buildId`, `info.displayId`, `info.firmwareLabel`, `info.magiskVersion`, `info.bootServiceRunning`, `gate:null`, and `slot:"b"`.
  - `device_status_unit_variants`: `None/Unauthorized/Edl` → `{"kind":"none"}`, `{"kind":"unauthorized"}`, `{"kind":"edl"}`.
  - `enums_serialize_as_contract_literals`: every `StepId`, `PromptKind`, `ErrorCode`, `FlowKind`, `FlowStatus`, `Gate` variant serializes to the contract literal. Use one table per enum, e.g. `(StepId::VerifyWrite,"verifyWrite")`, `(FlowKind::ReinstallBootService,"reinstallBootService")`, `(PromptKind::KeepWaitingOrRescue,"keepWaitingOrRescue")`, `(ErrorCode::ToolMissing,"toolMissing")`.
  - `flow_state_shape`: `stepIndex`, `stepCount`, `prompt:null`, `error:null`, `result:null`, `writing:false`.
  - `flow_error_shape`: `suggestedAction:"goToRescue"` and `suggestedAction:null`.
  - `flow_options_accepts_partial`: `{"imagePath":"/x.img"}` deserializes with `backup_id==None`, `slot==None`, and `{"slot":"a"}` → `Some(Slot::A)`.
  - `settings_patch_double_option`: `{}` → both `None`. `{"hasRescueCable":null}` → `Some(None)`. `{"acceptedRiskVersion":1}` → `Some(Some(1))`.
  - `log_line_and_app_info_shape`: `flowId:null`, `stream:"info"`, `backupRoot`, `os:"macos"`.
  - `event_names`: the four constants equal the contract strings.
  - `round_trip_all`: for one fully populated value of each struct, `from_value(to_value(x)) == x`.
- [ ] **Step 2: Run.** `cargo test --test contract`. Expected: FAIL (unresolved imports).
- [ ] **Step 3: Implement** `ipc.rs` and `events.rs` as above. Add `pub mod ipc; pub mod events;` to `lib.rs`.
- [ ] **Step 4: Run.** `cargo test --test contract`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): IPC contract types and event names"`

---

### Task 3: Device profile (`profile.rs`, `profiles/note-max.json`)

**Files:**
- Create: `app/src-tauri/profiles/note-max.json`, `app/src-tauri/src/profile.rs`
- Test: inline `#[cfg(test)] mod tests` in `profile.rs`

**Interfaces:**
- Produces:
  - `struct DeviceProfile { id, display_name, matcher: MatchRules /* json key "match" */, firmware: Vec<FirmwareEntry>, soc, storage, partitions: Partitions, boot_size: u64, edl: EdlConfig, magisk: MagiskConfig, bake_boot_service: bool, cable_guide_notes: Vec<String>, known_stock_boot_sha256: Vec<String> }` (camelCase)
  - `MatchRules { model, device, manufacturer }`, `FirmwareEntry { build_id, display_id, incremental, fingerprint, label }`, `Partitions { boot_a, boot_b, vbmeta_a, vbmeta_b }`, `EdlConfig { loader }`, `MagiskConfig { version, patch_env: PatchEnv }`
  - `#[serde(rename_all="UPPERCASE")] struct PatchEnv { keepverity: bool, keepforceencrypt: bool, patchvbmetaflag: bool, recoverymode: bool }`
  - `struct DeviceProps { serial, model, device, manufacturer, build_id, display_id, incremental, fingerprint, slot_suffix }`
  - `pub const MIN_BATTERY: u8 = 30;`
  - `pub fn builtin_profiles() -> Vec<DeviceProfile>` (`include_str!("../profiles/note-max.json")`, panics only on a malformed built-in)
  - `pub fn validate(p:&DeviceProfile) -> Result<(), String>`. It rejects `keepverity != true` or `keepforceencrypt != true`, an empty firmware list, or `boot_size == 0`.
  - `pub fn match_profile<'a>(ps:&'a [DeviceProfile], props:&DeviceProps) -> Option<&'a DeviceProfile>` (model AND device AND manufacturer)
  - `impl DeviceProfile { pub fn firmware_entry(&self, props:&DeviceProps) -> Option<&FirmwareEntry> /* all four fields equal */; pub fn is_known_stock(&self, sha:&str) -> bool }`
  - `pub fn evaluate_gate(profile: Option<&DeviceProfile>, props:&DeviceProps, battery:u8) -> Option<Gate>`. Order: unknownModel, then unknownFirmware, then lowBattery (`battery < MIN_BATTERY`).

- [ ] **Step 1: Write `note-max.json`** with exactly these values: id `note-max`, displayName `Note Max`, match `{model:"NoteMax",device:"NoteMax",manufacturer:"ONYX"}`, firmware `[{buildId:"TKQ1.230721.002",displayId:"TKQ1.230721.002",incremental:"226",fingerprint:"ONYX/TabBoox/TabBoox:13/TKQ1.230615.001/GV2.027.SQ83A:user/release-keys",label:"4.2"}]`, soc `SM8150`, storage `ufs`, partitions `boot_a/boot_b/vbmeta_a/vbmeta_b`, bootSize `100663296`, edl.loader `qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`, magisk `{version:"30.7",patchEnv:{KEEPVERITY:true,KEEPFORCEENCRYPT:true,PATCHVBMETAFLAG:false,RECOVERYMODE:false}}`, bakeBootService `true`, cableGuideNotes `["Plain power-button restart does not reach EDL on this model; use the deep-flash cable."]`, knownStockBootSha256 `["2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc","1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09"]`.
- [ ] **Step 2: Write failing tests.**
  - `builtin_note_max_values`: asserts every value above, including `boot_size==100663296` and both hashes.
  - `builtin_validates`: `validate(&builtin_profiles()[0]).is_ok()`.
  - `validate_rejects_verity_off`: setting `patch_env.keepverity=false` gives `Err`. Same for `keepforceencrypt=false`.
  - `gate_unknown_model`: model `Tab Ultra` → `Some(Gate::UnknownModel)` even with battery 5.
  - `gate_unknown_firmware_incremental`: buildId and displayId match, incremental `"227"` → `Some(Gate::UnknownFirmware)`.
  - `gate_unknown_firmware_fingerprint`: everything matches except the fingerprint → `UnknownFirmware`.
  - `gate_low_battery`: matching props, battery 29 → `LowBattery`. Battery 30 → `None`.
  - `known_stock_hash`: `is_known_stock("2f05dc1b…c8fc")` true; uppercase input also true (compare lowercase); a random hash false.
- [ ] **Step 3: Run.** `cargo test profile::`. Expected: FAIL.
- [ ] **Step 4: Implement** `profile.rs`.
- [ ] **Step 5: Run.** `cargo test profile::`. Expected: PASS.
- [ ] **Step 6: Commit.** `git commit -m "feat(core): device profile schema and Note Max profile"`

---

### Task 4: Clock, log store, process runner, tool paths, real adb/edl/usb

**Files:**
- Create: `src/clock.rs`, `src/logs.rs`, `src/tools/mod.rs`, `src/tools/process.rs`, `src/tools/adb.rs`, `src/tools/edl.rs`, `src/tools/usb.rs`
- Test: inline tests in each file

**Interfaces:**
- `clock.rs`: `trait Clock: Send+Sync { fn now(&self)->Duration /* monotonic since start */; fn sleep(&self,d:Duration); fn wall(&self)->chrono::DateTime<chrono::Local>; }`. `SystemClock`. `FakeClock::new(start_wall)`: `sleep` advances virtual time instantly, and `wall()` = start + virtual elapsed. It is `Clone` with a shared `Arc<Mutex<Duration>>`.
- `logs.rs`: `LogStore::new(emitter: Arc<dyn Emitter>, clock: Arc<dyn Clock>)`, `set_flow(&self, Option<String>)`, `info(&self, line)`, `line(&self, stream: LogStream, line)`. Each line is tagged with the current flowId and an ISO-8601 ms `ts`, stored, and forwarded via `emitter.log`. `text(&self, flow_id: Option<&str>) -> String` renders `"<ts> [<stream>] <line>\n"` per line, filtered by flow when given. The buffer is capped at 20 000 lines (oldest dropped).
- `tools/process.rs`:
  - `#[derive(Clone, Default)] struct CancelToken(Arc<AtomicBool>)` with `cancel()`, `is_cancelled()`
  - `struct CmdSpec { program: PathBuf, args: Vec<String> }`, `struct CmdOutput { code: i32, stdout: String, stderr: String }`
  - `fn run(spec:&CmdSpec, timeout:Duration, cancel:Option<&CancelToken>, log:&LogStore, on_line:&mut dyn FnMut(&str)) -> Result<CmdOutput, ToolError>`. Two reader threads; lines go to `log` (Stdout/Stderr) and `on_line`. A wait loop uses `try_wait` every 50 ms. On timeout → `kill_tree` → `Err(Timeout)`. On cancel → `kill_tree` → `Err(Cancelled)`. A missing program → `Err(Missing)`.
  - Unix: `std::os::unix::process::CommandExt::process_group(0)`, kill via `libc::kill(-pid, SIGKILL)`. Windows: `creation_flags(0x08000000 /*CREATE_NO_WINDOW*/ | 0x00000200 /*CREATE_NEW_PROCESS_GROUP*/)`, kill via `taskkill /PID <pid> /T /F`.
- `tools/mod.rs`:
  - `#[derive(thiserror::Error, Debug)] enum ToolError { Missing(PathBuf), Spawn(String), Timeout{tool:String, secs:u64}, Exit{tool:String, code:i32, stderr:String}, Cancelled }`
  - `impl From<ToolError> for FlowError`: `Missing` → `toolMissing`; `Timeout{tool:"edl"}` → `edlTimeout` with suggestedAction `reconnect`; anything else → `internal`. `detail` = the Debug text.
  - `struct Timeouts { adb_shell, adb_push, patch, edl_read, edl_write, edl_reset, wait_edl, wait_boot, wait_bootsvc, poll }`. `Default`: 60 s, 120 s, 180 s, 600 s, 900 s, 60 s, 60 s, 180 s, 90 s, 1.5 s.
  - `struct ToolPaths { root: PathBuf }` with `fn resolve(resource_dir: Option<PathBuf>) -> ToolPaths`. `INKROOT_TOOLS_DIR` wins; otherwise `resource_dir.join("resources")`, because Tauri's `"resources/**/*"` glob preserves the `resources/` prefix under `resource_dir()`. Path methods follow this table:

    | method | path (`.exe` on Windows where shown) |
    |---|---|
    | `adb()` | `tools/adb[.exe]` |
    | `edl()` | `tools/edl/edl[.exe]` |
    | `loader(rel)` | `tools/loaders/<rel>` |
    | `wdi_simple()` | `tools/wdi-simple.exe` |
    | `magisk_apk(version)` | `magisk/Magisk-v<version>.apk` |
    | `boot_service(name)` | `boot-service/<name>` (`init.bootsvc.rc`, `bootsvc.sh`, `inkroot-default-service.sh`) |

  - `fn require(&self, p:&Path) -> Result<PathBuf, FlowError>` (`toolMissing`, detail = path).
  - `fn require_for(&self, kind:FlowKind, p:&DeviceProfile) -> Result<(), FlowError>` checks: backup = adb, edl, loader; unroot = same; rescue = edl, loader; root = adb, edl, loader, magisk apk, all three boot-service files; reinstallBootService = adb, `bootsvc.sh`, `inkroot-default-service.sh`; importBackup = nothing. Magisk is required lazily at stock-check time.
- `tools/adb.rs`: `enum AdbState { Device, Unauthorized, Offline }`, `struct AdbDevice { serial, state }`, `enum RebootTarget { System, Edl }`, and
  ```rust
  pub trait Adb: Send + Sync {
      fn devices(&self) -> Result<Vec<AdbDevice>, ToolError>;
      fn shell(&self, serial: &str, cmd: &str, timeout: Duration) -> Result<CmdOutput, ToolError>;
      fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), ToolError>;
      fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<(), ToolError>;
      fn reboot(&self, serial: &str, target: RebootTarget) -> Result<(), ToolError>;
      fn kill_server(&self);
  }
  ```
  `RealAdb { path, log: Arc<LogStore>, timeouts }` with a pure `fn adb_args(serial:Option<&str>, rest:&[&str]) -> Vec<String>` (`["-s",serial, ...rest]`). `devices` parses `adb devices` lines `<serial>\t<state>`, where `device` → Device, `unauthorized` → Unauthorized, and anything else → Offline. `shell` returns the exit code (a non-zero code is not an `Err`). `push`/`pull`/`reboot` with a non-zero exit → `Err(Exit)`. `reboot(Edl)` = `reboot edl`.
- `tools/edl.rs`:
  ```rust
  pub trait Edl: Send + Sync {
      fn read(&self, part: &str, out: &Path, cancel: &CancelToken, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError>;
      fn write(&self, part: &str, img: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), ToolError>;
      fn reset(&self) -> Result<(), ToolError>;
  }
  ```
  `RealEdl { path, loader: PathBuf /* absolute */, log, timeouts, part_size: u64 }`. Pure `fn edl_args(loader:&Path, op:&EdlOp) -> Vec<String>` with `enum EdlOp { Read{part,out}, Write{part,img}, Reset }` → `["--memory=ufs", "--loader=<abs>", "r", part, out]` / `[..., "w", part, img]` / `[..., "reset"]`. A non-zero exit → `Err(Exit)`. `fn parse_percent(line:&str) -> Option<f32>` uses regex `(\d{1,3}(?:\.\d+)?)\s*%`, and progress = `pct/100*part_size`. Writes pass no cancel token, because cancel is rejected while writing.
- `tools/usb.rs`: `struct EdlUsbInfo { driver: Option<String> }`, `trait UsbProbe: Send+Sync { fn edl_device(&self) -> Option<EdlUsbInfo>; }`. `NusbProbe`: `nusb::list_devices()` finds vendor `0x05c6` product `0x9008`. On Windows it fills `driver` from `DeviceInfo::driver()`, and elsewhere it's `None`.

- [ ] **Step 1: Write failing tests.**
  - `clock::fake_sleep_advances`: `FakeClock` sleep 60 s → `now()` 60 s, `wall()` +60 s, no real delay (the test finishes in < 1 s).
  - `logs::tags_and_filters`: `set_flow(Some("f1"))`, then `info("a")`, `set_flow(None)`, `info("b")` → `text(Some("f1"))` contains `a` but not `b`, and the recording emitter got 2 `LogLine`s with `flow_id` `Some("f1")`/`None`.
  - `process::streams_lines_and_exit_code` (unix): `sh -c 'echo one; echo two 1>&2; printf "3%%\r4%%\n"; exit 3'` → `code==3`, and `on_line` saw `one`, `two`, `3%`, `4%`.
  - `process::timeout_kills_tree` (unix): `sh -c 'sleep 30 & echo $! > <tmp>/pid; sleep 30'`, timeout 300 ms → `Err(Timeout)` within 3 s, and `libc::kill(grandchild_pid, 0) != 0` after 200 ms.
  - `process::cancel_kills` (unix): cancel from another thread after 200 ms → `Err(Cancelled)`.
  - `process::missing_program`: `/nonexistent/adb` → `Err(Missing(_))`. `FlowError::from(..).code == ErrorCode::ToolMissing`.
  - `tools::paths_layout`: `ToolPaths{root:"/r"}` → `adb()=="/r/tools/adb"` (unix), `edl()=="/r/tools/edl/edl"`, `loader("qualcomm/x.bin")=="/r/tools/loaders/qualcomm/x.bin"`, `magisk_apk("30.7")=="/r/magisk/Magisk-v30.7.apk"`, `boot_service("bootsvc.sh")=="/r/boot-service/bootsvc.sh"`.
  - `tools::env_override`: with `INKROOT_TOOLS_DIR=/t` → `root=="/t"`. With it unset and `Some("/res")` → `"/res/resources"`. (Serialize env tests with a static `Mutex`.)
  - `tools::require_for_root_lists_missing`: an empty temp root → `require_for(Root,..)` is `Err` with code `toolMissing`. After creating every file it is `Ok`.
  - `edl::args_exact`: `edl_args("/L/x.bin", Read{"boot_b","/o/boot_b.img"}) == ["--memory=ufs","--loader=/L/x.bin","r","boot_b","/o/boot_b.img"]`. Write → `w`. Reset → `["--memory=ufs","--loader=/L/x.bin","reset"]`.
  - `edl::parse_percent_cases`: `"Progress: |████| 45.2% Read"` → `45.2`; `"no pct"` → `None`.
  - `adb::parse_devices`: `"List of devices attached\nABC\tdevice\nDEF\tunauthorized\nGHI\toffline\n\n"` → three entries with the right states.
- [ ] **Step 2: Run.** `cargo test clock:: logs:: process:: tools:: edl:: adb::`. Expected: FAIL.
- [ ] **Step 3: Implement** all files listed in this task.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): process runner, tool paths, adb/edl/usb runners"`

---

### Task 5: Image utilities and backup library

**Files:**
- Create: `src/image.rs`, `src/backup.rs`
- Test: inline tests

**Interfaces:**
- `image.rs`: `const BOOT_MAGIC:&[u8;8]=b"ANDROID!"`; `fn sha256_file(p:&Path)->io::Result<String>` (lowercase hex, streamed in 1 MiB chunks); `fn has_boot_magic(p)->io::Result<bool>`; `fn validate_boot_image(p:&Path, expected_size:u64)->Result<(),FlowError>`. That last one returns `badImage` when the file is missing or unreadable, the magic is wrong, or the size ≠ expected, and `detail` says which. Also `fn slot_from_filename(name:&str)->Option<Slot>` (case-insensitive regex `boot_([ab])`) and `fn copy_verified(src,dst)->io::Result<String>`, which copies and returns the sha after confirming the sha of dst equals the sha of src (`Err(InvalidData)` otherwise).
- `backup.rs`:
  - `#[serde(rename_all="camelCase")] struct Manifest { manifest_version: u32 /*=1*/, inkroot_version: String, created_at: String /*ISO 8601*/, serial, model, profile_id: Option<String>, firmware: FirmwareInfo, slot: Slot, stock: bool, stock_evidence: StockEvidence, verified: bool, source: BackupSource, files: Vec<BackupFile>, patched: Option<PatchedInfo>, imported_from: Option<String> }`
  - `FirmwareInfo { build_id, display_id, incremental, fingerprint }`. Imports with no device use the profile entry if the hash is known, else empty strings.
  - `enum StockEvidence { KnownHash, MagiskbootTest, NotStockRooted, NotStockFailedTest, Unchecked }`, `PatchedInfo { file: String /* "patched-boot.img" */, sha256, magisk_version }`
  - `impl Manifest { fn to_summary(&self, id:&str, dir:&Path) -> BackupSummary /* firmware = display_id */; fn file(&self, name:&str) -> Option<&BackupFile> }`
  - `struct BackupLibrary { root: PathBuf }`:
    - `fn new(root)`, `fn default_root() -> PathBuf` (`INKROOT_BACKUP_DIR` or `dirs::document_dir().unwrap_or(home)/Inkroot`)
    - `fn create_dir(&self, serial:&str, now: DateTime<Local>) -> Result<(String, PathBuf), FlowError>`. Folder name is `now.format("%Y-%m-%d_%H%M")`, and a collision appends `-2`, `-3`…. It also writes and deletes a probe file. A failure → `internal` with message "Inkroot can't save files in your Documents folder. Check that it exists and isn't read-only." and detail = the io error.
    - `fn write_manifest(&self, dir:&Path, m:&Manifest) -> Result<(), FlowError>` (atomic: write `manifest.json.tmp` then rename, pretty JSON)
    - `fn list(&self, serial: Option<&str>) -> Vec<BackupSummary>` (scans `<root>/*/*/manifest.json`, skips unparseable ones, newest `created_at` first)
    - `fn get(&self, id:&str) -> Option<(PathBuf, Manifest)>`. It rejects ids containing `..` or a leading `/`.
    - `fn stock_for(&self, serial:&str, display_id:&str, slot:Slot) -> Option<(String, PathBuf, Manifest)>`: newest with `stock && verified && serial==serial && firmware.display_id==display_id` and a file `boot_<slot>.img` listed.
    - `fn verify_file(&self, dir:&Path, m:&Manifest, name:&str) -> Result<PathBuf, FlowError>`: re-hashes and compares against the manifest. A mismatch or missing file → `badImage` with message "Your saved copy has changed since it was made, so Inkroot won't use it."

- [ ] **Step 1: Write failing tests.**
  - `image::magic_and_size`: an 8 KiB file starting `ANDROID!` → `validate_boot_image(p, 8192)` Ok. Wrong magic → `badImage` (detail contains "magic"). Size 8191 → `badImage` (detail contains "size").
  - `image::slot_from_filename_cases`: `"boot_b.stock.img"`→B, `"BOOT_A.img"`→A, `"stock.img"`→None.
  - `image::sha_known_vector`: sha of `b"abc"` = `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.
  - `backup::create_dir_names_and_collision`: wall `2026-10-25 14:30` → id `SER1/2026-10-25_1430`. Called twice → second id `SER1/2026-10-25_1430-2`.
  - `backup::create_dir_unwritable` (unix): root chmod 0o500 → `Err` code `internal`, and message contains "Documents".
  - `backup::list_sorted_and_filtered`: three manifests, two serials, mixed dates → `list(None)` newest first, `list(Some("SER1"))` only SER1, and a corrupt `manifest.json` is skipped.
  - `backup::manifest_json_shape`: the serialized manifest has keys `manifestVersion`, `inkrootVersion`, `createdAt`, `serial`, `model`, `profileId`, `firmware.buildId`, `firmware.displayId`, `firmware.incremental`, `firmware.fingerprint`, `slot`, `stock`, `stockEvidence`, `verified`, `source`, `files[0].sha256`, `patched`, `importedFrom`.
  - `backup::stock_for_rules`: it ignores `verified:false`, `stock:false`, another serial, another displayId, and a backup lacking `boot_<slot>.img`, and it picks the newest valid one.
  - `backup::verify_file_detects_tamper`: flipping one byte after the manifest is written → `badImage`.
  - `backup::get_rejects_traversal`: `get("../x")` → None.
- [ ] **Step 2: Run.** `cargo test image:: backup::`. Expected: FAIL.
- [ ] **Step 3: Implement** both modules.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): boot image utilities and backup library"`

---

### Task 6: Magisk payload, patch command, stock and hook checks; default boot service

**Files:**
- Create: `src/magisk.rs`, `boot-service/inkroot-default-service.sh` (repo root)
- Test: inline tests in `magisk.rs`

**Interfaces:**
- Constants (exact):
  ```rust
  pub const MAGISK_APK_SHA256: &str = "e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5";
  pub const MGK_DIR: &str = "/data/local/tmp/mgk";
  pub const MGK_RESET_CMD: &str = "rm -rf /data/local/tmp/mgk && mkdir -p /data/local/tmp/mgk";
  pub const APK_PAYLOAD: [(&str, &str); 7] = [
      ("lib/arm64-v8a/libmagiskboot.so", "magiskboot"),
      ("lib/arm64-v8a/libmagiskinit.so", "magiskinit"),
      ("lib/arm64-v8a/libmagisk.so", "magisk"),
      ("lib/arm64-v8a/libinit-ld.so", "init-ld"),
      ("assets/boot_patch.sh", "boot_patch.sh"),
      ("assets/util_functions.sh", "util_functions.sh"),
      ("assets/stub.apk", "stub.apk"),
  ];
  /// spec §3.2 step 2 — byte-for-byte; run with `adb shell '<PATCH_CMD>'` semantics (single shell arg)
  pub const PATCH_CMD: &str = "cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh stock_boot.img && ./magiskboot unpack new-boot.img && ./magiskboot cpio ramdisk.cpio \"add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc\" && ./magiskboot cpio ramdisk.cpio \"ls overlay.d\" | grep init.bootsvc && ./magiskboot repack new-boot.img patched-boot.img";
  pub const CHECK_DIR: &str = "/data/local/tmp/inkroot-check";
  pub const STOCK_CHECK_CMD: &str = "cd /data/local/tmp/inkroot-check && chmod 755 magiskboot && ./magiskboot unpack boot.img && ./magiskboot cpio ramdisk.cpio test";
  pub const HOOK_CHECK_CMD: &str = "cd /data/local/tmp/inkroot-check && chmod 755 magiskboot && ./magiskboot unpack boot.img && ./magiskboot cpio ramdisk.cpio \"ls overlay.d\" | grep init.bootsvc";
  pub const CHECK_CLEANUP_CMD: &str = "rm -rf /data/local/tmp/inkroot-check";
  pub const CHECK_PREP_CMD: &str = "rm -rf /data/local/tmp/inkroot-check && mkdir -p /data/local/tmp/inkroot-check";
  pub const DEVICE_BOOTSVC: &str = "/data/local/tmp/bootsvc.sh";
  pub const DEVICE_SERVICE: &str = "/data/local/tmp/service.sh";
  ```
- Functions:
  - `fn patch_command(env:&PatchEnv) -> Result<String, FlowError>` builds the command from the env in the fixed order `KEEPVERITY KEEPFORCEENCRYPT PATCHVBMETAFLAG RECOVERYMODE`. It refuses (`internal`) unless keepverity && keepforceencrypt.
  - `fn extract_payload(apk:&Path, expected_sha:&str, out:&Path) -> Result<Vec<(PathBuf, &'static str)>, FlowError>`. A missing apk → `toolMissing`. A sha mismatch → `internal`, message "Inkroot's bundled Magisk package failed its integrity check. Please re-download Inkroot.". A missing zip entry → `internal`. Returns `(local_path, device_name)` for the 7 entries in order.
  - `fn push_patch_inputs(adb:&dyn Adb, serial:&str, payload:&[(PathBuf, &str)], stock_img:&Path, rc:&Path, bootsvc:&Path, service:&Path) -> Result<(), FlowError>`. It runs `MGK_RESET_CMD`, then pushes the files in the same order and to the same remote names as `boot-service/build-and-flash.sh`:
    - each payload entry → `MGK_DIR/<device_name>`
    - `rc` → `MGK_DIR/init.bootsvc.rc`
    - `stock_img` → `MGK_DIR/stock_boot.img`
    - `bootsvc` → `DEVICE_BOOTSVC`
    - `service` → `DEVICE_SERVICE`

    A push failure → `internal` with message "Inkroot couldn't copy files to your tablet. Check the cable and try again." (suggestedAction `reconnect`).
  - `fn run_patch(adb:&dyn Adb, serial:&str, env:&PatchEnv, boot_size:u64, out:&Path, t:&Timeouts) -> Result<String /*sha256*/, FlowError>`. It runs `adb shell` with `patch_command(env)` (timeout `t.patch`).
    - A non-zero exit → `patchFailed`, message "Inkroot couldn't prepare the rooted startup file. Nothing on your tablet was changed.", detail = stdout+stderr.
    - Otherwise it pulls `MGK_DIR/patched-boot.img` → `out`, then `validate_boot_image(out, boot_size)`, and returns its sha.
  - `fn stock_check(adb:&dyn Adb, serial:&str, magiskboot:&Path, img:&Path, t:&Timeouts) -> Result<bool, FlowError>`. It runs `CHECK_PREP_CMD`, pushes `magiskboot` → `CHECK_DIR/magiskboot` and `img` → `CHECK_DIR/boot.img`, then runs `STOCK_CHECK_CMD`.
    - Exit `0` → `true` (stock). Exit `1` (Magisk-patched) or `2` (unsupported/other) → `false`. Any other exit → `internal`.
    - `CHECK_CLEANUP_CMD` always runs afterwards, even on error.
  - `fn hook_present(adb:&dyn Adb, serial:&str, magiskboot:&Path, img:&Path, t:&Timeouts) -> Result<bool, FlowError>`. Same shape as `stock_check`, with `HOOK_CHECK_CMD`. Exit 0 → `true`, otherwise `false`.
- Repo file `boot-service/inkroot-default-service.sh`: this is `boot-service/example-service.sh`'s heartbeat loop only. It logs `uid`/`ctx` at start and a battery heartbeat every 300 s to `/data/local/tmp/service.log`. It has **no** suspend, wakealarm, radio, cpufreq or settings lines, not even commented out.

- [ ] **Step 1: Write the failing tests** in `src-tauri/src/magisk.rs` `#[cfg(test)]`:
  - `patch_cmd_constant_is_spec_sequence`: `PATCH_CMD` contains `"KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh stock_boot.img"` and `"add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc"` and ends with `"./magiskboot repack new-boot.img patched-boot.img"`. Also, `patch_command(&note_max().magisk.patch_env).unwrap() == PATCH_CMD`.
  - `patch_command_refuses_verity_off`: with `keepverity=false` → `Err`, code `internal`. Same with `keepforceencrypt=false`.
  - `extract_payload_returns_seven_in_order`: build a fixture zip in a tempdir containing the 7 entries, pass its own sha → the 7 device names are in `APK_PAYLOAD` order.
  - `extract_payload_rejects_wrong_sha` → `internal`. `extract_payload_missing_apk` → `toolMissing`.
  - In `tests/default_service.rs`: `default_service_is_harmless`. The file has none of `/sys/power/state`, `wakealarm`, `svc `, `scaling_max_freq`, `settings put`, and `sh -n boot-service/inkroot-default-service.sh` exits 0 (skip on Windows).
- [ ] **Step 2: Run** `cargo test magisk default_service`. Expected: FAIL (unresolved items).
- [ ] **Step 3: Implement** `magisk.rs` and the default service file. Use the `zip` crate for extraction and `image::sha256_file` for hashing. The `&dyn Adb` functions get their tests in Task 7, once the fakes exist.
- [ ] **Step 4: Run** `cargo test magisk default_service`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): magisk payload, patch command, stock/hook checks, default service"`

---

### Task 7: Fakes, device probe and poller

**Files:** Create `src-tauri/src/fakes.rs`, `src-tauri/src/device.rs`, `src-tauri/src/poller.rs`, `src-tauri/tests/support/mod.rs`, `src-tauri/tests/device.rs`, `src-tauri/tests/magisk_adb.rs`.

**Interfaces:**
- Consumes: the traits from Task 4, `DeviceProfile`/`evaluate_gate` (Task 3), and the magisk constants (Task 6).
- `fakes.rs`:
  - `FakeWorld(Arc<Mutex<World>>)`, a single simulated tablet. `World` fields:
    - `mode: Mode { Off, System{authorized:bool}, Edl }`
    - `props: BTreeMap<String,String>`, `battery: u8`
    - `partitions: HashMap<String, Vec<u8>>`, `files: HashMap<String, Vec<u8>>` (device fs)
    - `rooted: bool`, `magisk_version: Option<String>`, `bootsvc_lines: Vec<String>`, `service_running: bool`
    - `other_devices: Vec<(String, AdbState, BTreeMap<String,String>)>`
    - `faults: Faults`, `calls: Vec<Call>`
  - `Faults { edl_never_appears, read_fails_on: Option<String>, write_corrupts_times: u32, patch_fails, boot_hangs, stock_check_exit: Option<i32>, hook_check_exit: Option<i32>, verify_read_differs }`.
  - `enum Call { Devices, Shell{serial, cmd}, Push{local_name, remote}, Pull{remote}, Reboot{serial, target}, EdlRead{part}, EdlWrite{part, sha}, EdlReset, DriverInstall, UdevInstall }`. Tests assert on the call log.
  - Booting rules:
    - On `reboot(System)` or `EdlReset` the tablet goes `Off`, then `System{authorized:true}` on the next `devices()` poll, unless `boot_hangs` is set.
    - On boot, `rooted`, `magisk_version` and the bootsvc line come from the bytes in the active `boot_<slot>`. Bytes produced by the fake patch (marker `b"INKROOT-PATCHED"` appended) mean `rooted = true`, version "30.7", a new `boot <date> uid=0 ctx=u:r:magisk:s0` line, and `service_running = true`. Anything else means not rooted.
  - `FakeAdb::new(world)` implements `Adb` by recognising exactly these commands (anything else → exit 127, recorded):
    - `getprop`
    - `dumpsys battery`
    - `/debug_ramdisk/magisk -c`
    - `cat /data/local/tmp/bootsvc.log`
    - `pgrep -f /data/adb/service.sh`
    - `MGK_RESET_CMD`, `PATCH_CMD` (writes `MGK_DIR/patched-boot.img` = `files[MGK_DIR/stock_boot.img]` + marker, or exits 1 if `patch_fails`)
    - `CHECK_PREP_CMD`, `STOCK_CHECK_CMD` (exit = `stock_check_exit` or 0 when the image has no marker, 1 when it does), `HOOK_CHECK_CMD` (likewise with `hook_check_exit`), `CHECK_CLEANUP_CMD`
  - `FakeEdl::new(world)`:
    - `read` copies the partition, or fails with `Timeout{tool:"edl"}` when `read_fails_on` names that partition.
    - `write` stores the image, flipping one byte while `write_corrupts_times > 0` (and decrementing it).
    - `reset` follows the booting rules.
    - When `verify_read_differs` is set, the second read of the same partition returns different bytes.
  - `FakeUsb` reports EDL iff `mode == Edl` and `!edl_never_appears`.
  - `FakeHost { needs_driver, needs_udev }` records installs. `FakeAwake` counts holds.
  - `fn fake_boot_image(size: usize, seed: u8) -> Vec<u8>` (starts with `ANDROID!`).
- `tests/support/mod.rs`:
  - `fn test_profile() -> DeviceProfile`: the Note Max profile with `boot_size = 4096` and `known_stock_boot_sha256 = []`.
  - `fn stock_world() -> FakeWorld`: Note Max props from spec §2, slot `_b`, battery 80, stock `fake_boot_image` in boot_a/boot_b, vbmeta 4096 B each.
  - `fn harness(world) -> Harness` where `Harness { engine: FlowEngine, world, events: RecordingEmitter, lib_dir: TempDir }`.
  - `fn run_to_end(h, kind, opts, answers: &[(&str /*prompt kind*/, &str /*option id*/)]) -> FlowState`. It answers prompts in order as they appear and panics on an unexpected prompt.
  - `RecordingEmitter` captures all events.
  - Uses `FakeClock`, so timeouts pass instantly.
- `device.rs`:
  - `fn parse_getprop(out:&str) -> BTreeMap<String,String>` (`[k]: [v]` lines)
  - `fn props_from(map, serial) -> DeviceProps`
  - `fn parse_battery(dumpsys:&str) -> Option<u8>` (`level: N`)
  - `fn parse_magisk_version(out:&str) -> Option<String>` (`"30.7:MAGISK:R"` → `"30.7"`)
  - `fn bootsvc_boot_lines(log:&str) -> usize` counts lines starting `boot `
  - `fn bootsvc_ok(log:&str) -> bool`: the last `boot ` line contains `uid=0` and `u:r:magisk:s0`
  - `fn probe(adb:&dyn Adb, usb:&dyn UsbProbe, profiles:&[DeviceProfile], t:&Timeouts) -> (DeviceStatus, Option<String /*serial*/>)`:
    - EDL USB present → `Edl`.
    - Otherwise prefer an authorized device whose props match a profile, then any authorized device, then `Unauthorized` if any device is unauthorized, else `None`.
    - Fills `DeviceInfo`: `rooted` = magisk -c exit 0; `boot_service_running` = `bootsvc_ok` && `pgrep` exit 0; firmware_label from the matched firmware entry, else display_id.
    - `gate` = `evaluate_gate`.
  - `fn bootsvc_line_count(adb, serial, t) -> usize` (0 when the file is missing).
- `poller.rs`: `Poller::spawn(adb, usb, profiles, emitter, clock, interval) -> Arc<Poller>` with `pause()`, `resume()`, `latest() -> DeviceStatus` and `serial() -> Option<String>`. It emits `device://status` only on change.

- [ ] **Step 1: Write the failing tests.**
  - `tests/device.rs`:
    - `parse_getprop_reads_note_max_props`
    - `parse_magisk_version_variants`
    - `probe_none`, `probe_unauthorized`, `probe_edl_wins`
    - `probe_note_max_ok_has_no_gate`: info slot `B`, firmwareLabel "4.2", gate `null`
    - `probe_wrong_incremental_is_unknown_firmware`
    - `probe_low_battery_gate`: battery 29 → `lowBattery`; 30 → none
    - `probe_prefers_profile_match_among_many`: a phone plus the tablet → the tablet's serial
    - `probe_rooted_reports_magisk_and_bootsvc`
    - `poller_emits_only_on_change`
  - `tests/magisk_adb.rs`:
    - `push_patch_inputs_order_matches_build_and_flash`: the Push calls' remote list equals the 7 `MGK_DIR/*` names, then `init.bootsvc.rc`, `stock_boot.img`, `/data/local/tmp/bootsvc.sh`, `/data/local/tmp/service.sh`
    - `run_patch_sends_exact_patch_cmd`: the Shell call cmd `== PATCH_CMD`
    - `run_patch_failure_is_patch_failed`
    - `stock_check_exit_codes`: 0 → true, 1 → false, 2 → false, 9 → `internal`, and cleanup ran each time
    - `hook_present_true_false`
- [ ] **Step 2: Run** `cargo test --test device --test magisk_adb`. Expected: FAIL.
- [ ] **Step 3: Implement** `fakes.rs` (compiled under `#[cfg(any(test, feature = "demo"))]`, with feature `demo` enabled by default so demo mode ships), then `device.rs` and `poller.rs`.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): fakes, device probe, poller"`

---

### Task 8: Settings, keep-awake, host setup (WinUSB driver / udev)

**Files:** Create `src-tauri/src/settings.rs`, `src-tauri/src/host/mod.rs`, `src-tauri/src/host/keep_awake.rs`, `src-tauri/src/host/setup.rs`.

**Interfaces:**
- `settings.rs`:
  - `pub const RISK_NOTICE_VERSION: u32 = 1;`
  - `SettingsStore::new(path)` and `SettingsStore::default_path()`: `INKROOT_CONFIG_DIR` or `dirs::config_dir()/Inkroot`, then `/settings.json`.
  - `get() -> Settings`, which returns defaults (`null`s) when the file is missing or unparseable.
  - `apply(&self, patch: SettingsPatch) -> Result<Settings, FlowError>`: an atomic write. It rejects `acceptedRiskVersion > RISK_NOTICE_VERSION`.
  - `fn risk_accepted(s:&Settings) -> bool`
- `host/keep_awake.rs`: `trait KeepAwake: Send+Sync { fn hold(&self, why:&str) -> Box<dyn std::any::Any> }`. The guard is created and dropped on the flow thread.
  - macOS: spawn `caffeinate -i -w <own pid>` and kill it on drop.
  - Windows: `SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED)`, reset to `ES_CONTINUOUS` on drop.
  - Linux: if `systemd-inhibit` is on PATH, spawn `systemd-inhibit --what=sleep:idle --who=Inkroot --why=<why> sleep infinity` and kill it on drop. Otherwise no-op.
- `host/setup.rs`: `trait HostSetup: Send+Sync`:
  - `fn needs_edl_driver(&self, usb:&EdlUsbInfo) -> bool`, `fn install_edl_driver(&self) -> Result<(), FlowError>`
  - `fn needs_udev(&self) -> bool`, `fn install_udev(&self) -> Result<(), FlowError>`
  - Windows:
    - The driver is needed unless `usb.driver` equals `WinUSB`, case-insensitive.
    - Install launches `tools/wdi-simple.exe` elevated (ShellExecuteExW verb `runas`, wait), with `--vid 0x05C6 --pid 0x9008 --type 0 --name "Qualcomm HS-USB QDLoader 9008"`. The implementer confirms these flags against the built binary's `--help` and records the result in the report.
    - A refused UAC prompt or non-zero exit → `driverMissing`, message "Inkroot needs permission to install a small USB driver so it can talk to your tablet."
  - Linux:
    - `udev` is needed when no file in `/etc/udev/rules.d/` and `/usr/lib/udev/rules.d/` contains `9008`.
    - Install writes `pub const UDEV_RULE: &str = "SUBSYSTEM==\"usb\", ATTRS{idVendor}==\"05c6\", ATTRS{idProduct}==\"9008\", MODE=\"0666\", TAG+=\"uaccess\"\n";` to a temp file, then runs `pkexec sh -c 'install -m 644 <tmp> /etc/udev/rules.d/51-inkroot-edl.rules && udevadm control --reload-rules && udevadm trigger'`. A failure → `driverMissing`.
    - The implementer compares this rule with `edl.rules` at the pinned edl commit and notes any difference in the report.
  - macOS: always `false`/`Ok`.

- [ ] **Step 1: Write the failing tests** (unit, in each module):
  - `settings_defaults_when_missing`
  - `settings_patch_absent_keeps_null_clears`: a `SettingsPatch` JSON `{}` leaves values, and `{"hasRescueCable":null}` clears them
  - `settings_rejects_future_risk_version`
  - `risk_accepted_only_at_current_version`
  - `windows_driver_needed_unless_winusb`: `None`/`"usbser"` → true, `"WinUSB"`/`"winusb"` → false (pure helper `driver_is_winusb`)
  - `udev_rule_text_exact`
  - `udev_needed_scans_dirs`: tempdir fixture
- [ ] **Step 2: Run** `cargo test settings host`. Expected: FAIL.
- [ ] **Step 3: Implement.** OS-specific code goes behind `#[cfg(target_os = …)]`, so the pure helpers stay testable on every OS.
- [ ] **Step 4: Run** `cargo test settings host`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): settings, keep-awake, driver/udev setup"`

---

### Task 9: Flow engine and shared steps

**Files:** Create `src-tauri/src/flows/mod.rs`, `src-tauri/src/flows/common.rs`, `src-tauri/tests/engine.rs`.

**Interfaces:**
- `flows/mod.rs`:
  - `struct Deps { adb: Arc<dyn Adb>, edl: Arc<dyn Edl>, usb: Arc<dyn UsbProbe>, host: Arc<dyn HostSetup>, awake: Arc<dyn KeepAwake>, clock: Arc<dyn Clock>, emitter: Arc<dyn Emitter>, log: Arc<LogStore>, library: BackupLibrary, settings: Arc<SettingsStore>, profiles: Vec<DeviceProfile>, tools: ToolPaths, timeouts: Timeouts, poller: Option<Arc<Poller>> }`
  - `FlowEngine::new(deps) -> Arc<FlowEngine>`
  - `start(&self, kind, opts) -> Result<String, FlowError>`. In order:
    1. If a flow is active → `busy` ("Inkroot is already working on something. Let it finish first.").
    2. If kind ∈ {root, unroot, rescue} and `!risk_accepted` → `internal` with the contract's exact message "Please read and accept the risk notice first.".
    3. Run `tools.require_for`.
    4. Pause the poller, spawn a thread running the flow, and return a `uuid` v4 string.
  - When the flow ends: `log.set_flow(None)` and resume the poller.
  - `answer(&self, flow_id, prompt_id, option_id)` ignores a stale prompt id.
  - `cancel(&self, flow_id) -> Result<(), FlowError>`: `internal` "Inkroot is writing to your tablet right now. Cancelling could damage it." while writing. Otherwise it sets the cancel token, and the flow ends `cancelled`.
  - `writing(&self) -> bool`, `current(&self) -> Option<FlowState>`
  - `struct FlowCtx`, passed to flow fns:
    - `id()`, `deps()`, `kind`, `set_kind(FlowKind)` (used by reinstall → root)
    - `step(StepId, index:u8, count:u8)` emits a `flow://state` snapshot (status running)
    - `progress(done, total, label)`
    - `ask(PromptKind) -> Result<String /*option id*/, Interrupt>` blocks until an answer or cancel, using options from `prompt_options`
    - `set_writing(bool)`, `check_cancel() -> Result<(), Interrupt>`, `cancel_token()`
  - Flow fns return `Result<FlowResult, FlowEnd>`, where `enum FlowEnd { Failed(FlowError), Cancelled }`. The engine maps the result to the final status.
  - `fn prompt_options(kind: PromptKind) -> Vec<PromptOption>`. Option ids are fixed by the contract. Labels are defaults (the UI's copy table may override them):

    | kind | options (id · label · emphasis) |
    |---|---|
    | cableCheck | have · "Yes, I have one" · primary; whereToGet · "Show me where to get one" · secondary; continueWithout · "Continue without one" · danger |
    | cableCheckConfirm | confirmContinue · "I understand, continue" · danger; back · "Go back" · primary |
    | chooseSlot | a · "Slot A" · secondary; b · "Slot B" · secondary |
    | keepWaitingOrRescue | keepWaiting · "Keep waiting" · primary; goToRescue · "Go to Rescue" · secondary |
    | installDriver | install · "Install driver" · primary; cancel · "Cancel" · secondary |
    | installUdev | install · "Allow access" · primary; cancel · "Cancel" · secondary |
    | edlNotFound | retry · "Try again" · primary; cancel · "Cancel" · secondary |
    | retryWrite | retry · "Try again" · primary; cancel · "Cancel" · secondary |
- `flows/common.rs`:
  - `fn check_device(ctx, require_rooted: Option<bool>) -> Result<(String, DeviceInfo, DeviceProfile), FlowEnd>`. Built on `device::probe`, mapping:
    - `None` → `noDevice` (`reconnect`)
    - `Unauthorized` → `unauthorized`
    - gates → `unknownModel` / `unknownFirmware` / `lowBattery` (`chargeDevice`)
    - `require_rooted == Some(false)` and rooted → `alreadyRooted`
    - `Some(true)` and not rooted → `notRooted`
  - `fn enter_edl(ctx, serial: Option<&str>) -> Result<(), FlowEnd>`:
    - If EDL isn't already present and a serial is given, run `adb reboot edl`.
    - Poll `usb.edl_device()` every 1 s up to `wait_edl`.
    - When found, if `host.needs_edl_driver` → ask `installDriver` (cancel → Cancelled). On Linux, if `needs_udev` → ask `installUdev`.
    - On timeout → ask `edlNotFound` (retry loops, cancel → Cancelled).
  - `fn write_and_verify(ctx, part:&str, img:&Path, expected_sha:&str, scratch_dir:&Path) -> Result<(), FlowEnd>`, a loop:
    1. Re-hash `img` → mismatch `badImage`.
    2. `set_writing(true)`, `edl.write` with progress.
    3. `edl.read` back into `scratch_dir/.verify-<part>.img`.
    4. `set_writing(false)`.
    5. On a write error or hash mismatch → ask `retryWrite`. Retry loops; cancel → `Failed(hashMismatch)` with message "The copy on your tablet doesn't match yet, so Inkroot didn't restart it. Re-seat the cable and try again, or use Rescue." and suggestedAction `goToRescue`.

    **It never calls `edl.reset` on a failure path.** On a match: delete the verify file, then `edl.reset` (a reset error is logged as info, not fatal).
  - `fn wait_boot(ctx, serial:&str) -> Result<(), FlowEnd>` polls `adb.devices()` until the serial is `Device`, up to `wait_boot`. On timeout → ask `keepWaitingOrRescue`: keepWaiting repeats, goToRescue → `Failed(bootTimeout)` with message "Your tablet hasn't restarted yet." and suggestedAction `goToRescue`.
  - `fn cable_check(ctx) -> Result<(), FlowEnd>`: skip if `settings.has_rescue_cable == Some(true)`. Otherwise ask `cableCheck`:
    - have → persist `true`
    - whereToGet → ask again (the UI normally handles this locally)
    - continueWithout → ask `cableCheckConfirm`: confirmContinue → persist `false` and continue; back → ask `cableCheck` again
  - `fn with_keep_awake<T>(ctx, f) -> T`

- [ ] **Step 1: Write the failing tests** in `tests/engine.rs`. Each flow under test is a tiny test-only flow fn composed of the shared steps, or a real flow added in later tasks:
  - `start_rejects_second_flow_busy`
  - `risk_gate_blocks_root_unroot_rescue_until_accepted`: code `internal`, exact message
  - `cancel_rejected_while_writing`
  - `stale_prompt_answer_ignored`
  - `write_and_verify_mismatch_prompts_retry_and_never_resets`: `write_corrupts_times=1`, answer cancel → failed `hashMismatch`, and the call log has **no** `EdlReset`
  - `write_retry_then_match_resets_exactly_once`
  - `enter_edl_timeout_prompts_edl_not_found_then_retry_succeeds`
  - `enter_edl_windows_driver_prompt`: `FakeHost{needs_driver:true}` → `installDriver` prompt, install recorded
  - `cable_check_skips_when_has_cable_true`
  - `cable_check_continue_requires_confirm`
  - `cable_check_back_reasks`
  - `wait_boot_timeout_offers_keep_waiting_or_rescue`
  - `keep_awake_held_during_write`
- [ ] **Step 2: Run** `cargo test --test engine`. Expected: FAIL.
- [ ] **Step 3: Implement** `flows/mod.rs` and `flows/common.rs`. Prompts use a `std::sync::mpsc` channel per active flow. Snapshots are always full `FlowState`s.
- [ ] **Step 4: Run** `cargo test --test engine`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): flow engine and shared steps"`

---

### Task 10: Back up and Import flows

**Files:** Create `src-tauri/src/flows/backup.rs`, `src-tauri/src/flows/import.rs`, `src-tauri/tests/flow_backup.rs`, `src-tauri/tests/flow_import.rs`.

**Interfaces:**
- `pub fn run_backup(ctx) -> Result<FlowResult, FlowEnd>`, made of reusable pieces:
  - `pub(crate) fn take_backup(ctx, serial, info:&DeviceInfo, profile, index_base:u8, count:u8) -> Result<(String /*id*/, PathBuf, Manifest), FlowEnd>`. Root reuses it.
- Backup steps (4):
  1. `checkDevice`: `check_device(ctx, None)`, then `library.create_dir` (**before** EDL).
  2. `enterEdl`
  3. `readPartitions`: read `boot_a`, `boot_b`, `vbmeta_a`, `vbmeta_b` with progress. On a read error: best-effort `edl.reset`, then `Failed(edlTimeout, reconnect)`. No manifest is written as verified.
  4. `verifyBackup`:
     - Re-read the active `boot_<slot>` to `.verify`, check equal sha, `validate_boot_image`, then `edl.reset` and `wait_boot`.
     - Stock evidence:
       - The device was rooted at check time → `stock=false`, `NotStockRooted`.
       - Else the known hash → `KnownHash`.
       - Else `stock_check` (magiskboot extracted from the bundled APK) → `MagiskbootTest` or `NotStockFailedTest`.
     - Write the manifest with `verified=true`.
- `pub fn run_import(ctx) -> Result<FlowResult, FlowEnd>` (3 steps):
  1. `pickImage`: `opts.image_path` is required, else `badImage`; then `validate_boot_image`.
  2. `stockCheck`:
     - A known hash → stock.
     - Else, with an authorized device matching a profile → `stock_check`. False → `Failed(notStock)` with message "This file isn't an original (stock) startup file, so Inkroot can't use it as your safety copy."
     - Else, with no device → keep it as `stock=false, Unchecked`, Rescue-only, with serial `"unassigned"`.
  3. `chooseSlot`/`saveImport`:
     - Slot = `opts.slot`, else `slot_from_filename`, else the device slot, else ask `chooseSlot`.
     - `copy_verified` into `create_dir(serial)` as `boot_<slot>.img`, with a manifest `source: imported`, `verified: true`, `imported_from` = the original path.
     - Serial = the connected device's serial if present.

- [ ] **Step 1: Write the failing tests:**
  - `backup_happy_path_writes_verified_stock_manifest`: 4 files, shas match the world partitions, stock true, `StockEvidence::MagiskbootTest`, device back in System
  - `backup_known_hash_skips_magiskboot`: test profile with that hash, no `STOCK_CHECK_CMD` call
  - `backup_on_rooted_device_marks_not_stock`
  - `backup_read_failure_resets_and_fails_cleanly`
  - `backup_unwritable_root_fails_before_edl`: read-only lib dir → failed at `checkDevice`, no `Reboot{Edl}` call
  - `backup_verify_mismatch_fails_not_verified`
  - `import_known_hash_without_device_is_stock`
  - `import_unknown_hash_without_device_is_rescue_only`: stock false, serial "unassigned"
  - `import_magisk_patched_rejected_not_stock`
  - `import_bad_magic_rejected_bad_image`
  - `import_slot_from_filename`: `boot_a.stock.img` → slot A
  - `import_asks_slot_when_unknown`
- [ ] **Step 2: Run** `cargo test --test flow_backup --test flow_import`. Expected: FAIL.
- [ ] **Step 3: Implement** both flows and register them in the engine's dispatch.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): backup and import flows"`

---

### Task 11: Root flow

**Files:** Create `src-tauri/src/flows/root.rs`, `src-tauri/tests/flow_root.rs`.

**Interfaces:**
- `pub fn run_root(ctx) -> Result<FlowResult, FlowEnd>`
- `pub(crate) fn root_from_step2(ctx, serial, info, profile) -> Result<FlowResult, FlowEnd>`. Reinstall reuses it.
- Visible groups (count 4):
  1. `checkDevice`: `require_rooted = Some(false)`.
  2. Safety copy: `library.stock_for(serial, display_id, slot)`. If there's none, `take_backup` with indices folded into step 2. If the resulting manifest isn't `stock && verified` → `Failed(noStockBackup)` with message "Inkroot needs an original safety copy of your tablet first." and suggestedAction `importBackup`.
  3. `patch` → `cableCheck` → `enterEdl` → `write` → `verifyWrite` → `reset`:
     - `verify_file` the stock `boot_<slot>.img`.
     - Record `before = bootsvc_line_count`.
     - `extract_payload` → `push_patch_inputs` (rc/bootsvc/service from `ToolPaths::boot_service`) → `run_patch` into `<backup dir>/patched-boot.img`, then update `manifest.patched`.
     - `cable_check`, `enter_edl(serial)`, then `write_and_verify("boot_<slot>", patched, sha)`.
  4. `waitBoot` → `verifyRoot`. Poll every 5 s up to `wait_bootsvc` until all of:
     - `parse_magisk_version(magisk -c) == profile.magisk.version`
     - `bootsvc_line_count > before`
     - `bootsvc_ok`

     If that fails → `Failed(verifyFailed)` with message "Your tablet restarted, but root didn't start. Your safety copy is ready if you want to go back." and suggestedAction `null`.
  - Result: `{ backup: summary, magiskVersion: "30.7" }`.

- [ ] **Step 1: Write the failing tests:**
  - `root_happy_path_call_order`: Shell `PATCH_CMD` precedes `cableCheck` (answered `have`), then `Reboot{Edl}`, `EdlWrite{boot_b}`, `EdlRead{boot_b}`, exactly one `EdlReset`; result magiskVersion "30.7"; `manifest.patched` set
  - `root_uses_existing_stock_backup_without_new_reads`: no `EdlRead` before the patch
  - `root_takes_backup_first_when_none`
  - `root_already_rooted_fails`
  - `root_refuses_non_stock_backup_suggests_import`
  - `root_patch_failure_makes_no_edl_calls`
  - `root_hash_mismatch_never_resets`
  - `root_boot_timeout_offers_rescue`: `boot_hangs` → `keepWaitingOrRescue` answered goToRescue → failed `bootTimeout`, `goToRescue`
  - `root_verify_failure`: the patch marker is absent because the fake is set to drop it
  - `root_skips_cable_prompt_when_has_cable_true`
  - `root_refuses_tampered_backup`
  - `root_patch_env_is_verity_safe`: the Shell cmd contains `KEEPVERITY=true KEEPFORCEENCRYPT=true`
- [ ] **Step 2: Run** `cargo test --test flow_root`. Expected: FAIL.
- [ ] **Step 3: Implement** `root.rs`.
- [ ] **Step 4: Run** `cargo test --test flow_root`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): root flow"`

---

### Task 12: Rescue, Return to stock, Reinstall boot service

**Files:** Create `src-tauri/src/flows/{rescue,unroot,reinstall}.rs`, `src-tauri/tests/flow_rescue.rs`, `src-tauri/tests/flow_unroot.rs`, `src-tauri/tests/flow_reinstall.rs`.

**Interfaces:**
- `run_rescue(ctx)` (4 steps):
  1. `pickImage`/`chooseSlot`:
     - With `opts.backup_id`: `library.get`, slot = `opts.slot` or the manifest slot, then `verify_file`.
     - With `opts.image_path`: `validate_boot_image`, slot = `opts.slot`, else the filename, else ask `chooseSlot`. The sha is computed fresh.
     - Neither → `badImage`.
  2. `cableGuide`, then immediately `waitEdl`: `enter_edl(ctx, None)`. No adb is used, and the 60 s timeout leads to the `edlNotFound` prompt.
  3. `write` → `verifyWrite` → `reset` via `write_and_verify`.
  4. `done`. Optionally watch for adb for up to 180 s and log "Tablet is back" (never fails).
- `run_unroot(ctx)` (3 steps):
  1. `checkDevice`, `require_rooted = Some(true)`.
  2. `stock_for`, else `Failed(noStockBackup, importBackup)`. Then `verify_file`, `enter_edl(serial)`, `write_and_verify`.
  3. `waitBoot`, then confirm `magisk -c` now exits non-zero (else `verifyFailed`).
- `run_reinstall(ctx)` (2 steps):
  1. `checkDevice` with `Some(true)`.
  2. Hook detection: present iff the device's bootsvc log has any `uid=0`+`u:r:magisk:s0` boot line, or the newest manifest for this serial has `patched` and `hook_present(patched)` is true.
     - **Present:** `pushScripts` pushes `bootsvc.sh` → `DEVICE_BOOTSVC` and the default service → `DEVICE_SERVICE`. Record `before`, run `adb reboot`, then `waitBoot`, then `verifyRoot` (line count increased and `bootsvc_ok`). There are no edl calls.
     - **Absent:** check `risk_accepted` (else the same `internal` risk error), then `set_kind(Root)` and `root_from_step2`. No stock backup → `Failed(noStockBackup, importBackup)`.

- [ ] **Step 1: Write the failing tests:**
  - `rescue_from_backup_happy`: the world starts in `Edl` mode (bricked), result done, one reset
  - `rescue_loose_file_slot_from_filename`
  - `rescue_waits_then_edl_not_found_prompt`
  - `rescue_bad_image_rejected`
  - `rescue_never_calls_adb`
  - `unroot_happy_confirms_not_rooted`
  - `unroot_refuses_tampered_backup`: Review Focus 1, with no edl calls
  - `unroot_not_rooted_fails`
  - `unroot_without_stock_backup_suggests_import`
  - `reinstall_hook_present_pushes_scripts_without_edl`
  - `reinstall_hook_absent_switches_kind_to_root`: a FlowState with kind "root" is emitted under the same flowId
  - `reinstall_hook_absent_no_backup_suggests_import`
- [ ] **Step 2: Run** `cargo test --test flow_rescue --test flow_unroot --test flow_reinstall`. Expected: FAIL.
- [ ] **Step 3: Implement** the three flows and register them.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): rescue, unroot, reinstall flows"`

---

### Task 13: Demo mode

**Files:** Create `src-tauri/src/demo.rs`, `src-tauri/tests/demo.rs`.

**Interfaces:**
- `pub fn demo_enabled() -> bool` (env `INKROOT_DEMO=1`).
- `pub enum DemoScenario { Fresh, Rooted, Bricked, RootedNoBackup }`, from env `INKROOT_DEMO_SCENARIO` (`fresh` | `rooted` | `bricked` | `rooted-no-backup`, default `fresh`).
- `pub fn demo_deps(scenario, emitter, speed: f32) -> Deps`:
  - Backed by `FakeWorld` and `SystemClock`. Fake tools sleep `speed ×` these realistic times: read 4 s per partition, write 6 s, reset-to-boot 5 s, patch 3 s.
  - The backup library is a temp dir under `std::env::temp_dir()/inkroot-demo`.
  - Settings are in memory.
  - `Bricked` starts `Off` and switches to `Edl` 3 s after `enter_edl` starts polling.
- `AppInfo.demo = true` when enabled.

- [ ] **Step 1: Write the failing test** `demo_all_flows_complete_at_speed_zero`. For each scenario, run its natural flow (fresh → root; rooted → unroot via a pre-seeded stock backup; bricked → rescue with a seeded backup; rooted-no-backup → import with a known-hash file) at `speed = 0.0`, and assert status `done`.
- [ ] **Step 2: Run** `cargo test --test demo`. Expected: FAIL.
- [ ] **Step 3: Implement** `demo.rs`.
- [ ] **Step 4: Run** `cargo test --test demo`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(core): demo mode"`

---

### Task 14: Tauri commands, events, close guard and app wiring

**Files:** Create `src-tauri/src/commands.rs`. Modify `src-tauri/src/events.rs` (add `TauriEmitter`), `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `src-tauri/Cargo.toml`. Create `src-tauri/tests/commands.rs`.

**Interfaces:**
- `struct AppState { engine: Arc<FlowEngine>, poller: Arc<Poller>, library: BackupLibrary, settings: Arc<SettingsStore>, log: Arc<LogStore>, info: AppInfo }`.
- Commands, named exactly per the contract:
  - `get_app_info`
  - `get_device` → `poller.latest()`
  - `list_backups{serial}`
  - `start_flow{kind, options}`
  - `answer_prompt{flowId, promptId, optionId}`
  - `cancel_flow{flowId}`
  - `get_log{flowId}`
  - `reveal_path{path}`: via `tauri-plugin-opener`'s reveal-item-in-dir. It refuses paths outside the backup root with `internal`.
  - `get_settings`
  - `set_settings{settings: SettingsPatch}` → `Settings`

  Every command returns `Result<T, FlowError>` (serialized as the contract's `FlowError`).
- `TauriEmitter(AppHandle)` emits the four event names.
- `lib.rs` `run()`:
  - Registers `tauri-plugin-opener` and `tauri-plugin-dialog`.
  - Setup: `ToolPaths::resolve(app.path().resource_dir().ok())`. Builds `demo_deps` or the real deps: `RealAdb`, a `RealEdl` with the profile loader path, `NusbProbe`, and the per-OS host/awake impls. Then starts the poller and manages `AppState`.
  - Close guard: on `WindowEvent::CloseRequested`, if `should_block_close(&engine)` → `api.prevent_close()` and re-emit the current `flow://state`. `pub fn should_block_close(engine:&FlowEngine) -> bool { engine.writing() }`.
- `capabilities/default.json`: permissions `core:default`, `dialog:allow-open`, `opener:allow-open-url`, `opener:allow-reveal-item-in-dir`, `core:window:allow-close`, `core:window:allow-destroy`. These are the UI plan's handoff list, plus reveal.

- [ ] **Step 1: Write the failing tests** in `tests/commands.rs`. They call the inner fns that the `#[tauri::command]` wrappers delegate to, so no Tauri runtime is needed:
  - `close_blocked_while_writing`: Review Focus 5
  - `start_flow_busy_maps_to_flow_error_json`: JSON `{"code":"busy",…,"suggestedAction":null}`
  - `list_backups_filters_by_serial_newest_first`
  - `reveal_path_outside_backup_root_refused`
  - `set_settings_partial_patch`
  - `app_info_reports_demo_and_os`
- [ ] **Step 2: Run** `cargo test --test commands`. Expected: FAIL.
- [ ] **Step 3: Implement** the commands, `TauriEmitter`, wiring and capabilities.
- [ ] **Step 4: Full verification** from `app/`: `npm run build && npm run check:rust`. Expected: fmt clean, clippy with no warnings, all tests pass. Then a manual smoke check: `INKROOT_DEMO=1 npm run tauri dev` opens the window, and the placeholder lists DeviceStatus `adb` / Note Max.
- [ ] **Step 5: Commit** `git commit -m "feat(core): tauri commands, events, close guard, wiring"`

---

## Completion note (coordinator)

Tasks 1–6 (to `extract_payload`) were written by the original plan agent. The rest of Task 6 and Tasks 7–14 were completed by the coordinator after the agent was cut off. Rulings the coordinator took while completing it:
- The cable answer is skipped only when `hasRescueCable == true`. A previous "continue without one" asks again next time, which is safer.
- `magiskboot cpio test` exit 1 and 2 are both "not stock". Other codes are errors.
- An import with no device and no known hash is Rescue-only under serial `unassigned`.
- Reinstall's switch to root re-checks the risk notice.
- The `wdi-simple` flags and the udev rule are to be confirmed against the real tools by the implementer.
