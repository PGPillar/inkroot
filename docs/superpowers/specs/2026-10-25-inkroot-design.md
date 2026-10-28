# Inkroot — design

*2026-10-25 · status: approved by owner in conversation; builder-check questions resolved*

Inkroot turns this repository (today `n0dal/boox-note-max-root`, a set of guides and shell
scripts) into a **one-click desktop app plus a polished public home** for rooting and rescuing the
ONYX BOOX Note Max. The guides and scripts in `docs/`, `scripts/` and `boot-service/` are the
source of truth for *how* every device operation is done; the app automates exactly those steps
and must not invent new ones.

---

## 1. Intent

Two people find this project:

1. **"I want root on my Note Max."** Their tablet works. They want Magisk without learning adb,
   EDL, verity flags or SELinux.
2. **"My Note Max is stuck on the ONYX logo and I'm panicking."** They need calm, exact
   instructions and a tool that writes a good `boot` back.

Both may have **zero technical ability**. The reference experience is **checkra1n**: one window,
it detects the device, it tells you exactly what to press and when, one obvious button per
screen. Research (2026-10-25) found **no GUI tool of any kind for rooting or un-bricking any BOOX
model** — every existing method is terminal commands spread across MobileRead threads and
single-model GitHub repos — and **no Note Max root guide other than this repo**. Recent threads
show demand: Onyx removed `fastboot flash/boot`, and 4.1.x users hang on the logo after Magisk.

### Success looks like
- A non-technical Note Max owner downloads Inkroot, double-clicks it, and ends with working root
  (Magisk daemon + the boot-service running as root) without opening a terminal.
- A panicking owner with a stuck tablet and a backup gets it booting again by following the
  app's illustrated cable steps.
- Someone googling "boox root", "boox note max root", "boox stuck on logo", "unbrick boox",
  "boox magisk", "boox edl" lands on the Inkroot site or repo.
- The GitHub page looks like a product, not a notes dump.

### Goals
- Desktop app for **macOS, Windows and Linux** covering four jobs: **Back up**, **Root**,
  **Rescue (un-brick)**, **Return to stock**.
- Rename to **Inkroot**; rewrite README; GitHub Pages website; repo SEO (description, topics,
  social preview); signed releases.
- Architecture ready for more BOOX models via per-device **profiles**, shipping Note Max only.

### Non-goals (v1)
- Any model other than the Note Max on firmware `TKQ1.230721.002` (Onyx 4.2).
- Rescuing a bricked device that has **no backup** (see §9, future ideas).
- Fixing the Magisk app's broken interactive `su` (documented as unfixable on Onyx 4.2; the
  boot-service is the answer).
- Mac App Store / Microsoft Store distribution.
- Auto-update inside the app (the app may *check* GitHub Releases and show a "new version"
  note; it does not self-update).

### Anti-goals (failure even if it technically works)
- **Ever writing to the device without a verified stock backup** of that exact device (root
  flow). The backup is the lifeline.
- **Patching with verity disabled.** `KEEPVERITY=true KEEPFORCEENCRYPT=true` is non-negotiable
  and must not be configurable in the UI.
- A destructive action as the default or one mis-click away (MiFlash's "clean all and lock"
  default is the cautionary tale).
- Wall-of-text logs as the primary UI, cryptic PASS/FAIL, or jargon on the main path.
- Proceeding on an unknown device or unknown firmware.
- An ugly, generic, templated look.

---

## 2. Decisions made with the owner

