# Inkroot Release & Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship everything around the Inkroot app: a hash-pinned binary supply chain that stages adb/edl/loader/Magisk/boot-service into the Tauri bundle, CI and a signed and notarized 3-OS release pipeline, legal files, brand assets, the rewritten README, the GitHub Pages site and the docs fixes.

**Architecture:** One small Node workspace in `tools/` holds the lock file, the staging script, brand rendering, release-notes rendering and the site checks, all tested with `node:test`. CI jobs build edl (PyInstaller onedir) and wdi-simple per target as artifacts. The Tauri build's `beforeBuildCommand` stages verified binaries into `app/src-tauri/resources/`, and signs every nested Mach-O on macOS. Docs, legal files, the README and the site are plain Markdown/HTML that share one copy deck (this plan) and one visual system (Panel).

**Tech Stack:** Node 22 (ESM, `node:test`, `fflate`), Python 3.12 + `uv` + PyInstaller 6.16.0, MSYS2/MinGW (libwdi), GitHub Actions (`tauri-apps/tauri-action@v0`, `actions/deploy-pages@v4`), Playwright (Chromium) for rendering brand PNGs, `opentype.js` for outlining the wordmark, `html-validate` + `lychee` for the site.

**Spec:** `docs/superpowers/specs/2026-10-25-inkroot-design.md` (+ mockup `docs/superpowers/specs/assets/2026-10-25-look-directions.html`, direction B "Panel"; IPC contract `docs/superpowers/specs/2026-10-25-inkroot-ipc-contract.md`).

## Global Constraints

