# Developing Inkroot

Everything — the app, the website, the release pipeline and the manual guides — lives in this one
repository. This page is the map.

## What's where

| Path | What it is |
|---|---|
| `app/` | The Inkroot desktop app (Tauri 2). |
| `app/src/` | The UI: Svelte 5 + TypeScript, the "Panel" design (IBM Plex, 1-bit dithering). |
| `app/src/lib/mock/` | A fake backend with scripted scenarios, so the UI runs in a plain browser. |
| `app/src-tauri/` | The engine: Rust. Device detection, the flows (back up, root, rescue, return to stock, import), adb/edl runners, backup library. |
| `app/src-tauri/profiles/note-max.json` | The supported device: firmware match, partitions, EDL loader, Magisk version, known stock hashes. |
| `boot-service/` | The init hook + scripts Inkroot bakes into the patched boot so a script runs as root at every boot. Also usable by hand. |
| `tools/` | Node + Python build tooling: binary lock file and staging, edl/wdi-simple builders, macOS signing, brand art, website build, release notes. |
| `site/` | The website (static HTML/CSS), deployed to <https://n0dal.github.io/inkroot/>. |
| `assets/` | Logo, icon, README hero, social preview, screenshots (shared by README and site). |
| `docs/01–03` | The manual rooting / un-brick guides the app automates. |
| `docs/RELEASING.md` | How to cut a signed release, the Apple secrets, the hardware test checklist. |
| `docs/superpowers/` | The original design spec, IPC contract, build plans and every decision made during the build. |
| `scripts/` | Small manual helpers (`patch-boot.sh`, `edl-restore.sh`). |
| `.github/workflows/` | `ci` (tests on macOS/Windows/Linux), `edl-build` (edl self-test on all OSes), `release` (signed builds on a `v*` tag), `pages` (website). |

The engine and the UI talk through one contract: `docs/superpowers/specs/2026-10-25-inkroot-ipc-contract.md`.
Change it, the Rust types (`app/src-tauri/src/ipc.rs`) and the TypeScript types (`app/src/lib/ipc.ts`) together.

## Setup

- Node 22+, Rust (stable, via rustup), Python 3.12 (only to build edl).
- `npm ci --prefix tools` and `npm ci --prefix app`.
- Third-party binaries (adb, edl, the Qualcomm loader, Magisk) are never committed. `npm run stage --prefix app`
  downloads, hash-checks and stages them into `app/src-tauri/resources/` from `tools/binaries.lock.json`.
  edl must be built first for your platform: `python -I tools/build_edl.py --target darwin-aarch64`
  (or `darwin-x86_64`, `linux-x86_64`, `windows-x86_64`).

## Everyday commands

| Do this | Run |
|---|---|
| UI in a browser with a fake tablet | `npm run dev:mock --prefix app` (pick scenarios with `?mock=<name>`) |
| Desktop app with a simulated tablet | `INKROOT_DEMO=1 npm run tauri dev --prefix app` |
| Desktop app against a real tablet | `npm run stage --prefix app`, then `npm run tauri dev --prefix app` |
| UI checks + tests | `npm run check --prefix app` · `npm test --prefix app` |
| Engine checks + tests | `npm run check:rust --prefix app` (fmt, clippy `-D warnings`, cargo test) |
| Tooling tests | `npm test --prefix tools` |
| Rebuild brand art / social preview | `npm run brand --prefix tools` |
| Retake README/site screenshots | `npm run screenshots --prefix app` (from the mock UI) |
| Build + check the website | `npm run site --prefix tools` · `npm run check-site --prefix tools` |

`tauri dev` sometimes rewrites `features = []` into `app/src-tauri/Cargo.toml` — don't commit that.

## Releasing

Bump the version in `app/package.json`, `app/src-tauri/tauri.conf.json` and `app/src-tauri/Cargo.toml`
(plus the `inkroot` entry in `Cargo.lock`), push, then tag `vX.Y.Z`. The release workflow builds all four
downloads, signs and notarizes the Macs, and leaves a **draft** release to review and publish.
Full details, secrets and the checklist: [RELEASING.md](RELEASING.md).

## Testing on real hardware

Automated tests use a simulated tablet; anything that touches EDL must also be checked on a real Note Max.
What we learned doing that:

- **macOS:** works over a normal USB-C cable (`adb reboot edl`). No driver needed.
- **Linux:** the first EDL use asks for a password once, to install a USB permission rule.
- **Windows:** the first EDL use installs Windows' own WinUSB driver for the tablet's rescue mode (one prompt).
- **The deep-flash cable** (for a tablet that won't boot) needs a real USB-A port. Don't touch its button
  during a transfer — it cuts the connection.
- **Virtual machines:** passing a single USB *port* through to a VM breaks EDL (the handshake stalls).
  Pass the whole USB *controller* (PCI passthrough) instead — that's how Windows and Linux were verified.
- **E-ink** keeps its last picture when the tablet is off or in EDL mode; a "frozen" screen is normal.
- A tablet stuck in EDL mode with nothing answering: hold power for up to 30 seconds.

## Ground rules

- Never write to a tablet without a verified stock backup; never restart after a write that didn't verify.
- `KEEPVERITY=true KEEPFORCEENCRYPT=true` is non-negotiable (patching without it bricks this device).
- Only support firmware that's in the device profile; unknown firmware is blocked.
- Plain commit messages; no co-author or generated-by trailers.