| Topic | Decision |
|---|---|
| Name | **Inkroot**. (An unrelated 33★ notes app named "InkRoot" exists on GitHub; accepted. Always pair the name with "BOOX" in titles/descriptions.) |
| Repo | Renamed `n0dal/boox-note-max-root` → **`n0dal/inkroot`** before implementation (owner approved; GitHub auto-redirects old URLs). All links target `inkroot`. |
| Jobs in the app | All four: Back up, Root, Rescue, Return to stock — done the way this repo's docs/scripts do them. Plus **Import an existing backup** (§3.1b). |
| Boot-service | **Always baked into the root.** Without it root is unusable on Onyx 4.2. |
| Magisk | **Pinned to 30.7** — the official release asset `Magisk-v30.7.apk`, SHA-256 `e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5` (matches GitHub's published digest). This is what the owner's working root was built from. **Not** `app-debug.apk`. (The repo's docs say "30.x" and use 30.1 in examples; update those examples to 30.7.) |
| Rescue-cable warning | Before Root flashes, one screen asks "Do you have a rescue cable?" with a link to buy one; **"Continue without one" needs a deliberate second confirmation**. |
| Look | **"Panel"** direction (see §6) — chosen from three mockups. |
| Binaries | **Bundle everything** in the release builds (adb, edl + loaders, Magisk 30.7). Not committed to git — fetched and hash-pinned at build time. |
| macOS signing | **Developer ID + notarization** with the owner's existing Apple Developer account. No App Store, no human review. |
| Windows signing | **Unsigned.** Download page shows the SmartScreen "More info → Run anyway" step with screenshots. |
| Linux | **Yes**, AppImage. |
| More models | Note Max now; **profile system** so others can be added by PR. |
| GitHub glow-up | Rewritten README, project website, repo SEO, (research done). |
| Agents | Implementation will be executed by subagents; this doc and the plan must be explicit enough that they build it as discussed. |

### Verified hardware facts (from the owner's working setup)
- **macOS EDL works over a normal USB-C cable** when entered with `adb reboot edl`: the
  2026-08-30 backup readback ran from the Mac (a local `bkerler/edl` checkout). Backup, root and unroot need
  no deep-flash cable.
- **Recovery** (deep-flash cable) was done from a **Linux (Proxmox) host's native USB-A port**; a
  USB-A→C adapter on the Mac dropped the data lines and failed. Keep `docs/02`'s warning.
- **Device properties** (`getprop`): `ro.product.model` = `ro.product.device` = `NoteMax`,
  `ro.product.manufacturer` = `ONYX`, `ro.build.id` = `ro.build.display.id` = `TKQ1.230721.002`,
  `ro.build.version.incremental` = `226`, `ro.build.fingerprint` =
  `ONYX/TabBoox/TabBoox:13/TKQ1.230615.001/GV2.027.SQ83A:user/release-keys`,
  `ro.board.platform` = `msmnile`, `ro.boot.slot_suffix` = `_b`.
- **edl:** `bkerler/edl` commit `2f8e89a848afaaef68997fcbcb5b178d958d497b` (client V3.62), Loaders
  submodule `bf0d8017eb97464530114a348c5157a6ea6a3372`. With no loader given it detected
  `SM8150`, HWID `0x000a50e100000000`, "possibly unfused", and auto-picked
  `Loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`.
- **Stock boot hashes** for this firmware (owner's unit): `boot_b` =
  `2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc`, `boot_a` =
  `1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09`; 100,663,296 bytes (96 MiB) each.
  Only `boot_b` (the active slot on `TKQ1.230721.002`) is trusted as a known original. `boot_a`
  is **not** trusted: it is the inactive slot and most likely the previous update's boot.
- The owner's tablet is **currently rooted**; their stock backup lives at
  `boot_b.stock.img`, kept privately (not in this repo).

---

## 3. What the app does — operations, exactly

All device operations below mirror `scripts/patch-boot.sh`, `scripts/edl-restore.sh`,
`boot-service/build-and-flash.sh` and the docs. Where they differ, the docs win; flag the
conflict rather than guess.

### 3.0 Device detection & gates (every flow)
Read over adb (device booted, USB debugging on):
- `ro.product.model`, `ro.product.device`, `ro.build.id`, `ro.build.display.id`,
  `ro.boot.slot_suffix`, `ro.serialno`, battery level (`dumpsys battery`).
- Match against a **device profile** (§5). Gates, in order, each with a plain-language screen:
  1. **No device** → "Plug in your Note Max", followed by an illustrated **"Turn on USB
     debugging"** guide. The owner found this the hardest step: the standard "tap the build
     number" route isn't reachable from Onyx's normal Settings UI. Show two methods, in this
     order:
     - **A · Onyx menu:** *Settings → Apps → ⋮ (top-right menu) → USB Debug Mode*. This comes
       from the owner's other project's notes and isn't hardware-verified yet.
     - **B · "If you can't find it":** install a free launcher app that opens Android's hidden
       settings. Recommend **Activity Launcher**, which is open source and available on F-Droid
       and Google Play. On a BOOX, Google Play may first need enabling under Onyx *Settings →
       Apps*. Then: open Android **Settings** (`com.android.settings`) → *About tablet* → tap
       **Build number** 7 times → back → *System → Developer options* → turn on
       **USB debugging**.
     After either method, tell them to tap **Allow** on the "Allow USB debugging?" prompt and
     tick "Always allow from this computer". The guide is reachable from the home screen and
     the gate screen, and it auto-advances as soon as adb sees the device. Hardware pass: find
     out which method works on firmware 4.2 and put that one first. Fix the same instructions
     in `docs/01`, `docs/03` §7 and the README.
  2. **Unauthorized** → "Tap *Allow* on your tablet."
  3. **Unknown model** → blocked: "Inkroot only supports the BOOX Note Max right now."
  4. **Unknown firmware** → blocked, explaining that other firmware may brick, with a link to
     open an issue. A device is supported only if **all** of `ro.build.id`,
     `ro.build.display.id`, `ro.build.version.incremental` and `ro.build.fingerprint` match a
     verified entry in the profile (values in §2). Onyx reuses Android build IDs across
     releases, so `ro.build.id` alone is not enough.
  5. **Battery < 30 %** → "Charge to at least 30 % first."
  6. **Already rooted** (`/debug_ramdisk/magisk -c` succeeds) → the root flow is replaced by
     "Already rooted ✓" with options *Reinstall boot service* / *Return to stock*. Any *new*
     backup taken now is labelled **not stock** (it would capture a patched boot), so the
     screen points to **Import an existing backup** (§3.1b) if no stock backup is on file.
     *Reinstall boot service* first checks whether the hook is baked in
     (`/data/local/tmp/bootsvc.log` exists with a recent `uid=0 … u:r:magisk:s0` line, or the
     backed-up patched image's ramdisk lists `overlay.d/init.bootsvc.rc`):
     - **Hook present:** re-push `bootsvc.sh` and the service to `/data/local/tmp`, then reboot.
       No EDL step, no write to boot.
     - **Hook absent** (rooted with plain `patch-boot.sh` or another guide): explain this, then
       run the full Root flow from the stock backup, with every Root gate.

While any write is running: keep the host awake (macOS `caffeinate -w <pid>`, Windows
`SetThreadExecutionState`, Linux `systemd-inhibit` if present), and disable closing the window
without a confirm.

### 3.1 Back up (also step 1 of Root; always required before Root)
1. `adb reboot edl` → wait for USB `05c6:9008` (timeout 60 s; on timeout, guide the user).
2. `edl --memory=ufs r boot_a`, `boot_b`, `vbmeta_a`, `vbmeta_b` into the backup folder.
3. **Verify:** read the active slot's `boot_<slot>` a second time and compare SHA-256; check the
   `ANDROID!` boot magic at offset 0. Write `manifest.json`: serial, model, firmware, active
   slot, file names, sizes, SHA-256, timestamp, `stock: true|false`, Inkroot version.
4. `edl reset` → wait for adb to return.
- Location: `~/Documents/Inkroot/<serial>/<YYYY-MM-DD_HHMM>/` (Windows: `Documents\Inkroot\…`).
  The Done screen shows the path with a "Show in Finder/Explorer" button and tells the user to
  keep a copy somewhere safe. *Never* upload anywhere.
- A backup counts as **verified** only if step 3 passed. Root refuses to continue without a
  verified, `stock: true` backup for the connected serial.
- A backup is `stock: true` only if the device was **not** rooted when it was taken **and** the
  active boot matches a known `{ sha256, slot, displayId }` entry for its slot and the tablet's
  firmware, or else passes the magiskboot `cpio test` (push the image plus `magiskboot`, run
  `magiskboot unpack` and then `magiskboot cpio ramdisk.cpio test`; exit 0 means stock).

### 3.1b Import an existing backup
For people who already have a stock `boot` image, such as anyone who rooted with this repo's
scripts (including the owner).
1. "Choose your stock boot image…" (and optionally its sibling files). Validate the `ANDROID!`
   magic and the size (100,663,296 bytes for this profile).
2. **Stock check:** an imported image is `stock: true` **only** if its SHA-256 matches an entry in
   the profile's `known_stock_boot_sha256` list (`{ sha256, slot, displayId }`) whose `slot` is the
   slot the image is filed under (step 3) **and** whose `displayId` is the firmware it is filed
   under: the connected tablet's when one is connected, otherwise the entry's own. Evidence is
   `knownHash`, and the firmware recorded is the profile's entry for that `displayId`.
   - The magiskboot `cpio test` is **never** used for imports (nothing is pushed to the tablet). It
     is only for Inkroot's own backups (§3.1).
   - Any other image (unknown hash, or a known hash for another slot or firmware, including a
     Magisk-patched one) is still saved and verified, but for **Rescue only**: `stock: false`,
     evidence `unknownImport`. The flow ends normally and tells the user: "Inkroot doesn't
     recognise this file, so it can only use it for Rescue. To root, let Inkroot make its own
     safety copy from your tablet."
   - Files that fail the magic/size check are still rejected (`badImage`).
3. Ask which slot it belongs to: default to the one in the filename (`boot_b…`), otherwise the
   device's active slot. Copy it into the backup library with a manifest (`source: "imported"`,
   `stock` and evidence as decided in step 2, which runs once the slot is known).

### 3.2 Root
Preconditions: gates pass; verified stock backup exists for this serial (else run 3.1 first,
inline, as "Step 2 of 4 · Safety copy").
1. **Patch on device** (magiskboot is an Android arm64 binary): extract the bundled Magisk 30.7
   APK; push `libmagiskboot.so→magiskboot`, `libmagiskinit.so→magiskinit`,
   `libmagisk.so→magisk`, `libinit-ld.so→init-ld`, `assets/boot_patch.sh`,
   `assets/util_functions.sh`, `assets/stub.apk`, and `init.bootsvc.rc` to
   `/data/local/tmp/mgk/`; push the backed-up `boot_<slot>` as `boot.img` (never `stock_boot.img`: for a stock image Magisk's
   `boot_patch.sh` runs `cat $BOOTIMAGE > stock_boot.img` in its cwd to keep its own copy, which
   would truncate an input of that name to 0 bytes and fail the repack — found on hardware); seed
   `bootsvc.sh` → `/data/local/tmp/bootsvc.sh` and the default service → `/data/local/tmp/service.sh`.
2. Run, exactly:
   `cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld &&
   KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh boot.img &&
   ./magiskboot unpack new-boot.img &&
   ./magiskboot cpio ramdisk.cpio "add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc" &&
   ./magiskboot cpio ramdisk.cpio "ls overlay.d" | grep init.bootsvc &&
   ./magiskboot repack new-boot.img patched-boot.img`
   then pull `patched-boot.img` into the backup folder (keep it; record its hash in the manifest).
3. **Rescue-cable check** (before the first write): "If anything goes wrong, getting back
   needs a Qualcomm deep-flash cable and a computer with a USB-A port. Do you have one?"
   - **Yes, I have one:** continue.
   - **Show me where to get one:** what to search for, plus a picture.
   - **Continue without one:** a second, explicit confirmation is required.
   Remember the answer per machine so the check isn't repeated every time.
4. **Flash:** `adb reboot edl` → wait for 9008 → `edl --memory=ufs w boot_<slot> patched-boot.img`
   → read back and compare SHA-256 (as `edl-restore.sh`) → **only if hashes match**, `edl reset`.
   On mismatch: do not reset; tell the user calmly to re-seat the cable and retry the write.
5. **Verify after boot:**
   - Wait for adb to come back.
   - `adb shell /debug_ramdisk/magisk -c` must report 30.7.
   - `/data/local/tmp/bootsvc.log` must contain a fresh line with `uid=0` and `u:r:magisk:s0`.
   - If the device doesn't come back within about 3 minutes, don't panic the user. Show "Your
     tablet hasn't restarted yet" with two options: keep waiting, or **Go to Rescue**. Rescue
     opens with this device's stock backup and slot already selected.
6. **Done screen:** "Your Note Max is rooted." Show the Onyx **app-freeze** tip (turn off
   "Freeze new apps"; allow auto-start — `docs/03` §6) and a short "What can I do with root?"
   pointer to the boot-service docs.

Default `service.sh` shipped by the app: a minimal, harmless heartbeat logger (based on
`boot-service/example-service.sh`, without suspend/radio examples) so verification has
something to see. *(Builder's call on exact contents; it must do nothing risky.)*

**Advanced (tucked away, not on the main path):** "Install my own boot script" → pick a
`service.sh`, app pushes it to `/data/local/tmp/service.sh` and reboots (the hook re-seeds each
boot, per `boot-service/README.md`). Also "Run a one-off root command" via `oneshot.sh` is
**out of scope** for the UI (docs cover it).

### 3.3 Rescue (un-brick)
For a tablet that won't boot. No adb available, so no gates except the cable/EDL check.
1. **Pick the image:** the Inkroot backup library (grouped by serial/date) or "Choose a file…".
   Validate: `ANDROID!` magic, plausible size (~96 MB on this device). Pick the slot: default to
   the slot recorded in the backup manifest; for a loose file, ask with a plain explanation and
   default to the slot in the filename if it has one (`boot_b…` → `boot_b`).
2. **Cable guide** — illustrated, one instruction per panel, big type:
   - "You need a **Qualcomm deep-flash (EDL) cable** — a USB cable with a small button." Show a
     picture, what to search for ("Qualcomm deep flash cable" / "EDL cable"), and that a normal
     cable won't work. *Guide the user to get the correct one.*
   - "Plug its USB-A end into a **real USB-A port** on a computer. USB-C adapters often don't
     work." If the computer has no USB-A port: suggest borrowing one with USB-A (a desktop/old
     laptop) and running Inkroot there (Windows/Linux builds exist for exactly this).
   - Hold the cable button → plug into the tablet → hold tablet power ~15 s until fully blank →
     keep the cable button held ~10 more s → release (from `docs/02`).
   - Live status: "Waiting for your tablet…" → "Found it ✓" when `05c6:9008` appears. After
     60 s without it, show the troubleshooting panel (cable/port/adapter, try again, hold longer).
   - Reassurance copy: a black screen is normal; this is a soft brick.
3. **Write** → **verify by read-back hash** → `edl reset` (same rules as 3.2 step 3).
4. **Finish:** "Unplug the deep-flash cable, put the tablet on its normal charger, hold power
   ~5 s." Then optionally detect adb coming back and celebrate.

### 3.4 Return to stock (unroot)
Device boots with adb. Pick the stock backup for this serial (manifest `stock: true`, matching
firmware). `adb reboot edl` → write `boot_<slot>` → verify → reset → confirm `magisk -c` now
fails (no longer rooted).

### 3.5 Process & robustness rules (from `docs/02` gotchas)
- `adb` and `edl` run as **child processes in the foreground of the app's backend**, never
  detached; stdout/stderr streamed into the log.
- Every edl call has a timeout; on timeout kill the process tree (the doc notes a stuck edl
  pegs a core at 100 % and needs `pkill -9`).
- Only one device operation at a time.
- Every screen has an expandable **"Details"** log panel (mono, raw output) and a
  **"Copy log"** button for bug reports. The main view never shows raw output.

---

## 4. Platform specifics

- **macOS:** no drivers needed (libusb works unprivileged). Ship **two DMGs**:
  "Apple Silicon (M1 and newer)" and "Intel" — unless the builder gets a working universal2
  build of every sidecar, in which case one DMG. Signed with Developer ID, hardened runtime,
  **every bundled executable signed**, notarized and stapled. Secrets supplied by the owner in
  GitHub Actions (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`). The builder documents exactly
  which secrets to add in `docs/RELEASING.md`.
- **Windows:** unsigned NSIS or MSI installer (builder's choice). The app installs, with one UAC
  prompt and a clear explanation screen: the **WinUSB** driver for `05c6:9008` (via libwdi /
  `wdi-simple`, the same mechanism as Zadig) and, if needed, an **ADB** USB driver. The driver
  step appears only when it's needed. Builder confirms the BOOX's ADB USB IDs on hardware.
- **Linux:** AppImage. First EDL use offers to install the udev rule (from the edl project) with
  a single `pkexec` password prompt, then asks the user to re-plug.

---

## 5. Architecture

**App framework: Tauri 2** (Rust backend, web frontend). Chosen because the Panel UI is web
CSS (so the approved mockup carries over), it builds for all three OSes from one codebase, and
bundles are small. **Frontend: Svelte 5 + TypeScript + Vite** (builder may choose vanilla TS if
simpler; no heavyweight UI kit — the design system is custom).

```
inkroot/                        (repo root, renamed)
├─ app/                         Tauri app
│  ├─ src/                      Svelte frontend (screens, components, Panel design tokens)
│  ├─ src-tauri/
│  │  ├─ src/                   Rust: device detection, flows (state machines), adb/edl
│  │  │                         runners, hashing, backup library, platform helpers
│  │  ├─ profiles/note-max.json device profile
│  │  └─ resources/             tools/, magisk/, boot-service/, legal/ staged at build time (git-ignored)
├─ tools/stage-resources.mjs   downloads adb / builds edl / fetches Magisk 30.7 per target, verifies
│                               against tools/binaries.lock.json (URL + SHA-256), never commits them
├─ site/                        GitHub Pages website (static)
├─ assets/                      logo, hero, social preview, screenshots (shared by README + site)
├─ docs/  scripts/  boot-service/   unchanged in role (guides + CLI path for tinkerers)
└─ .github/workflows/           ci.yml (test/lint), release.yml (tag → 3-OS build → draft Release), pages.yml
```

- **Flows are explicit state machines in Rust**; the frontend renders the current state and
  sends user intents. Progress, status and log lines stream to the UI as events.
- **Tool runners are abstracted** behind traits so tests can substitute fakes. Paths to `adb`
  and `edl` resolve to bundled sidecars, overridable by env var for development.
- **Demo mode** (`INKROOT_DEMO=1` or a hidden toggle): a simulated device walks every flow
  without hardware. Used for development, automated tests, README/site screenshots.
- **Device profile** (`profiles/*.json`): display name; match rules (model/device strings,
  firmware build IDs); SoC; storage `ufs`; partition names (`boot_a/b`, `vbmeta_a/b`); expected
  boot size; edl loader selection (auto or explicit file); Magisk version + required patch env
  (`KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false`); whether to
  bake the boot-service; per-model cable-guide notes; `known_stock_boot_sha256` (entries of
  `{ sha256, slot, displayId }`, seeded with the trusted `boot_b` hash in §2). Adding a model
  means adding a profile, with its verification evidence in the PR.
- **edl** is bundled as a standalone executable built from a pinned `bkerler/edl` commit, for
  example with PyInstaller, so users don't need Python. Pin the commit and Loaders submodule
  recorded in §2. The Note Max profile names the loader that worked
  (`qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`) and passes it
  explicitly with `--loader=`, so a future change to edl's auto-selection can't break the device.
  Bundling only the loaders that profiles reference is enough. Including all of them is also fine.
- **Licensing:** app code MIT (existing LICENSE). Magisk and edl are GPLv3 and shipped as
  separate executables; include `THIRD_PARTY_NOTICES` with licenses + source links in the app
  and repo. Note in the README that adb (Google platform-tools) and Qualcomm loaders are
  third-party, redistributed as the edl project and others do. Device firmware is **never**
  bundled or uploaded.

---

## 6. Visual design — "Panel"

The owner picked this from three mockups: direction **B** in
`docs/superpowers/specs/assets/2026-10-25-look-directions.html`. Open it in a browser. Its
"B · Panel" card and its flow screens in theme B are the visual reference, and its CSS (the
`.B` rules and `flowwrap[data-t="B"]`, plus the `eink` keyframes) holds the exact values. The UI the BOOX would have "if it were honest about being 1-bit."

- **Palette:** panel `#E8E9E4` background, ink `#111111`, muted ink `#555`. No accent colour.
  Dark mode = inverted (ink background `#111`, panel-coloured text), following the system setting.
  The single exception for danger/irreversible warnings: still monochrome, signalled by a heavy
  border + icon + wording, not red.
- **Type:** **IBM Plex Sans** (headings 600, tight tracking; body 400) and **IBM Plex Mono**
  (labels, status, meta, logs; uppercase small labels with letter-spacing). Bundle fonts locally
  (OFL) — the app works offline.
- **Shapes:** hard 2 px ink borders, **no rounded corners**, no shadows inside the UI, no
  gradients. Primary action = solid ink block with panel-coloured text; secondary = outlined.
- **Iconography & art:** 1-bit **dithered** fills (checkerboard / ordered dither) inside
  masked silhouettes; illustrations (tablet, cable, button, ports) drawn as line + dither.
  The cable guide illustrations are the most important art in the app.
- **Wordmark:** `INKROOT` in Plex Mono 600 with the dithered square mark (as in the mockup).
- **Motion:** step changes use the **e-ink full-refresh flash** (brief invert → black → white →
  content, ~500 ms, stepped not eased) from the mockup; respect `prefers-reduced-motion` (plain
  cut). Progress bars are segmented/dithered, not smooth.
- **Layout:** fixed-feel window ~880×640 (resizable within limits). Header row: wordmark left,
  device status chip right (e.g. `● NOTE MAX / FW 4.2 / SLOT_B` in an ink block). Home: "Pick a
  job." with three cards — **Root** (primary), **Rescue**, **Stock** — plus a backup line
  (`backup ✓ boot_b.img · 96.0 MB · sha 3f9a…c21`). Flows: "STEP 2 OF 4 · SAFETY COPY" label,
  one heading, one or two sentences, one primary button.
- **Voice:** plain, warm, short. Second person. No jargon on the main path ("safety copy of your
  tablet's startup software", with the technical term in the Details panel). Never blame the
  user. Reassure at scary moments ("The screen may stay blank. That's normal. Don't unplug it.").

The README, website and social preview use the same system so it all reads as one product.

---

## 7. GitHub page, website, SEO

- **Repo metadata** (owner applies or approves `gh` commands):
  - Name `inkroot`.
  - Description: *"Inkroot — one-click root (Magisk) & un-brick (Qualcomm EDL) for the ONYX BOOX
    Note Max e-ink tablet. Mac, Windows, Linux."*
  - Topics: `boox`, `onyx-boox`, `boox-note-max`, `eink`, `e-ink`, `root`, `magisk`, `edl`,
    `qualcomm-edl`, `unbrick`, `bootloop`, `android-13`, `snapdragon-855`, `tauri`.
  - Homepage: the Pages URL. Social preview image (1280×640, Panel style).
- **README** (rewritten): Panel-style hero image (light/dark via `<picture>`), one-line pitch,
  **Download** buttons per OS linking to the latest Release, a 3-step "how it works", an
  "**Is your BOOX stuck on the ONYX logo?**" callout linking Rescue, screenshots from demo
  mode, a safety section (what can go wrong, backup first, verity), FAQ (Will it void my
  warranty? Which firmware? Which cable? Why can't the Magisk app grant su? Is it safe?), "For
  tinkerers" section linking the existing guides/scripts/boot-service, credits (Magisk, edl),
  third-party notices. Keep existing technical content in `docs/` — do not delete it.
- **Website** (`site/`, static HTML/CSS, no framework, deployed by `pages.yml` to
  `https://n0dal.github.io/inkroot/`): landing page in the Panel style (hero, download,
  how it works, rescue callout, FAQ); a dedicated **rescue page** targeting "BOOX stuck on
  logo / bootloop / unbrick"; SEO basics — descriptive `<title>`s and meta descriptions,
  Open Graph/Twitter cards, JSON-LD `SoftwareApplication` and `FAQPage`, `sitemap.xml`,
  `robots.txt`, canonical URLs, fast and accessible. Download buttons resolve to the latest
  release assets.
- **Target phrases** (from research): boox root, root boox note max, boox magisk, boox edl,
  unbrick boox, boox stuck on logo, boox bootloop, boox firmware 4.2 root, boox fastboot removed.
  Use them naturally in headings/copy; no keyword stuffing.
- **Docs fixes:**
  - Update the Magisk examples from `30.1`/`30.x` to `30.7` (`Magisk-v30.7.apk`).
  - In `docs/02`, add that EDL over a normal USB-C cable from a Mac works when the tablet is
    entered with `adb reboot edl`. The "native USB-A" rule applies to the deep-flash cable.
  - In `docs/02`, record the edl commit and loader that worked.
  - Point every guide at the app as the easy path.
  - Otherwise, none of the guides' technical claims change.

---

## 7b. Legal protection (owner request, 2026-10-25)

The owner doesn't want to be liable if someone bricks their device. This is not legal advice.
It's the standard layered protection used by projects like Magisk, checkra1n and LineageOS.
Tell the owner that a lawyer is the only route to real certainty.

- **License:** keep **MIT**. Its all-caps "AS IS, WITHOUT WARRANTY … IN NO EVENT SHALL THE
  AUTHORS … BE LIABLE" clause is the core shield. The copyright line stays "the contributors"
  (or the owner's handle). Don't use the owner's legal name unless they ask for it.
- **`DISCLAIMER.md`** at the repo root, in plain language:
  - Modifying your device's software can permanently damage it, void your warranty, cause
    data loss, or make it unusable.
  - You do this entirely at your own risk.
  - The software is provided as-is with no warranty, and the authors and contributors accept
    no liability for any damage.
  - Inkroot is not affiliated with, endorsed by, or sponsored by Onyx International / BOOX,
    Qualcomm, Google or the Magisk project. Product names are used only to describe
    compatibility, and the trademarks belong to their owners.
  - Rooting may be restricted by law or contract where you live. Checking is your
    responsibility.
  - Not for use on devices you don't own.
  - Link it from the README, site footer, release notes and the app's About screen.
- **In-app acknowledgement:**
  - Shown before the first Root, Unroot or Rescue, and again whenever `RISK_NOTICE_VERSION`
    changes. It's a short, plain screen: risk of bricking, warranty, no liability, not
    affiliated, plus a link to the full disclaimer.
  - It has an unticked checkbox ("I understand and accept these risks; I'm responsible for my
    device") and a Continue button that stays disabled until the box is ticked.
  - Back up and Import never write to the device, so they don't need it.
  - Store the accepted version locally (contract: `acceptedRiskVersion`). This gives the
    project an explicit, recorded user acceptance.
- **Trademark hygiene:**
  - Don't use the BOOX or Onyx logos or trade dress anywhere.
  - "BOOX" appears only descriptively ("for the ONYX BOOX Note Max").
  - The Inkroot wordmark and mark are original.
  - The tablet illustrations are generic line art, not traced product photos.
- **Third-party licence compliance:** this avoids copyright claims.
  - `THIRD_PARTY_NOTICES` lists every bundled component: Magisk (GPLv3), edl (GPLv3), adb
    (Android SDK platform-tools terms), Qualcomm loaders (proprietary, as distributed by the
    edl project) and IBM Plex (OFL).
  - Each GPL component gets an exact source link pinned to its version or commit, which meets
    the GPL source-offer requirement.
  - Never bundle or host device firmware.
- **Privacy:** the app collects and transmits nothing, with no analytics and no telemetry. Say
  so in the README and site, which also builds trust.
- **Rooting legality note** (README FAQ, worded carefully): in the US, the Copyright Office's
  DMCA §1201 exemptions have allowed jailbreaking smartphones and general-purpose portable
  computing devices since 2015, renewed most recently in 2024. Other countries differ, and
  readers should check their local law. No guarantees.
- **Releases and issue templates:**
  - Release notes repeat the one-line disclaimer.
  - The bug-report issue template asks for the log and includes a "this is community software
    provided as-is" note.

---

## 8. Testing & verification

- **Automated (CI, every push):** Rust unit tests for flow state machines with fake adb/edl
  runners covering happy paths and every failure (no device, unauthorized, wrong firmware, low
  battery, already rooted, EDL timeout, hash mismatch, patch failure, verify failure); manifest
  and backup-library tests; frontend type-check/lint; a demo-mode smoke test of every flow.
- **Release build check:** CI produces all OS artifacts; macOS artifact passes
  `spctl --assess` / `stapler validate`.
- **Hardware pass (owner, before first public release)** on the real Note Max. The tablet is
  currently rooted, so the order matters:
  1. Detection and gates show "Already rooted ✓".
  2. **Import** `boot_b.stock.img`. Its hash matches the known-stock list.
  3. **Return to stock** from macOS. Confirm `magisk -c` now fails.
  4. **Back up** from macOS. The new backup must be `stock: true` and its `boot_b` hash must
     equal `2f05dc1b…`.
  5. **Root** end-to-end from macOS. Verify Magisk 30.7 and the bootsvc log.
  6. **Rescue** using the deep-flash cable from a USB-A host (Linux AppImage and/or Windows).
  7. Windows driver install on a real Windows machine.
  8. Optional: Reinstall boot service (hook-present path).
  Builders must **not** claim device behaviour is verified until this pass happens; release
  notes for pre-pass builds say "beta — not yet hardware-verified on <OS>".

---

## 9. Left to the builder / future ideas

**Builder's call:** Svelte vs vanilla TS; installer format on Windows; exact default
`service.sh`; PyInstaller vs another freezer for edl; universal vs split macOS builds; illustration
technique; site build tooling (keep it minimal); exact copy (following §6 voice); log retention.

**Future (not v1):** rescue **without a backup** — read the bricked patched boot over EDL and
restore its original ramdisk with `magiskboot cpio … restore` (how Magisk's own "restore images"
works), or switch to the other slot with `edl setactiveslot`; more device profiles (Tab Ultra C
Pro is also SDM855 per MobileRead); WebUSB browser version; in-app updates; localisation.