- Repo is `n0dal/inkroot`. Every link targets `https://github.com/n0dal/inkroot` and site `https://n0dal.github.io/inkroot/`.
- Magisk is pinned to `Magisk-v30.7.apk`, URL `https://github.com/topjohnwu/Magisk/releases/download/v30.7/Magisk-v30.7.apk`, SHA-256 `e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5`. Never `app-debug.apk`.
- edl is pinned to `bkerler/edl` commit `2f8e89a848afaaef68997fcbcb5b178d958d497b` (client 3.62), with Loaders submodule `bf0d8017eb97464530114a348c5157a6ea6a3372`. Loader: `qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin` (682,616 bytes, SHA-256 `7d85aafd5fc0872c12e61a0a480ca4c1042095a129278225de893b23444605ea`, verified 2026-10-25).
- Google platform-tools is pinned to **r37.0.1** (verified 2026-10-25 against Google's repository2-3.xml SHA-1s):
  - darwin: `https://dl.google.com/android/repository/platform-tools_r37.0.1-darwin.zip`, SHA-256 `ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d`. The adb inside is universal (x86_64 + arm64).
  - linux: `…platform-tools_r37.0.1-linux.zip`, SHA-256 `d230f13842f60f782a8645f9c813f8f845bf36089ea7289f28c48f17979313f1`.
  - windows: `…platform-tools_r37.0.1-win.zip`, SHA-256 `45f4d63113e895ebde0c90f194099a4676b6ac653bd28d54314a9e022bbc1a99`.
- libwdi is pinned to tag `v1.5.1` = commit `9b23b82a2dd1cbffc16d46c212f92c6bf8c0c602`.
- **Staged layout contract (shared with the app-core plan; names are fixed):** under `app/src-tauri/resources/`:
  - `tools/adb` (Windows: `tools/adb.exe`, `tools/AdbWinApi.dll`, `tools/AdbWinUsbApi.dll`)
  - `tools/edl/edl` (Windows: `tools/edl/edl.exe`), plus its onedir `_internal/`
  - `tools/loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`
  - `tools/wdi-simple.exe` (Windows only)
  - `magisk/Magisk-v30.7.apk`
  - `boot-service/init.bootsvc.rc`, `boot-service/bootsvc.sh`, `boot-service/inkroot-default-service.sh`
- Nothing binary is committed. `app/src-tauri/resources/`, `tools/.build/`, `tools/.cache/` and `tools/node_modules/` are git-ignored. The existing `.gitignore` rules (`*.img *.apk *.bin …`) stay.
- Target IDs used everywhere in this plan: `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64`, `linux-x86_64`.
- Panel visual system: panel `#E8E9E4`, ink `#111111`, muted `#555`. IBM Plex Sans (headings 600, tight tracking; body 400) and IBM Plex Mono (labels uppercase, letter-spacing .12–.14em). Hard 2px ink borders, `border-radius:0`, no shadows, no gradients other than dithers. 1-bit dithered fills. Inverted ink-block chips. Dark mode = inverted. No accent colour, no red for danger.
- Trademark: no BOOX/Onyx logos or trade dress. "BOOX" only descriptively ("for the ONYX BOOX Note Max"). Footer line, verbatim: `Inkroot is not affiliated with, endorsed by, or sponsored by Onyx International / BOOX, Qualcomm, Google or the Magisk project.`
- License stays MIT (`LICENSE`, "Copyright (c) 2026 the contributors"). Never use the owner's legal name.
- Privacy statement, verbatim wherever privacy is mentioned: `Inkroot collects and sends nothing — no analytics, no telemetry, no account. Your safety copies stay on your computer and are never uploaded.`
- Device firmware is never bundled, hosted or uploaded.
- Voice: plain, warm, short, second person. No jargon on the main path. Never blame the user.
- Do not claim device behaviour is hardware-verified. Until the owner's pass, release notes say `beta — not yet hardware-verified on <OS>`.
- SEO target phrases, used naturally and never stuffed: boox root, root boox note max, boox magisk, boox edl, unbrick boox, boox stuck on logo, boox bootloop, boox firmware 4.2 root, boox fastboot removed, plus "turn on USB debugging on a BOOX".

## Review Focus

1. **A corrupt, truncated or swapped download** (bad mirror, a cached file changed on disk). Staging must abort with the file name, the expected hash and the actual hash. It must leave no partial file in `resources/` and must never trust a cache entry without re-hashing it. *Test: Task 6 `stage aborts on hash mismatch and leaves resources untouched`, `corrupt cache entry is re-downloaded`.*
2. **Building for a target that isn't the host, or a missing edl artifact for that target.** Staging uses `TAURI_ENV_PLATFORM`/`TAURI_ENV_ARCH` over the host. A missing `tools/.build/<target>/edl` is a hard error that names the build command. Staging never falls back to another arch's edl. *Test: Task 6 `missing edl build for target fails with build hint`, `env target overrides host`.*
3. **Placeholder resources leaking into a release** (CI uses placeholders so `cargo clippy` can run). `--check` fails if any staged file is a placeholder, and release.yml runs it. *Test: Task 6 `check rejects placeholders`.*
4. **One unsigned or non-hardened nested Mach-O** (a dylib in edl's `_internal/`, Google's adb). Notarization rejects the whole app. Every Mach-O must be signed with our Team ID, the runtime flag and a timestamp, and this is checked before bundling and again after with `spctl`/`stapler`. *Test: Task 8 `findMachO finds thin, fat and dylib files`. Release job "Verify nested signatures" step.*
5. **Site download buttons when the GitHub API is rate-limited, no release exists yet, or an asset is missing.** The buttons must still lead somewhere useful, and the static `/releases/latest/download/<stable-name>` links must be correct. *Test: Task 5 `pickAssets` and `downloadLinks` cases.*

Also exercised: staged executables keep mode 0755 on POSIX (AppImage loses exec bits otherwise; Task 6), and tag/version mismatch fails the release (Task 10).

---

## Task dependency map

| Task | Depends on | Can run in parallel with the app plans? |
|---|---|---|
| 1 Legal files | — | yes |
| 2 Docs fixes | — | yes |
| 3 tools/ workspace + brand assets | — | yes |
| 4 README rewrite | 1, 3 | yes |
| 5 Website + pages.yml | 1, 3 | yes |
| 6 Lock file + stage-resources | 3 (tools/package.json) | yes (no app needed; uses fixtures) |
| 7 edl + wdi-simple builders | 6 | yes |
| 8 macOS signer + release-notes renderer | 6 | yes |
| 9 App wiring + ci.yml + Tauri icon set | 6; **app-core plan** (`app/`, `tauri.conf.json`, `app/package.json` scripts `build`, `check`, `test`) and `boot-service/inkroot-default-service.sh` | no, after app-core scaffolding |
| 10 release.yml + docs/RELEASING.md | 7, 8, 9 | no |

---

### Task 1: Legal files and issue templates

**Files:**
- Create: `DISCLAIMER.md`, `THIRD_PARTY_NOTICES.md`, `SECURITY.md`, `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/config.yml`
- Keep unchanged: `LICENSE`

**Interfaces:**
- Produces: `DISCLAIMER.md` and `THIRD_PARTY_NOTICES.md` at the repo root. They are linked by the README (Task 4), the site footer (Task 5) and the release notes (Task 8), and staged into the app (Task 6 `legal/`). The one-line disclaimer string `ONE_LINE_DISCLAIMER` (below) is reused verbatim by Tasks 4, 5 and 8.

`ONE_LINE_DISCLAIMER` = `Use at your own risk: modifying your tablet's software can brick it or void its warranty. Inkroot is provided as-is, with no warranty, and is not affiliated with Onyx/BOOX. Read the full [disclaimer](https://github.com/n0dal/inkroot/blob/main/DISCLAIMER.md).`

- [ ] **Step 1: Write `DISCLAIMER.md`, exactly:**

```markdown
# Disclaimer

**Please read this before you use Inkroot.**

- **You can damage your device.** Modifying your tablet's software — rooting it, writing to its
  startup ("boot") partition, or using Qualcomm's emergency download mode — can permanently damage
  it, void your warranty, cause data loss, or make it unusable.
- **You do this entirely at your own risk.** You are responsible for your device and for what you
  do with it.
- **No warranty, no liability.** Inkroot is provided "as is", without warranty of any kind. The
  authors and contributors accept no liability for any damage, data loss or other harm arising from
  its use, to the fullest extent the law allows. See also the [MIT License](LICENSE).
- **Not affiliated.** Inkroot is not affiliated with, endorsed by, or sponsored by Onyx
  International / BOOX, Qualcomm, Google or the Magisk project. Product names such as "BOOX" and
  "Note Max" are used only to describe compatibility; all trademarks belong to their owners.
- **Check your local law.** Rooting may be restricted by law or by contract where you live.
  Checking that is your responsibility.
- **Only your own device.** Do not use Inkroot on a device you don't own.

Inkroot collects and sends nothing — no analytics, no telemetry, no account. Your safety copies
stay on your computer and are never uploaded.

*This is not legal advice.*
```

- [ ] **Step 2: Write `THIRD_PARTY_NOTICES.md`** with one section per bundled component. Each section has the name, version/commit, licence, the upstream source URL pinned to that version, and a one-line note of how it's shipped. Exact entries:
  - **Magisk 30.7**: GPL-3.0. Source: `https://github.com/topjohnwu/Magisk/tree/v30.7`. Shipped unmodified as `magisk/Magisk-v30.7.apk`. Binaries from it run only on the tablet.
  - **edl (bkerler/edl) 3.62**: GPL-3.0. Source: `https://github.com/bkerler/edl/tree/2f8e89a848afaaef68997fcbcb5b178d958d497b`. Shipped as a separate standalone executable built with PyInstaller by `tools/build_edl.py` from that exact commit. Also list its bundled Python dependencies and their licences: pyusb (BSD-3), pyserial (BSD-3), lxml (BSD-3), pycryptodome (BSD/Public Domain), colorama (BSD-3), docopt (MIT), pylzma (LGPL-2.1+), Exscript (MIT), paramiko (LGPL-2.1), requests (Apache-2.0), passlib (BSD), libusb-package / libusb-1.0 (LGPL-2.1+). The implementer confirms this list from `uv pip list` in the build venv and corrects it if different.
  - **Qualcomm firehose loader** `000a50e100000000_d40eee56f3194665_fhprg.bin`: proprietary Qualcomm/OEM code, redistributed as the edl project distributes it, from `https://github.com/bkerler/Loaders/tree/bf0d8017eb97464530114a348c5157a6ea6a3372`. No licence is granted by Inkroot.
  - **Android SDK Platform-Tools r37.0.1 (adb)**: Google, Android Software Development Kit License / Apache-2.0 components (see `NOTICE.txt` in the platform-tools zip, staged as `legal/platform-tools-NOTICE.txt`). Source: `https://android.googlesource.com/platform/packages/modules/adb/`.
  - **libwdi / wdi-simple v1.5.1** (Windows only): LGPL-3.0. Source: `https://github.com/pbatard/libwdi/tree/v1.5.1`. Built unmodified by `tools/build-wdi.sh`.
  - **IBM Plex Sans and IBM Plex Mono**: SIL Open Font License 1.1. Source: `https://github.com/IBM/plex`.
  - A closing **"Written offer / source"** paragraph: "For each GPL/LGPL component above, the complete corresponding source is available at the pinned link, and the build scripts in this repository (`tools/`) reproduce the binaries we ship. Device firmware is never bundled."
- [ ] **Step 3: Write `SECURITY.md`.** Report vulnerabilities via GitHub private vulnerability reporting (`https://github.com/n0dal/inkroot/security/advisories/new`), not in public issues. Scope: the app, the build/release pipeline and the site. Out of scope: Magisk, edl and Onyx firmware (report those upstream).
- [ ] **Step 4: Write `.github/ISSUE_TEMPLATE/bug_report.yml`.** `name: Bug report`, `labels: [bug]`. In order:
  - a markdown block: "Inkroot is community software provided as-is, with no warranty. If your tablet won't start, first read the [rescue guide](https://n0dal.github.io/inkroot/rescue/)."
  - inputs: `version` (Inkroot version, required); `os` dropdown (`macOS — Apple Silicon`, `macOS — Intel`, `Windows 10/11`, `Linux`, required); `os_version` (input, required); `device` (input, default `BOOX Note Max`, required); `firmware` (input, "Settings → About: firmware version, e.g. 4.2 / TKQ1.230721.002", required); `job` dropdown (`Back up`, `Import a backup`, `Root`, `Rescue`, `Return to stock`, `Other`); `what_happened` (textarea, required); `log` (textarea, `render: text`, required, description "In Inkroot open **Details → Copy log** and paste it here. You may remove your tablet's serial number."); `asis` checkbox, required: "I understand Inkroot is provided as-is, with no warranty, and that I use it at my own risk."
- [ ] **Step 5: Write `.github/ISSUE_TEMPLATE/config.yml`.** `blank_issues_enabled: false`, with `contact_links`: "My tablet is stuck on the ONYX logo" → `https://n0dal.github.io/inkroot/rescue/`, and "Manual guides (adb / EDL)" → `https://github.com/n0dal/inkroot/tree/main/docs`.
- [ ] **Step 6: Verify the YAML parses.**
  Run: `npx --yes yaml-lint .github/ISSUE_TEMPLATE/*.yml`
  Expected: `✔ YAML Lint successful.` for both files.
- [ ] **Step 7: Commit.** `git add DISCLAIMER.md THIRD_PARTY_NOTICES.md SECURITY.md .github/ISSUE_TEMPLATE && git commit -m "docs: add disclaimer, third-party notices, security policy and issue templates"`

---

### Task 2: Docs fixes (Magisk 30.7, USB debugging, EDL facts, app pointer)

**Files:**
- Modify: `docs/01-rooting.md` (lines 3, 21–22, 26–27, 37, 44), `docs/02-unbrick-edl.md` (top, cable IMPORTANT block, "Install the EDL tool", "How to get into EDL"), `docs/03-notes-and-gotchas.md` (top, §7 first bullet), `boot-service/README.md` (top, line 19), `boot-service/build-and-flash.sh:4`

**Interfaces:**
- Produces: `USB_DEBUG_BLOCK` (below). It is the single canonical USB-debugging copy, reused verbatim by Tasks 4 and 5 and consistent with spec §3.0 gate 1.

`USB_DEBUG_BLOCK`:

```markdown
**Turn on USB debugging**

*Method A — Onyx menu (try this first):*
1. Open **Settings → Apps**.
2. Tap the **⋮** menu (top-right) → **USB Debug Mode**, and turn it on.

*Method B — if you can't find it:*
1. Install **Activity Launcher** (free and open source) from F-Droid or Google Play. On a BOOX,
   Google Play may first need to be turned on under **Settings → Apps**.
2. In Activity Launcher, open Android **Settings** (`com.android.settings`).
3. Tap **About tablet**, then tap **Build number** 7 times.
4. Go back to **System → Developer options** and turn on **USB debugging**.

Then plug the tablet into your computer, tap **Allow** on "Allow USB debugging?", and tick
**Always allow from this computer**.

> Method A comes from field notes and hasn't been confirmed on firmware 4.2 hardware yet. If it
> isn't there on your tablet, Method B works on any Android 13 build.
```

`APP_POINTER` (first line under each guide's H1):
`> **Easiest: use the [Inkroot app](https://github.com/n0dal/inkroot#download).** It does every step in this guide for you, with checks at each step. This guide is the manual path for tinkerers.`

- [ ] **Step 1: Write the failing check.** Run `grep -rnE "30\.1|30\.x|tap build" README.md docs/0*.md boot-service scripts`. Expected now: 8 hits (README:37,44; docs/01:3,21,37,44; boot-service/README.md:19; build-and-flash.sh:4). After this task only README hits remain, and Task 4 removes those.
- [ ] **Step 2: Edit `docs/01-rooting.md`:**
  - Insert `APP_POINTER` under the H1.
  - Line 3: `Magisk **30.x**` → `Magisk **30.7**`.
  - Replace the USB-debugging bullet (lines 21–22) with `- The Note Max with **USB debugging** on:` followed by `USB_DEBUG_BLOCK`, indented as a nested block.
  - The Magisk bullet: link `https://github.com/topjohnwu/Magisk/releases/tag/v30.7` and name the file `Magisk-v30.7.apk` (SHA-256 `e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5`). Say "not `app-debug.apk`".
  - Lines 37 and 44: `Magisk-v30.x.apk` → `Magisk-v30.7.apk`.
- [ ] **Step 3: Edit `docs/02-unbrick-edl.md`:**
  - Insert `APP_POINTER` under the H1.
  - After the cable `[!IMPORTANT]` block add a `> [!NOTE]`: "**Backing up or flashing a tablet that still boots doesn't need the deep-flash cable or USB-A.** Enter EDL with `adb reboot edl` and a normal USB-C cable works, including straight into a Mac (verified: our stock backup was read this way from macOS). The native-USB-A rule above is for the **deep-flash cable** method."
  - Under "Install the EDL tool" add "**Tested with:** `bkerler/edl` commit `2f8e89a848afaaef68997fcbcb5b178d958d497b` (client 3.62), Loaders submodule `bf0d8017eb97464530114a348c5157a6ea6a3372`. With no loader given it detected `SM8150`, HWID `0x000a50e100000000`, and picked `Loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`. To use exactly that: `git checkout 2f8e89a8 && git submodule update`, and pass `--loader=Loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`."
  - Change nothing else.
- [ ] **Step 4: Edit `docs/03-notes-and-gotchas.md`.** Insert `APP_POINTER` under the H1. Replace §7's first bullet with `- **USB debugging:** Onyx hides Android's usual "tap Build number" route. See the two methods in [01 · What you need](01-rooting.md#what-you-need).`
- [ ] **Step 5: Edit `boot-service/README.md`** (insert `APP_POINTER` under the H1; line 19 `Magisk-v30.x.apk` → `Magisk-v30.7.apk`) and `boot-service/build-and-flash.sh:4` (`Magisk-v30.1.apk` → `Magisk-v30.7.apk`).
- [ ] **Step 6: Verify.**
  Run: `grep -rnE "30\.1|30\.x|tap build" docs/0*.md boot-service scripts; grep -c "Easiest: use the" docs/0*.md boot-service/README.md; bash -n boot-service/build-and-flash.sh && echo SH_OK`
  Expected: no output from the first grep; `1` for each of the 4 files; `SH_OK`.
  Run: `git diff --stat -- docs boot-service`
  Expected: only the 5 files above changed, mostly insertions.
- [ ] **Step 7: Commit.** `git commit -am "docs: pin Magisk 30.7, fix USB-debugging steps, record edl commit/loader, point guides at the app"`

---

### Task 3: tools/ workspace and brand assets

**Files:**
- Create: `tools/package.json`, `tools/package-lock.json`, `tools/brand/mark.mjs`, `tools/brand/wordmark.mjs`, `tools/brand/render.mjs`, `tools/brand/templates/{hero.html,social.html,panel.css}`, `tools/test/brand.test.mjs`
- Create (generated, committed): `assets/brand/inkroot-mark.svg`, `assets/brand/inkroot-wordmark.svg`, `assets/brand/inkroot-wordmark-dark.svg`, `assets/icon/inkroot-icon-1024.svg`, `assets/icon/inkroot-icon-1024.png`, `assets/readme/hero-light.png`, `assets/readme/hero-dark.png`, `assets/social/social-preview.png` (1280×640), placeholder `assets/screenshots/{home,root-step,rescue-cable,done,home-dark}.png`
- Modify: `.gitignore` (append `tools/node_modules/`, `tools/.cache/`, `tools/.build/`, `app/src-tauri/resources/`, `_site/`, `!assets/**/*.png`)

**Interfaces:**
- Produces:
  - `tools/package.json`: `{"name":"inkroot-tools","private":true,"type":"module"}`. deps `fflate@0.8.2`. devDeps `opentype.js@1.3.4`, `playwright@^1.48`, `html-validate@^9`, `@ibm/plex-sans@^1`, `@ibm/plex-mono@^1`. The lockfile is committed. Scripts: `"test":"node --test test/"`, `"brand":"node brand/render.mjs all"`, `"site":"node site/build.mjs"`, `"check-site":"node site/check.mjs"`.
  - `markSvg({ size: number, border: number, cell: number, ink?: string, panel?: string }): string` in `tools/brand/mark.mjs`. It draws a square with a hard ink border and a 1-bit ordered-dither (Bayer 4×4) fill whose density ramps from about 75% top-left to about 25% bottom-right. Only `<rect>` elements, only the two colours, no `rx`.
  - `wordmarkSvg({ theme: "light"|"dark", height: number }): Promise<string>` in `tools/brand/wordmark.mjs`. It returns the mark plus `INKROOT` in IBM Plex Mono SemiBold, letter-spacing .02em, text converted to `<path>` with opentype.js from `node_modules/@ibm/plex-mono/fonts/complete/ttf/IBMPlexMono-SemiBold.ttf` (check the path with `ls`), so it renders without fonts on GitHub.
  - `render.mjs <target>` with targets `mark | wordmark | icon | hero | social | placeholders | all`. Playwright renders PNGs from SVG or HTML at exact pixel sizes. `placeholders` never overwrites an existing screenshot unless `--force`.
  - `tools/brand/templates/panel.css`: the shared Panel tokens (`--panel`, `--ink`, `--muted`, `@font-face` for Plex from `node_modules`), reused by the site in Task 5.

- [ ] **Step 1: Scaffold `tools/package.json`** with the deps above. Run `npm install --prefix tools && npx --prefix tools playwright install chromium`. Expected: `tools/package-lock.json` is created; Chromium downloads.
- [ ] **Step 2: Write failing tests in `tools/test/brand.test.mjs`:**

```js
test('markSvg is 1-bit, square-cornered and deterministic', () => {
  const a = markSvg({ size: 1024, border: 48, cell: 16 });
  assert.equal(a, markSvg({ size: 1024, border: 48, cell: 16 }));
  assert.doesNotMatch(a, /\brx=|gradient|filter|opacity/);
  const fills = new Set([...a.matchAll(/fill="(#[0-9A-Fa-f]{6})"/g)].map(m => m[1].toUpperCase()));
  assert.deepEqual([...fills].sort(), ['#111111', '#E8E9E4']);
  assert.match(a, /viewBox="0 0 1024 1024"/);
});
test('wordmarkSvg outlines text (no <text>, no font dependency)', async () => {
  const s = await wordmarkSvg({ theme: 'light', height: 64 });
  assert.doesNotMatch(s, /<text|font-family/);
  assert.match(s, /<path /);
});
test('dark wordmark inverts colours', async () => {
  const s = await wordmarkSvg({ theme: 'dark', height: 64 });
  assert.match(s, /#E8E9E4/i);
});
```

- [ ] **Step 3: Run.** `npm test --prefix tools`. Expected: FAIL, `Cannot find module '../brand/mark.mjs'`.
- [ ] **Step 4: Implement `markSvg` and `wordmarkSvg`** per the Interfaces. Use the Bayer 4×4 matrix `[[0,8,2,10],[12,4,14,6],[3,11,1,9],[15,7,13,5]]`. A cell is ink when `bayer[y%4][x%4]/16 < density(x,y)`. Merge each row's horizontal runs into one `<rect>` so the file stays small.
- [ ] **Step 5: Run.** `npm test --prefix tools`. Expected: 3 passing.
- [ ] **Step 6: Write the templates and render.**
  - `hero.html` (?theme=light|dark): 1600×560 at deviceScaleFactor 1, panel background, 2px ink frame. Left: wordmark, H1 "One-click root & rescue for the BOOX Note Max", Mono sub-label `MAGISK ROOT · EDL UN-BRICK · MAC / WINDOWS / LINUX`, ink-block chip `● NOTE MAX / FW 4.2`. Right: generic line-art tablet (an outlined rectangle with a dithered screen showing "Pick a job." and three card outlines). Do not trace product photos and do not draw a logo on the bezel.
  - `social.html`: 1280×640. The same composition plus the URL `n0dal.github.io/inkroot` in Mono.
  - `placeholders`: open `docs/superpowers/specs/assets/2026-10-25-look-directions.html` and screenshot the theme-B elements: the B home window → `home.png`; flowwrap `[data-t="B"]` screens → `root-step.png`, `rescue-cable.png`, `done.png`; the B home window with `filter:invert(1)` → `home-dark.png`. Each is 880×640 (pad on panel colour). Find the selectors in the file.
  Run: `npm run brand --prefix tools`
  Expected: every file in the Files list exists. `file assets/social/social-preview.png` reports `1280 x 640`, `assets/icon/inkroot-icon-1024.png` reports `1024 x 1024`, and the hero PNGs report `1600 x 560`.
- [ ] **Step 7: Look at the outputs.** Open the hero, social and icon PNGs and compare them to mockup card B. Check the hard borders, the dither, no rounded corners and Plex glyphs (not a fallback font). Fix any template that falls back to a system font.
- [ ] **Step 8: Commit.** `git add tools/package.json tools/package-lock.json tools/brand tools/test assets .gitignore && git commit -m "feat(brand): Panel wordmark, dithered mark, icon, README hero, social preview and screenshot placeholders"`

---

### Task 4: README rewrite

**Files:**
- Modify (rewrite): `README.md`

**Interfaces:**
- Consumes: Task 1 `ONE_LINE_DISCLAIMER`, `DISCLAIMER.md`, `THIRD_PARTY_NOTICES.md`. Task 2 `USB_DEBUG_BLOCK`. Task 3 `assets/readme/hero-{light,dark}.png`, `assets/screenshots/*.png`.
- Produces: `FAQ` (the Q/A copy below). Task 5 reuses it verbatim on the site and in JSON-LD. Stable download asset names, which are a contract with Task 10:
  - `Inkroot-macOS-AppleSilicon.dmg`
  - `Inkroot-macOS-Intel.dmg`
  - `Inkroot-Windows-x64-setup.exe`
  - `Inkroot-Linux-x86_64.AppImage`

  Download link form: `https://github.com/n0dal/inkroot/releases/latest/download/<name>`.

- [ ] **Step 1: Write `README.md` with this outline and copy.** Keep the order. Body copy may be tightened, but the headings, the facts and every quoted line stay as written.
  1. `<picture>` hero: `<source media="(prefers-color-scheme: dark)" srcset="assets/readme/hero-dark.png">`, img `assets/readme/hero-light.png`, alt "Inkroot — one-click root and rescue for the ONYX BOOX Note Max".
  2. H1 `Inkroot`. Pitch: **"One-click root and rescue for the ONYX BOOX Note Max."** Then: "Inkroot roots your BOOX Note Max with Magisk, un-bricks it over Qualcomm EDL when it's stuck on the logo, and puts it back to stock — on Mac, Windows and Linux, without a terminal."
  3. `## Download`: a four-row table (Your computer | Download). Rows: Mac with Apple Silicon (M1 and newer), Mac with Intel, Windows 10/11 (64-bit), Linux (x86_64, AppImage). Each row links to its stable asset name. Under the table: "Windows may say *Windows protected your PC*: click **More info → Run anyway** (Inkroot isn't code-signed on Windows). On Linux, make the AppImage executable first: `chmod +x Inkroot-Linux-x86_64.AppImage`." Then "Supports: **BOOX Note Max, firmware 4.2 (`TKQ1.230721.002`)** only. Inkroot checks before it does anything." Then `ONE_LINE_DISCLAIMER`.
  4. `## How it works` (3 numbered steps):
     1. **Plug in.** Turn on USB debugging and connect your Note Max. Inkroot checks the model, the firmware and the battery.
     2. **Safety copy.** Inkroot reads your tablet's startup software (the `boot` partition) over Qualcomm EDL, checks it twice, and keeps it on your computer. Nothing is written without it.
     3. **Root.** Inkroot patches that copy on the tablet with Magisk 30.7, verified boot kept on, writes it back, reads it back to check it, and confirms root and the boot service are running.
     Then the screenshots row: `assets/screenshots/home.png`, `root-step.png`, `done.png` (width 260 each, alt text describing each screen).
  5. `> [!CAUTION]` callout, heading **"Is your BOOX stuck on the ONYX logo?"**: "After a bad boot flash the tablet can look dead, hang on the logo or bootloop. That's almost always a soft brick. If you have a copy of its boot image, Inkroot's **Rescue** job walks you through the deep-flash cable step by step and writes it back. → [Rescue guide](https://n0dal.github.io/inkroot/rescue/) · [manual EDL guide](docs/02-unbrick-edl.md)". Include `assets/screenshots/rescue-cable.png`.
  6. `## Safety`, bullets: backup first (Root refuses without a verified stock copy of *this* tablet); verified boot stays on (`KEEPVERITY=true KEEPFORCEENCRYPT=true`, not configurable — patching with verity off bricks this device); every write is checked by read-back hash before the tablet restarts; unknown model or firmware is refused; one job at a time; a "Do you have a rescue cable?" check before the first write. Then "What can go wrong: a write can still fail (cable pulled, power lost). The tablet is then soft-bricked and **Rescue** writes your safety copy back with a deep-flash cable." Then the privacy statement (Global Constraints).
  7. `## FAQ`, as `<details><summary>` items, in this order, with this copy (`FAQ`):
     - **Will it void my warranty?** It can. Rooting changes your tablet's startup software, and Onyx may refuse warranty service for a modified device. Inkroot can put the original back (*Return to stock*), but that's no promise your warranty will be honoured.
     - **Which BOOX models and firmware are supported?** Only the BOOX Note Max on firmware 4.2 (build `TKQ1.230721.002`). Inkroot checks the model and four firmware fields and refuses anything else, because the wrong firmware can brick the tablet. On different firmware? [Open an issue](https://github.com/n0dal/inkroot/issues/new/choose) with your details. Support for other models comes through device profiles.
     - **Which cable do I need?** For backing up, rooting and returning to stock, the normal USB-C cable you charge with. Inkroot puts the tablet into Qualcomm's download mode (EDL) itself. Only to rescue a tablet that won't start do you need a **Qualcomm deep-flash (EDL) cable**, plugged into a real USB-A port (USB-C adapters usually don't work).
     - **How do I turn on USB debugging on a BOOX?** `USB_DEBUG_BLOCK`.
     - **Why can't the Magisk app grant root (su)?** On Onyx firmware 4.2, Onyx changed part of Android so that the Magisk app's root service crashes, and it can never show the "Allow root?" prompt. Root itself works. Inkroot installs a small boot service that runs a script as root at every boot instead. [Details](docs/03-notes-and-gotchas.md).
     - **Why EDL and not fastboot?** Onyx removed `fastboot flash` and `fastboot boot` on 4.x firmware and the bootloader is locked, so Qualcomm's EDL mode is the only way to read and write `boot`.
     - **Is it safe?** No root tool is risk-free. Inkroot makes and checks a safety copy first, checks the model and firmware, keeps verified boot on and checks every write. If something still goes wrong, the tablet can almost always be rescued with that copy. You use it at your own risk. Please read the [disclaimer](DISCLAIMER.md).
     - **Is rooting legal?** In the United States, the Copyright Office's DMCA §1201 exemptions have allowed jailbreaking smartphones and general-purpose portable computing devices since 2015, most recently renewed in 2024. Other countries have different rules, and your purchase terms may matter too. Please check your local law. This isn't legal advice and comes with no guarantee.
     - **Does Inkroot collect data?** The privacy statement, verbatim.
  8. `## For tinkerers`: keep the existing "What's here" table (guides 01–03 and boot-service, same links), the "Device this was verified on" table (Root row changed to `**Magisk 30.7**`) and "The 60-second version". In step 1, replace the USB-debugging text with "Enable **USB debugging** ([how](docs/01-rooting.md#what-you-need))." Add links to `scripts/patch-boot.sh` and `scripts/edl-restore.sh`.
  9. `## Credits`: Magisk (topjohnwu) and edl (bkerler), with links and roles as today. Add "Binaries Inkroot bundles are listed with their licences and pinned sources in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). adb (Google platform-tools) and the Qualcomm loader are third-party and redistributed as the edl project and others do." Replace the old "not redistributed here" paragraph with: "The repository itself contains no binaries; release builds fetch them hash-pinned at build time."
  10. `## License & disclaimer`: "[MIT](LICENSE). Read the [disclaimer](DISCLAIMER.md) before use." Then the not-affiliated footer line, verbatim. Then "BOOX and ONYX are trademarks of their respective owners."
- [ ] **Step 2: Verify.**
  Run: `grep -cE "30\.1|30\.x|tap build" README.md; npx --yes markdown-link-check -q README.md`
  Expected: `0`; every link `✓` (the four `releases/latest/download` links are allowed to 404 before the first release; add them to a `.markdown-link-check.json` `ignorePatterns` entry for `releases/latest/download`).
  Run: `grep -c "Inkroot is not affiliated with, endorsed by, or sponsored by" README.md`. Expected: `1`.
- [ ] **Step 3: Commit.** `git add README.md .markdown-link-check.json && git commit -m "docs: rewrite README as the Inkroot product page"`

---

### Task 5: Website (site/) and Pages deployment

**Files:**
- Create: `site/index.html`, `site/rescue/index.html`, `site/404.html`, `site/robots.txt`, `site/sitemap.xml`, `site/css/panel.css`, `site/js/downloads.js`, `site/.htmlvalidate.json`
- Create: `tools/site/build.mjs`, `tools/site/check.mjs`, `tools/test/downloads.test.mjs`
- Create: `.github/workflows/pages.yml`

**Interfaces:**
- Consumes: Task 3 `tools/brand/templates/panel.css` tokens, Plex fonts from `node_modules/@ibm/plex-*`, `assets/**` images. Task 4 `FAQ`, stable asset names. Task 2 `USB_DEBUG_BLOCK`. Task 1 `DISCLAIMER.md`.
- Produces:
  - `site/js/downloads.js` (ES module, no DOM at import) exports:
    - `STATIC_LINKS: Record<"mac-arm"|"mac-intel"|"windows"|"linux", string>`: the `releases/latest/download/<stable-name>` URLs.
    - `pickAssets(release: {tag_name: string, assets: {name: string, browser_download_url: string, size: number}[]} | null): {version: string|null, links: Record<key, {url: string, sizeMB: number|null}>}`
    - `detectOS(ua: string, platform?: string): "mac-arm"|"mac-intel"|"windows"|"linux"|null`. It returns `"mac-arm"` for any Mac unless `platform` says Intel, because Safari can't tell the two apart; both Mac buttons are always shown.
    - `init(doc: Document, fetchImpl = fetch): Promise<void>`, which wires the buttons.
  - `tools/site/build.mjs`: copies `site/` → `_site/`, copies the needed `assets/` files into `_site/assets/`, and copies the woff2 files (Plex Sans 400/600, Mono 400/600) into `_site/fonts/`.
  - `tools/site/check.mjs`: prints `site OK: <n> pages` or exits 1 with the reasons.

- [ ] **Step 1: Write failing tests in `tools/test/downloads.test.mjs`:**

```js
test('pickAssets maps stable names and sizes', () => {
  const r = pickAssets({ tag_name: 'v0.1.0', assets: [
    { name: 'Inkroot-macOS-AppleSilicon.dmg', browser_download_url: 'https://x/a.dmg', size: 12_582_912 }] });
  assert.equal(r.version, 'v0.1.0');
  assert.deepEqual(r.links['mac-arm'], { url: 'https://x/a.dmg', sizeMB: 12 });
  assert.equal(r.links.windows.url, STATIC_LINKS.windows);     // missing asset → static fallback
  assert.equal(r.links.windows.sizeMB, null);
});
test('pickAssets(null) (rate-limited / no release) falls back to static links', () => {
  const r = pickAssets(null);
  assert.equal(r.version, null);
  for (const k of ['mac-arm','mac-intel','windows','linux']) assert.equal(r.links[k].url, STATIC_LINKS[k]);
});
test('STATIC_LINKS use releases/latest/download and exact stable names', () => {
  assert.equal(STATIC_LINKS.linux, 'https://github.com/n0dal/inkroot/releases/latest/download/Inkroot-Linux-x86_64.AppImage');
  assert.equal(STATIC_LINKS['mac-intel'], 'https://github.com/n0dal/inkroot/releases/latest/download/Inkroot-macOS-Intel.dmg');
});
test('detectOS', () => {
  assert.equal(detectOS('Mozilla/5.0 (Windows NT 10.0; Win64; x64)'), 'windows');
  assert.equal(detectOS('Mozilla/5.0 (X11; Linux x86_64)'), 'linux');
  assert.equal(detectOS('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'), 'mac-arm');
  assert.equal(detectOS('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', 'MacIntel-x86'), 'mac-intel');
  assert.equal(detectOS('Mozilla/5.0 (iPhone)'), null);
});
test('init survives a failed fetch (403 rate limit) and leaves static hrefs', async () => { /* minimal fake doc with 4 <a data-dl=key>; fetchImpl resolves {ok:false,status:403}; expect hrefs === STATIC_LINKS */ });
```

- [ ] **Step 2: Run.** `npm test --prefix tools`. Expected: FAIL, module not found.
- [ ] **Step 3: Implement `site/js/downloads.js`.** `init` fetches `https://api.github.com/repos/n0dal/inkroot/releases/latest` with a 5 s `AbortController` timeout. On any failure it keeps the static hrefs (already in the HTML) and shows nothing extra. On success it sets the hrefs, fills `[data-version]` and size labels, and adds `data-primary` to the detected OS's button.
- [ ] **Step 4: Run.** `npm test --prefix tools`. Expected: all pass.
- [ ] **Step 5: Write the pages.** All pages share: `<html lang="en">`; `<meta name="viewport">`; `<link rel="canonical" href="https://n0dal.github.io/inkroot/…">`; OG tags (`og:title`, `og:description`, `og:url`, `og:type=website`, `og:image=https://n0dal.github.io/inkroot/assets/social/social-preview.png`, `og:image:width=1280`, `og:image:height=640`); `twitter:card=summary_large_image`; `<link rel="icon" href="assets/brand/inkroot-mark.svg">`; `css/panel.css` (Task 3 tokens + `@font-face` → `../fonts/*.woff2`, `font-display:swap`, light/dark via `prefers-color-scheme`); a header with the wordmark SVG; and a footer with: `ONE_LINE_DISCLAIMER` (rendered as HTML), the not-affiliated line verbatim, the privacy statement, and links (GitHub, Disclaimer, Third-party notices, MIT License). Use relative links within the site (base path `/inkroot/`).
  - **`index.html`**: `<title>Inkroot — BOOX root & un-brick tool for the Note Max (Magisk, EDL)</title>`; meta description "Root your ONYX BOOX Note Max with Magisk, or unbrick a BOOX stuck on the logo over Qualcomm EDL. Free, open-source app for Mac, Windows and Linux." Sections:
    - hero: H1 "Root and rescue your BOOX Note Max", the pitch from Task 4, chip `● NOTE MAX / FW 4.2`, hero image
    - `#download`: four `<a data-dl="…">` buttons with static hrefs; SmartScreen and `chmod` notes; "Supports firmware 4.2 (`TKQ1.230721.002`) only"
    - "How it works": the 3 steps from Task 4, with screenshots
    - rescue callout: H2 "BOOX stuck on the ONYX logo?", linking `rescue/`
    - "Why EDL? Onyx removed fastboot": the FAQ answer, as a short paragraph
    - `#faq`: the full `FAQ` in `<details>`
    - JSON-LD: `SoftwareApplication` (name Inkroot, applicationCategory `UtilitiesApplication`, operatingSystem `macOS, Windows, Linux`, `offers` price `0`, license `https://opensource.org/licenses/MIT`, url, downloadUrl `https://github.com/n0dal/inkroot/releases/latest`, image social preview) and `FAQPage`, whose `acceptedAnswer.text` equals the visible answer text.
  - **`rescue/index.html`**: `<title>BOOX stuck on logo or bootlooping? Unbrick your Note Max over EDL — Inkroot</title>`; meta description "Is your BOOX stuck on the ONYX logo or in a bootloop? It's usually a soft brick. Unbrick your BOOX Note Max over Qualcomm EDL with a deep-flash cable and your boot backup." Content, rewritten for web from `docs/02` and spec §3.3 in calm voice:
    - "Don't panic: a black screen or a logo hang is almost always a soft brick"
    - "What you need": a deep-flash (EDL) cable (what to search for: "Qualcomm deep flash cable" / "EDL cable"), a real USB-A port, and your boot backup
    - "With the Inkroot app": download links and the `rescue-cable.png` screenshot
    - "The cable steps": hold the cable button → plug into the tablet → hold power ~15 s until fully blank → keep the cable button held ~10 s more → release
    - "No USB-A port?": borrow a desktop or older laptop; Windows and Linux builds exist for this
    - "No backup?": not supported yet; open an issue
    - "Manual way": link to `docs/02`
    - JSON-LD `FAQPage` with 3 Q/As: "Why is my BOOX stuck on the logo?", "Can I unbrick a BOOX without a backup?", "Which cable do I need to unbrick a BOOX?"
  - **`404.html`**: "This page isn't here." plus links home and to rescue. `<meta name="robots" content="noindex">`. Use absolute `/inkroot/…` links (served at any depth).
  - **`robots.txt`**: `User-agent: *`, `Allow: /`, `Sitemap: https://n0dal.github.io/inkroot/sitemap.xml`.
  - **`sitemap.xml`**: two URLs, `https://n0dal.github.io/inkroot/` and `https://n0dal.github.io/inkroot/rescue/`.
  - **`site/.htmlvalidate.json`**: `{"extends":["html-validate:recommended"],"rules":{"no-inline-style":"off"}}`.
- [ ] **Step 6: Implement `tools/site/check.mjs`.** For each `_site/**/*.html` except 404, assert: exactly one canonical starting `https://n0dal.github.io/inkroot/`; `og:image` and `twitter:card` present; every `application/ld+json` block parses; on index, the FAQPage question count equals the `#faq details` count and each `acceptedAnswer.text` (whitespace-normalised, tags stripped) equals the visible answer; the footer contains the not-affiliated line; no `<img>` without `alt`; no request to `fonts.googleapis.com`. Also check that `sitemap.xml` URLs equal the canonicals.
- [ ] **Step 7: Verify locally.**
  Run: `npm run site --prefix tools && npx --prefix tools html-validate "_site/**/*.html" && npm run check-site --prefix tools && lychee --offline --no-progress _site`
  Expected: html-validate prints nothing (exit 0); `site OK: 3 pages`; lychee `0 Errors`. (`brew install lychee` locally; CI uses `lycheeverse/lychee-action@v2`.)
  Then serve `_site` from a parent folder that maps it to `/inkroot/` (`mkdir -p /tmp/pv && ln -sfn "$PWD/_site" /tmp/pv/inkroot && npx --yes http-server /tmp/pv -p 8080`) and open `http://localhost:8080/inkroot/` at 375px and 1280px wide, in light and dark. Check there's no horizontal scroll, the fonts are Plex, and the buttons point to `releases/latest/download/...`.
- [ ] **Step 8: Write `.github/workflows/pages.yml`.**
  - Triggers: `push` to `main` with paths `site/**`, `assets/**`, `tools/site/**`, `README.md`; also `workflow_dispatch`.
  - `permissions: {contents: read, pages: write, id-token: write}`; `concurrency: {group: pages, cancel-in-progress: true}`.
  - Job `build` (ubuntu-latest): checkout → `actions/setup-node@v4` (node 22, cache npm, `cache-dependency-path: tools/package-lock.json`) → `npm ci --prefix tools` → `npm run site --prefix tools` → `npx --prefix tools html-validate "_site/**/*.html"` → `npm run check-site --prefix tools` → `lycheeverse/lychee-action@v2` with `args: --offline --no-progress _site` → `actions/configure-pages@v5` → `actions/upload-pages-artifact@v3` (`path: _site`).
  - Job `deploy`: needs build, `environment: {name: github-pages, url: ${{ steps.d.outputs.page_url }}}`, `actions/deploy-pages@v4` (id `d`).
- [ ] **Step 9: Commit.** `git add site tools/site tools/test/downloads.test.mjs .github/workflows/pages.yml && git commit -m "feat(site): Panel-style GitHub Pages site with rescue page, SEO metadata and download resolution"`
- [ ] **Step 10: OWNER STEP (tell the owner; agents can't do it).**
  1. GitHub → *Settings → Pages → Build and deployment → Source: **GitHub Actions***.
  2. Run the workflow (*Actions → pages → Run workflow*) and wait for green.
  3. `curl -sI https://n0dal.github.io/inkroot/ | head -1` → `HTTP/2 200`.
  4. `gh repo edit n0dal/inkroot --homepage https://n0dal.github.io/inkroot/`
  5. *Settings → General → Social preview → Upload* `assets/social/social-preview.png`. GitHub has no API for this.

---

### Task 6: Binary lock file and stage-resources

**Files:**
- Create: `tools/binaries.lock.json`, `tools/stage-resources.mjs`, `tools/lib/stage.mjs`, `tools/test/stage.test.mjs`, `tools/test/fixtures/` (tiny zips built inside the test)

**Interfaces:**
- Consumes: `tools/package.json` (Task 3; `fflate`).
- Produces:
  - `tools/binaries.lock.json` (schema below), read by Tasks 7, 8 and 10.
  - `tools/lib/stage.mjs` exports:
    - `type Target = "darwin-aarch64"|"darwin-x86_64"|"windows-x86_64"|"linux-x86_64"`
    - `resolveTarget(env: NodeJS.ProcessEnv, platform: string, arch: string): Target`. It prefers `TAURI_ENV_PLATFORM` (`darwin|windows|linux`) + `TAURI_ENV_ARCH` (`aarch64|x86_64`), then `--target`, then the host. Anything else throws `Unsupported target`.
    - `planStaging(lock, target: Target, repoRoot: string): StagingItem[]`, where `StagingItem = {kind: "download"|"zip-member"|"copy"|"dir", dest: string, url?, sha256?, member?, src?, mode?: 0o755|0o644}`.
    - `stage({lock, target, repoRoot, outDir, fetchImpl?, cacheDir?, placeholders?: boolean, log?}): Promise<{files: string[]}>`
    - `checkStaged(outDir: string, target: Target): {ok: boolean, problems: string[]}`
    - `sha256File(path): Promise<string>`
  - CLI `node tools/stage-resources.mjs [--target T] [--placeholders] [--check] [--out DIR]`. Default out is `app/src-tauri/resources`. On darwin, after staging and when `APPLE_SIGNING_IDENTITY` is set, it calls Task 8's `signAll` + `verifyAll`. `--check` runs `checkStaged`. Exit codes: 0 ok, 1 verification failure, 2 usage.
  - Additive staged paths beyond the contract (they don't rename anything; flag for app-core): `tools/libwinpthread-1.dll` (Windows; adb r37 needs it), `legal/DISCLAIMER.md`, `legal/THIRD_PARTY_NOTICES.md`, `legal/platform-tools-NOTICE.txt`, `legal/edl-LICENSE.txt`, `legal/OFL.txt`.

`tools/binaries.lock.json`:

```json
{
  "schemaVersion": 1,
  "magisk": { "version": "30.7", "file": "Magisk-v30.7.apk",
    "url": "https://github.com/topjohnwu/Magisk/releases/download/v30.7/Magisk-v30.7.apk",
    "sha256": "e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5" },
  "platformTools": { "version": "37.0.1", "archives": {
    "darwin":  { "url": "https://dl.google.com/android/repository/platform-tools_r37.0.1-darwin.zip", "sha256": "ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d" },
    "linux":   { "url": "https://dl.google.com/android/repository/platform-tools_r37.0.1-linux.zip",  "sha256": "d230f13842f60f782a8645f9c813f8f845bf36089ea7289f28c48f17979313f1" },
    "windows": { "url": "https://dl.google.com/android/repository/platform-tools_r37.0.1-win.zip",    "sha256": "45f4d63113e895ebde0c90f194099a4676b6ac653bd28d54314a9e022bbc1a99" } },
    "members": { "darwin": ["platform-tools/adb"], "linux": ["platform-tools/adb"],
      "windows": ["platform-tools/adb.exe", "platform-tools/AdbWinApi.dll", "platform-tools/AdbWinUsbApi.dll", "platform-tools/libwinpthread-1.dll"],
      "notice": "platform-tools/NOTICE.txt" } },
  "edl": { "repo": "https://github.com/bkerler/edl", "commit": "2f8e89a848afaaef68997fcbcb5b178d958d497b",
    "version": "3.62", "loadersCommit": "bf0d8017eb97464530114a348c5157a6ea6a3372",
    "python": "3.12", "pyinstaller": "6.16.0", "libusbPackage": "1.0.30.0",
    "buildDir": "tools/.build/{target}/edl", "manifest": "tools/.build/{target}/edl.manifest.json" },
  "loader": { "path": "qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin",
    "url": "https://raw.githubusercontent.com/bkerler/Loaders/bf0d8017eb97464530114a348c5157a6ea6a3372/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin",
    "sha256": "7d85aafd5fc0872c12e61a0a480ca4c1042095a129278225de893b23444605ea", "size": 682616 },
  "libwdi": { "tag": "v1.5.1", "commit": "9b23b82a2dd1cbffc16d46c212f92c6bf8c0c602",
    "wdkRedist": { "url": "https://download.microsoft.com/download/0/5/F/05FD6919-6250-425B-86ED-9B095E54065A/wdfcoinstaller.msi", "sha256": "FILL_IN_TASK_7" },
    "build": "tools/.build/windows-x86_64/wdi-simple.exe" },
  "bootService": { "files": ["boot-service/init.bootsvc.rc", "boot-service/bootsvc.sh", "boot-service/inkroot-default-service.sh"] }
}
```

(macOS targets both use the `darwin` platform-tools archive, because its adb is universal.)

- [ ] **Step 1: Write failing tests in `tools/test/stage.test.mjs`.** Use a temp dir, a fake lock pointing at fixture zips/files served by `fetchImpl` stubs, and a fake `tools/.build/<target>/edl/` tree with a manifest. Tests:
  - `planStaging produces exactly the contract layout for each target`: assert the sorted `dest` lists. darwin-aarch64 → `["boot-service/bootsvc.sh","boot-service/init.bootsvc.rc","boot-service/inkroot-default-service.sh","legal/…5 files","magisk/Magisk-v30.7.apk","tools/adb","tools/edl/","tools/loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin"]`. windows-x86_64 adds `tools/adb.exe, tools/AdbWinApi.dll, tools/AdbWinUsbApi.dll, tools/libwinpthread-1.dll, tools/wdi-simple.exe` instead of `tools/adb`.
  - `stage aborts on hash mismatch and leaves resources untouched`: the fetch stub returns wrong bytes for Magisk. Expect a rejection whose message includes `Magisk-v30.7.apk`, the expected hash and the actual hash. Expect `outDir` to be absent or byte-identical to before. Staging goes to `outDir + ".tmp-<pid>"` and is renamed only on full success.
  - `corrupt cache entry is re-downloaded`: write garbage into `cacheDir/<sha256>`; expect the stub to be called once and staging to succeed.
  - `missing edl build for target fails with build hint`: expect the message to match `/tools\/\.build\/linux-x86_64\/edl.*python tools\/build_edl\.py --target linux-x86_64/`.
  - `edl manifest mismatch fails`: change one byte in the fake onedir; expect the failure to name the file.
  - `env target overrides host`: `resolveTarget({TAURI_ENV_PLATFORM:'darwin',TAURI_ENV_ARCH:'x86_64'}, 'darwin', 'arm64') === 'darwin-x86_64'`; `resolveTarget({}, 'win32', 'x64') === 'windows-x86_64'`; `resolveTarget({}, 'linux', 'arm64')` throws `Unsupported target`.
  - `staged executables are 0755 on POSIX` (skip on win32): `tools/adb` and `tools/edl/edl` have mode `0o755`; the APK and the loader have `0o644`.
  - `missing inkroot-default-service.sh fails with app-core hint`.
  - `check rejects placeholders`: `stage({placeholders:true})` creates every contract path containing `INKROOT-PLACEHOLDER\n` (edl as `tools/edl/edl` only). Then `checkStaged(...).ok === false` and `problems` lists them. A real stage gives `ok === true`.
- [ ] **Step 2: Run.** `npm test --prefix tools`. Expected: FAIL, `Cannot find module '../lib/stage.mjs'`.
- [ ] **Step 3: Implement `tools/lib/stage.mjs`** per the Interfaces:
  - Downloads use `fetch` with redirects and are cached by SHA-256 in `tools/.cache/`.
  - Zip members are extracted with `fflate.unzipSync` and a filter.
  - The edl onedir is copied recursively after every file is checked against `edl.manifest.json` (`{commit, target, files: {relpath: sha256}}`, written by Task 7).
  - `wdi-simple.exe` comes from `lock.libwdi.build`.
  - Boot-service files and legal files are copied from the repo. `edl-LICENSE.txt` comes from the edl build dir (Task 7 copies `LICENSE` there). `OFL.txt` comes from `tools/node_modules/@ibm/plex-sans/LICENSE.txt`.
  - Set modes, then do an atomic rename of the temp dir over `outDir`.
- [ ] **Step 4: Implement the CLI `tools/stage-resources.mjs`** (argument parsing; on darwin with `APPLE_SIGNING_IDENTITY`, dynamic `import('./lib/sign-macos.mjs')`, from Task 8; print one line per staged file with its SHA-256 prefix).
- [ ] **Step 5: Run.** `npm test --prefix tools`. Expected: all stage tests pass.
- [ ] **Step 6: Real-network smoke test for the downloads only.**
  Run: `node tools/stage-resources.mjs --target linux-x86_64 --out /tmp/inkroot-stage 2>&1 | tail -3`
  Expected: it fails only at the edl step with the build hint (Review Focus 2) after the Magisk, platform-tools and loader downloads verified. Then `ls tools/.cache | wc -l` → `3`.
- [ ] **Step 7: Commit.** `git add tools/binaries.lock.json tools/stage-resources.mjs tools/lib/stage.mjs tools/test && git commit -m "feat(tools): hash-pinned binaries lock and cross-platform resource staging"`

---

### Task 7: edl (PyInstaller onedir) and wdi-simple builders

**Files:**
- Create: `tools/build_edl.py`, `tools/edl/edl_entry.py`, `tools/edl/rthook_libusb.py`, `tools/edl/edl.spec`, `tools/edl/requirements-build.txt`, `tools/build-wdi.sh`
- Modify: `tools/binaries.lock.json` (`libwdi.wdkRedist.sha256`)

**Interfaces:**
- Consumes: Task 6 lock (`edl.*`, `libwdi.*`) and the build-dir and manifest paths.
- Produces:
  - `python tools/build_edl.py --target <Target>` → `tools/.build/<target>/edl/` (onedir: `edl` or `edl.exe` + `_internal/`, plus `LICENSE`) and `tools/.build/<target>/edl.manifest.json` `{commit, target, files:{relpath: sha256}}`. It runs only on a host whose native target equals `--target`, and refuses otherwise.
  - `bash tools/build-wdi.sh` (MSYS2 MINGW64 shell) → `tools/.build/windows-x86_64/wdi-simple.exe`.
  - The hidden self-test `edl --inkroot-selftest` prints `libusb OK <path>` and exits 0. It's for CI only; the app never calls it.

Decisions (justified):
- **One Python builder for all OSes**, not sh+ps1, so there's one code path.
- **Dependencies come from edl's own `uv.lock`** (`uv sync --frozen --no-dev` in the checkout) for reproducibility. Build-only extras are hash-pinned in `requirements-build.txt` (`pyinstaller==6.16.0`, `libusb-package==1.0.30.0`), generated by `uv pip compile --generate-hashes`.
- **libusb:**
  - macOS and Linux: at this commit edl asks pyusb for `"libusb-1.0.so"` on darwin, which doesn't exist on macOS, so the frozen app can't find libusb. `rthook_libusb.py` wraps `usb.backend.libusb1.get_backend` so that on darwin and linux `find_library` always returns `libusb_package.get_library_path()`. That library is bundled per arch by the `libusb-package` wheel, so no Homebrew and no system libusb are needed.
  - Windows: edl already prepends its own `edlclient/Windows/libusb-1.0.dll` to `PATH`. The spec includes `edlclient/Windows` as data at `edlclient/Windows`, and the WinUSB driver comes from wdi-simple.
- **Loaders are not bundled in the onedir.** The app passes `--loader=<resources>/tools/loaders/...` explicitly (spec §5). The Loaders submodule is still checked out at the pinned commit to confirm the pin.
- **macOS: split builds, not universal2.** pylzma has no universal2 wheels and is compiled from source per arch, so the only reliable path is the native runners `macos-15` (arm64) and `macos-15-intel` (x64). That gives two DMGs (spec §4 default).
- **wdi-simple is built, not downloaded.** libwdi releases ship only Zadig (an interactive GUI that can't be scripted) and no `wdi-simple.exe`. We build it from the pinned commit with libwdi's own MinGW CI recipe, reduced to WinUSB only (no libusb0 or libusbK), which still needs the WDF coinstaller redistributable.

- [ ] **Step 1: Write `tools/edl/edl_entry.py`.** If `sys.argv[1:] == ["--inkroot-selftest"]`: import `usb.backend.libusb1`, call `get_backend()`, print `libusb OK <path>` (Windows: `libusb OK PATH`), exit 0, or exit 3 with the error. Otherwise call `edlclient.edl.run()`.
- [ ] **Step 2: Write `tools/edl/rthook_libusb.py`** (the darwin/linux wrapper above; a no-op on Windows) and `tools/edl/edl.spec`:
  - `Analysis(['tools/edl/edl_entry.py'], pathex=[<checkout>], hiddenimports=collect_submodules('edlclient'), datas=collect_data_files('libusb_package') + ([(<checkout>/edlclient/Windows, 'edlclient/Windows')] if win), runtime_hooks=['tools/edl/rthook_libusb.py'])`
  - `EXE(name='edl', console=True, upx=False, target_arch=None)`
  - `COLLECT(name='edl')`
- [ ] **Step 3: Write `tools/build_edl.py`.**
  1. Map the host to a Target and refuse a mismatch.
  2. Clone `lock.edl.repo` to `tools/.build/src/edl`, `git checkout <commit>`, `git submodule update --init Loaders`, and assert `git -C Loaders rev-parse HEAD == loadersCommit`.
  3. `uv sync --frozen --no-dev --python <lock.edl.python>` in the checkout.
  4. `uv pip install --require-hashes -r tools/edl/requirements-build.txt` into that venv.
  5. `pyinstaller tools/edl/edl.spec --distpath tools/.build/<target> --workpath tools/.build/work-<target> --noconfirm`.
  6. Copy the checkout's `LICENSE` into the onedir.
  7. Run the self-test (`<onedir>/edl --inkroot-selftest`) and `<onedir>/edl --help`, and fail unless the output contains `edl.py r <partitionname> <filename>`.
  8. Write the manifest.
- [ ] **Step 4: Run it on this Mac** (host is darwin-aarch64).
  Run: `python3 -I tools/build_edl.py --target darwin-aarch64`
  Expected: it ends with `libusb OK …/_internal/libusb_package/libusb-1.0.dylib` and `manifest: tools/.build/darwin-aarch64/edl.manifest.json (<n> files)`.
  Then run: `node tools/stage-resources.mjs --target darwin-aarch64 --out /tmp/inkroot-stage && node tools/stage-resources.mjs --check --target darwin-aarch64 --out /tmp/inkroot-stage`
  Expected: staging succeeds (it needs `boot-service/inkroot-default-service.sh`; if app-core hasn't created it yet, the expected failure is the app-core hint), then `check OK`.
  Optional hardware smoke test (owner present, tablet booted): `adb reboot edl` then `/tmp/inkroot-stage/tools/edl/edl printgpt --memory=ufs --loader=/tmp/inkroot-stage/tools/loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin` → GPT listing including `boot_a`/`boot_b`, then `…/edl reset`. This step is read-only.
- [ ] **Step 5: Pin the WDK redistributable hash.**
  Run: `curl -sSL -o /tmp/wdf.msi "https://download.microsoft.com/download/0/5/F/05FD6919-6250-425B-86ED-9B095E54065A/wdfcoinstaller.msi" && shasum -a 256 /tmp/wdf.msi`
  Write the hash into `libwdi.wdkRedist.sha256`. Expected: a 64-hex value, with no `FILL_IN` left (`grep -c FILL_IN tools/binaries.lock.json` → `0`). This host couldn't reach that URL while the plan was written. If it's unreachable here too, do this step in the first CI run of `build-wdi`, which prints the hash, and commit it.
- [ ] **Step 6: Write `tools/build-wdi.sh`** (set -euo pipefail; MSYS2 MINGW64):
  1. Download the WDK MSI and verify its SHA-256 from the lock (`node -e` reading the lock).
  2. `msiexec /a wdk-redist.msi /qn TARGETDIR=$(cygpath -w $PWD/wdk)`.
  3. Clone libwdi, check out the lock commit, `./bootstrap.sh`.
  4. `./configure --build=x86_64-w64-mingw32 --host=x86_64-w64-mingw32 --enable-examples-build --disable-debug --disable-shared --disable-32bit --with-wdkdir="wdk/Windows Kits/8.0" --with-wdfver=1011`, then `make`.
  5. Copy `examples/wdi-simple.exe` to the build path.
  6. Smoke test: `./wdi-simple.exe --help 2>&1 | grep -qi usage`.
  It's verified in CI (Task 10 `build-wdi` job). Expected log tail: `wdi-simple.exe OK sha256=<hex>`.
- [ ] **Step 7: Commit.** `git add tools/build_edl.py tools/edl tools/build-wdi.sh tools/binaries.lock.json && git commit -m "build: reproducible PyInstaller edl and libwdi wdi-simple builders"`

---

### Task 8: macOS nested-signing helper and release-notes renderer

**Files:**
- Create: `tools/lib/sign-macos.mjs`, `tools/macos/edl.entitlements`, `tools/render-release-notes.mjs`, `.github/release-notes.md.tmpl`, `.github/hardware-verification.json`, `tools/test/sign.test.mjs`, `tools/test/release-notes.test.mjs`

**Interfaces:**
- Consumes: Task 6 lock and the staged dir. Task 1 `ONE_LINE_DISCLAIMER`. Task 4 stable asset names.
- Produces:
  - `findMachO(dir: string): Promise<string[]>`. It recursively lists regular files whose first 4 bytes are `feedface|feedfacf|cefaedfe|cffaedfe|cafebabe` (with the `cafebabe` + small-arch-count check that excludes Java classes), sorted deepest-first so libraries are signed before the executables that load them.
  - `signAll(dir, identity: string, {entitlementsFor: (path) => string|null}): Promise<void>`. For each file: `codesign --force --timestamp --options runtime --sign <identity> [--entitlements <e>] <file>`. Then, for each `*.framework` directory, sign the bundle after its contents. The entitlements are `tools/macos/edl.entitlements` for `tools/edl/edl` only.
  - `verifyAll(dir, teamId: string): Promise<{ok: boolean, problems: string[]}>`. It parses `codesign -dv --verbose=4 <f>` stderr and requires `TeamIdentifier=<teamId>`, `Timestamp=` and a `flags=` value containing `runtime`.
  - `tools/macos/edl.entitlements`: plist with `com.apple.security.cs.allow-unsigned-executable-memory` = true. This is needed by Python/ctypes (libffi) under the hardened runtime. Nothing else.
  - `renderReleaseNotes({version: string, verification: {macos: string|null, windows: string|null, linux: string|null}, lock}): string`. CLI: `node tools/render-release-notes.mjs v0.1.0 > notes.md`.
  - `.github/hardware-verification.json`: `{"macos": null, "windows": null, "linux": null}`. The owner sets an ISO date per OS after the §8 pass.

- [ ] **Step 1: Write failing tests.**
  - `sign.test.mjs`: build fake files in a temp dir: thin Mach-O (`cffaedfe` header), fat (`cafebabe` + nfat=2), a dylib under `_internal/lib/`, a text file, and a Java-class-like file (`cafebabe` + major version 52). Expect `findMachO` to return exactly the three Mach-O paths, with the deeper dylib before the top-level executable. Run the real `codesign` only when `process.platform === 'darwin'`: ad-hoc sign (`identity "-"`) a copy of `/usr/bin/true`, and expect `verifyAll(dir, 'X')` to report a missing TeamIdentifier (this proves the check rejects).
  - `release-notes.test.mjs`:

```js
test('pre-hardware-pass notes say beta per OS and carry the disclaimer', () => {
  const md = renderReleaseNotes({ version: 'v0.1.0', verification: { macos: null, windows: null, linux: null }, lock });
  assert.match(md, /^## Inkroot v0\.1\.0/m);
  assert.ok(md.includes(ONE_LINE_DISCLAIMER));
  for (const os of ['macOS', 'Windows', 'Linux']) assert.ok(md.includes(`beta — not yet hardware-verified on ${os}`));
  for (const n of ['Inkroot-macOS-AppleSilicon.dmg','Inkroot-macOS-Intel.dmg','Inkroot-Windows-x64-setup.exe','Inkroot-Linux-x86_64.AppImage','SHA256SUMS.txt']) assert.ok(md.includes(n));
  assert.ok(md.includes('Magisk 30.7') && md.includes('2f8e89a848afaaef68997fcbcb5b178d958d497b') && md.includes('r37.0.1'));
});
test('verified OS shows the date, others stay beta', () => {
  const md = renderReleaseNotes({ version: 'v0.2.0', verification: { macos: '2026-11-02', windows: null, linux: null }, lock });
  assert.ok(md.includes('hardware-verified on macOS (2026-11-02, Note Max, firmware TKQ1.230721.002)'));
  assert.ok(md.includes('beta — not yet hardware-verified on Windows'));
});
```

- [ ] **Step 2: Run.** `npm test --prefix tools`. Expected: FAIL, modules missing.
- [ ] **Step 3: Implement** `sign-macos.mjs`, `render-release-notes.mjs` and the template. The template's sections:
  1. `## Inkroot {{version}}`
  2. the disclaimer as a blockquote
  3. `### Download` (the four-row table from Task 4, with the SmartScreen and `chmod` notes)
  4. `### Status` (one line per OS)
  5. `### What's inside` (Magisk, edl commit, platform-tools version, from the lock)
  6. `### Verify your download` ("`shasum -a 256 -c SHA256SUMS.txt`")
  7. a link to `THIRD_PARTY_NOTICES.md` (GPL source offer)
- [ ] **Step 4: Run.** `npm test --prefix tools`. Expected: all pass.
- [ ] **Step 5: Commit.** `git add tools/lib/sign-macos.mjs tools/macos tools/render-release-notes.mjs .github/release-notes.md.tmpl .github/hardware-verification.json tools/test && git commit -m "build: macOS nested-binary signing/verification and release-notes renderer"`

---

### Task 9: App wiring, Tauri icon set and ci.yml

**Depends on the app-core plan** having created `app/package.json` (scripts `build`, `check`, `test`), `app/src-tauri/tauri.conf.json`, `app/src-tauri/Cargo.toml`, and `boot-service/inkroot-default-service.sh`.

**Files:**
- Modify: `app/src-tauri/tauri.conf.json` (`build.beforeBuildCommand` only), `app/package.json` (add the `stage` script)
- Create (generated): `app/src-tauri/icons/*` via `tauri icon`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: Task 6 CLI; Task 3 `assets/icon/inkroot-icon-1024.png`; app-core's existing `bundle.resources` (do not change it; confirm it includes `resources/**/*` or the equivalent map, and flag it to the coordinator if not).
- Produces:
  - `app/package.json` script `"stage": "npm ci --prefix ../tools --no-audit --no-fund && node ../tools/stage-resources.mjs"`
  - `tauri.conf.json` → `"beforeBuildCommand": "npm run stage && npm run build"`. Tauri runs it from `app/`, and it inherits `TAURI_ENV_PLATFORM`/`TAURI_ENV_ARCH`.
  - CI placeholders: `node tools/stage-resources.mjs --placeholders` makes `cargo` builds pass `tauri_build`'s resource checks without real binaries.

- [ ] **Step 1: Edit** `tauri.conf.json` and `app/package.json` as above.
- [ ] **Step 2: Generate the icon set.**
  Run: `cd app && npx tauri icon ../assets/icon/inkroot-icon-1024.png -o src-tauri/icons`
  Expected: `icon.icns`, `icon.ico`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png` and the Square*Logo PNGs are written. `bundle.icon` in `tauri.conf.json` lists the files that exist (`jq '.bundle.icon' app/src-tauri/tauri.conf.json`).
- [ ] **Step 3: Write `.github/workflows/ci.yml`.**
  - `on: [push, pull_request]`, `concurrency` per ref.
  - Jobs:
    - **`tools`**: matrix `[ubuntu-latest, macos-15, windows-latest]`. setup-node 22 → `npm ci --prefix tools` → `npm test --prefix tools`.
    - **`rust`** (ubuntu-22.04):
      1. `sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev build-essential`
      2. `dtolnay/rust-toolchain@stable` with `components: rustfmt, clippy`
      3. `Swatinem/rust-cache@v2` (`workspaces: app/src-tauri`)
      4. setup-node 22 → `npm ci --prefix tools` → `node tools/stage-resources.mjs --placeholders --target linux-x86_64`
      5. in `app/src-tauri`: `cargo fmt --all -- --check`, then `cargo clippy --all-targets -- -D warnings`, then `cargo test`
    - **`frontend`** (ubuntu-latest): in `app/`, `npm ci`, `npm run check`, `npm test` (vitest).
    - **`site`**: the same build and check steps as pages.yml, without deploy.
    - **`docs`**: `grep -rnE "30\.1|30\.x|tap build" README.md docs/0*.md boot-service scripts && exit 1 || true`.
- [ ] **Step 4: Verify locally,** then push a branch.
  Run: `node tools/stage-resources.mjs --placeholders && (cd app/src-tauri && cargo clippy --all-targets -- -D warnings) && (cd app && npm run check && npm test)`
  Expected: clippy reports no warnings, and check and test pass. Then `node tools/stage-resources.mjs --check` → exit 1 listing placeholders (correct: the placeholders must never ship).
  Push the branch: all CI jobs green (`gh run watch`).
- [ ] **Step 5: Commit.** `git add app/package.json app/src-tauri/tauri.conf.json app/src-tauri/icons .github/workflows/ci.yml && git commit -m "ci: stage resources before Tauri build, generate icon set, add CI workflow"`

---

### Task 10: release.yml and docs/RELEASING.md

**Files:**
- Create: `.github/workflows/release.yml`, `docs/RELEASING.md`

**Interfaces:**
- Consumes: Task 7 builders, Task 8 signer and renderer, Task 9 wiring, the Task 4 stable names, and `.github/hardware-verification.json`.
- Produces: a draft GitHub Release for tag `vX.Y.Z` with exactly these assets:
  - `Inkroot-macOS-AppleSilicon.dmg`
  - `Inkroot-macOS-Intel.dmg`
  - `Inkroot-Windows-x64-setup.exe`
  - `Inkroot-Linux-x86_64.AppImage`
  - `SHA256SUMS.txt`

- [ ] **Step 1: Write `release.yml`.**
  - `on: push: tags: ['v*']` plus `workflow_dispatch` (input `tag`).
  - `permissions: contents: write`.
  - Jobs:
    1. **`prepare`** (ubuntu):
       1. Checkout, then assert the tag without `v` equals `jq -r .version app/src-tauri/tauri.conf.json` and `app/package.json` `.version`; on a mismatch fail with `tag vX ≠ app version Y`.
       2. `npm ci --prefix tools`; `node tools/render-release-notes.mjs $TAG > notes.md`.
       3. `gh release create $TAG --draft --title "Inkroot $TAG" --notes-file notes.md`. Not `--prerelease`: `/releases/latest/download/...` links only resolve to a published non-prerelease, so "beta" is stated in the notes instead.
    2. **`build-edl`**: matrix `{os: macos-15, target: darwin-aarch64}`, `{os: macos-15-intel, target: darwin-x86_64}`, `{os: windows-latest, target: windows-x86_64}`, `{os: ubuntu-22.04, target: linux-x86_64}`. Steps: `actions/setup-python@v5` (3.12) → `astral-sh/setup-uv@v6` → `python -I tools/build_edl.py --target ${{ matrix.target }}` → `actions/upload-artifact@v4` (`name: edl-${{ matrix.target }}`, path `tools/.build/${{ matrix.target }}/`). ubuntu-22.04 sets the glibc floor for the AppImage.
    3. **`build-wdi`** (windows-latest, `msys2/setup-msys2@v2`, `msystem: MINGW64`, install `mingw-w64-x86_64-toolchain base-devel autotools git`): `bash tools/build-wdi.sh` → upload `wdi-windows-x86_64`.
    4. **`bundle`**: `needs: [prepare, build-edl, build-wdi]`. Matrix:

       | os | target | tauri args | stable name |
       |---|---|---|---|
       | macos-15 | darwin-aarch64 | `--target aarch64-apple-darwin --bundles app,dmg` | `Inkroot-macOS-AppleSilicon.dmg` |
       | macos-15-intel | darwin-x86_64 | `--target x86_64-apple-darwin --bundles app,dmg` | `Inkroot-macOS-Intel.dmg` |
       | windows-latest | windows-x86_64 | `--bundles nsis` | `Inkroot-Windows-x64-setup.exe` |
       | ubuntu-22.04 | linux-x86_64 | `--bundles appimage` | `Inkroot-Linux-x86_64.AppImage` |

       Steps:
       1. Checkout, setup-node 22, Rust stable with the target, rust-cache. Linux: apt deps (as ci.yml) + `libfuse2`.
       2. `actions/download-artifact@v4` `edl-${{ matrix.target }}` → `tools/.build/${{ matrix.target }}/`. Windows also downloads `wdi-windows-x86_64` → `tools/.build/windows-x86_64/`.
       3. **macOS only, "Import Developer ID certificate":** decode `APPLE_CERTIFICATE` into `cert.p12`, `security create-keychain -p "$KC_PW" build.keychain`, `security default-keychain -s build.keychain`, `security unlock-keychain`, `security set-keychain-settings -t 3600 -u`, `security import cert.p12 -k build.keychain -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign`, `security set-key-partition-list -S apple-tool:,apple: -s -k "$KC_PW" build.keychain`. `KC_PW` = `openssl rand -hex 16`. The cert is imported here because `beforeBuildCommand` (staging + nested signing) runs before Tauri's own keychain import. `APPLE_CERTIFICATE` is not passed to tauri-action, so it uses this keychain.
       4. `npm ci` in `app/`.
       5. `tauri-apps/tauri-action@v0` with `projectPath: app`, `args: ${{ matrix.args }}`, and no `tagName` (build only; this job uploads the assets). Env: `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` (macOS), `GITHUB_TOKEN`. Tauri signs the app with the hardened runtime, notarizes it and staples it.
       6. `node tools/stage-resources.mjs --check --target ${{ matrix.target }}` (no placeholders made it in).
       7. **macOS "Verify signatures & notarization":** let `APP=app/src-tauri/target/<triple>/release/bundle/macos/Inkroot.app`.
          - `node -e` calling `verifyAll("$APP/Contents/Resources", process.env.APPLE_TEAM_ID)`. Expect `ok`.
          - `codesign --verify --deep --strict --verbose=2 "$APP"`. Expect `valid on disk` and `satisfies its Designated Requirement`.
          - `spctl --assess --type execute -vv "$APP"`. Expect `accepted` and `source=Notarized Developer ID`.
          - `xcrun stapler validate "$APP"`. Expect `The validate action worked!`
          - Then notarize and staple the DMG itself: `xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait` (expect `status: Accepted`), `xcrun stapler staple "$DMG"`, and `spctl --assess --type open --context context:primary-signature -vv "$DMG"` (expect `accepted`).
       8. Rename the single bundle output (glob `bundle/dmg/*.dmg`, `bundle/nsis/*-setup.exe`, `bundle/appimage/*.AppImage`) to the stable name, then `gh release upload $TAG <stable> --clobber`.
    5. **`checksums`**: needs bundle. `gh release download $TAG -p 'Inkroot-*'`, then `sha256sum Inkroot-* > SHA256SUMS.txt`, then `gh release upload $TAG SHA256SUMS.txt --clobber`. Assert there are exactly 4 `Inkroot-*` files.
- [ ] **Step 2: Write `docs/RELEASING.md`:**
  1. **One-time: GitHub secrets.** A table of the exact six names: `APPLE_CERTIFICATE` (base64 of the .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (e.g. `Developer ID Application: Your Name (ABCDE12345)`), `APPLE_ID` (Apple Account email), `APPLE_PASSWORD` (an app-specific password, not your login password), `APPLE_TEAM_ID` (10 characters).
  2. **Export the Developer ID certificate (step by step):**
     1. Keychain Access → *Certificate Assistant → Request a Certificate From a Certificate Authority…* → your email → *Saved to disk*.
     2. developer.apple.com → *Certificates, IDs & Profiles → Certificates → +* → **Developer ID Application** (needs the Account Holder role) → upload the CSR → download → double-click to install.
     3. Keychain Access → *login → My Certificates* → right-click *Developer ID Application: …* → *Export…* → `.p12` with a strong password.
     4. `base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE -R n0dal/inkroot`
     5. `gh secret set APPLE_CERTIFICATE_PASSWORD -R n0dal/inkroot`
     6. `security find-identity -v -p codesigning` → copy the quoted name → `gh secret set APPLE_SIGNING_IDENTITY`.
     7. appleid.apple.com → *Sign-In and Security → App-Specific Passwords* → generate "inkroot-notary" → `gh secret set APPLE_PASSWORD`.
     8. `gh secret set APPLE_ID`, then `gh secret set APPLE_TEAM_ID` (from developer.apple.com → *Membership details*).
     9. Delete the local `.p12` afterwards.
  3. **Cut a release:**
     1. Bump the version in `app/src-tauri/tauri.conf.json`, `app/package.json` and `app/src-tauri/Cargo.toml`, then commit.
     2. `git tag vX.Y.Z && git push origin vX.Y.Z`
     3. `gh run watch`
     4. Open the draft release and check the 5 assets and the notes.
     5. Download the Mac DMG on a clean Mac: it should open with no Gatekeeper warning.
     6. Publish. Don't tick "pre-release", or the README/site download links break.
  4. **Hardware test checklist (before the first public release; spec §8).** The order matters because the owner's tablet is currently rooted.
     1. Detection and gates show "Already rooted ✓".
     2. **Import** `boot_b.stock.img`; its hash matches the known-stock list.
     3. **Return to stock** from macOS; confirm `magisk -c` now fails.
     4. **Back up** from macOS; the new backup is `stock: true` and its `boot_b` SHA-256 is `2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc`.
     5. **Root** end-to-end from macOS; verify Magisk 30.7 and the bootsvc log line (`uid=0 … u:r:magisk:s0`).
     6. **Rescue** with the deep-flash cable from a USB-A host (Linux AppImage and/or Windows).
     7. Windows driver install on a real Windows machine (WinUSB for `05c6:9008`; confirm the BOOX's ADB USB IDs).
     8. Optional: Reinstall boot service (hook-present path).
     9. Find out which USB-debugging method (A or B) works on 4.2. Put it first in the app, `docs/01`, the README and the site, and drop the "not yet confirmed" note.

     After each OS passes, set its date in `.github/hardware-verification.json` and commit. The next release's notes switch from "beta — not yet hardware-verified on <OS>" automatically.
  5. **Troubleshooting:** notarization "The binary is not signed with a valid Developer ID certificate" → look at `verifyAll` problems (a nested file was missed). Get the log with `xcrun notarytool log <id>`.
- [ ] **Step 3: Dry run.** Push tag `v0.0.1-rc.1` from a branch with versions set to `0.0.1-rc.1` (and delete the draft and the tag afterwards).
  Expected: every job is green; the draft has the 5 assets; the macOS verify step logs `source=Notarized Developer ID` and `The validate action worked!`; the notes contain three `beta — not yet hardware-verified on …` lines.
  If the Apple secrets aren't set yet, the macOS signing steps fail. That is expected: stop and hand the owner `docs/RELEASING.md` §1–2.
- [ ] **Step 4: Commit.** `git add .github/workflows/release.yml docs/RELEASING.md && git commit -m "ci: signed, notarized 3-OS release workflow and release guide"`

---

## Self-review notes (decisions flagged for the coordinator)

- The spec §5 tree says `tools/fetch-binaries.*`, `app/src-tauri/binaries/` and `app/resources/`. This plan follows the coordinator's contract instead: `tools/stage-resources.mjs` → `app/src-tauri/resources/`. Spec §2/§3.2 still mention "Magisk 30.1" in two places; 30.7 is used throughout.
- Additive staged files (no contract names changed): `tools/libwinpthread-1.dll` (adb r37 on Windows needs it) and `legal/*` (GPL/OFL notices for the About screen). app-core should not have to reference them except optionally from About.
- The Linux udev rule (spec §4) is not staged here. app-core owns embedding it. Flag it if app-core expects a resource file.
