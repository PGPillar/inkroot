# Inkroot app-ui Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Svelte frontend of the Inkroot desktop app in the "Panel" look. It renders every device status, gate, flow state, prompt and result from the backend contract, and a scripted mock backend lets it run and be screenshotted in a plain browser.

**Architecture:** One `AppState` class (Svelte 5 runes) holds device status, the current `FlowState`, progress, logs, settings, backups and the user's route. A pure `resolveScreen()` maps that state to one screen. All backend access goes through a `Backend` interface with two implementations: Tauri (`invoke`/`listen`) and a mock scenario engine chosen by `?mock=<scenario>`. Host actions (file dialog, opening URLs, window close, clipboard) go through a separate `Platform` interface. All user-facing words live in one copy module, which has a completeness test against the contract.

**Tech Stack:** Svelte 5 (runes) + TypeScript + Vite (scaffolded by app-core Task 1), `@tauri-apps/api` v2, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-opener`, `@fontsource/ibm-plex-sans` + `@fontsource/ibm-plex-mono`, Vitest + jsdom + `@testing-library/svelte` + `@testing-library/jest-dom`, `@playwright/test` (Chromium) for screenshots.

**Spec:** `docs/superpowers/specs/2026-10-25-inkroot-design.md` (esp. §1, §3, §6, §7b), contract `docs/superpowers/specs/2026-10-25-inkroot-ipc-contract.md`, visual reference `docs/superpowers/specs/assets/2026-10-25-look-directions.html` (direction B), cable wording from `docs/02-unbrick-edl.md`.

## Global Constraints

- Scope is `app/src/**`, plus `app/vitest.config.ts`, `app/playwright.config.ts`, `app/e2e/**`, `app/package.json` (deps and scripts only) and repo-root `assets/screenshots/*.png`. Do not touch `app/src-tauri/**`, `app/vite.config.ts` or anything outside `app/`.
- Contract names (types, command names, arg keys, event names, enum literals) are used **verbatim** from the contract. `src/lib/ipc.ts` is the only file that calls `invoke`/`listen`.
- Palette: panel `#E8E9E4`, ink `#111111`, muted `#555555`. No other colours. No accent colour. Dark mode is the inverse, following `prefers-color-scheme`.
- Type: IBM Plex Sans (400/600) and IBM Plex Mono (400/500/600), bundled through `@fontsource`. **No CDN and no network font loads.** The app must work offline.
- Shapes: 2px ink borders, `border-radius: 0` everywhere, no `box-shadow`, no gradients except the dither patterns.
- Danger is never red. It is shown with a 4px border, a `warning` dither icon and the wording.
- Motion: the e-ink refresh is `@keyframes eink` verbatim from the mockup, `.52s steps(1,end)`. `prefers-reduced-motion: reduce` gives a plain cut, and indeterminate progress also stops moving.
- Voice (§6): plain, warm, short, second person. No jargon on the main path. Technical terms appear only in the Details panel. Never blame the user. All visible strings come from `src/lib/copy.ts` (exact text in Task 5). Backend `FlowError.message` is shown as given.
- No destructive action is the default or one mis-click away. `danger` options are never the focused or first-tab-order button. "Continue without one" needs a second, deliberate confirmation.
- Risk acknowledgement is required before `start_flow` for `root`, `unroot`, `rescue` (and `reinstallBootService`, see Task 4). `RISK_NOTICE_VERSION = 1`.
- Trademark hygiene: no BOOX or Onyx logos. Illustrations are original, generic line art plus dither (inline SVG). No raster images and no traced product photos.
- Privacy: the UI makes no network requests. The only outbound links open in the system browser through `Platform.openExternal`.
- Window is about 880×640. Layouts must fit 880×640 without scrolling on the main path. Details may scroll.

## Review Focus

1. **A bricked tablet looks like "no device".** The panicking user (§1 persona 2) lands on the "Plug in your Note Max" gate and must reach Rescue from there, and from the EDL gate, without a booted tablet. Test in Task 9 (`gate-none offers Rescue`).
2. **The device drops off or changes mid-flow** (the tablet reboots into EDL, adb disappears during `waitBoot`). The flow screen must stay up and must not flip to a gate screen. Test in Task 4 (`flow outranks device gates`).
3. **Late or foreign events.** `flow://progress` / `flow://log` for another `flowId`, or a `flow://state` for a flow the user already dismissed, must not resurrect or corrupt the screen. Test in Task 4 (`ignores events for dismissed flows`, `ignores progress for other flowId`).
4. **`start_flow` rejects** (`busy`, or the risk-not-accepted `internal` error). The user must see a failure screen with the message, not a silent no-op. Test in Task 4 (`start rejection becomes a failed flow`).
5. **Double-clicks on answers and Continue** would send duplicate `answer_prompt`/`start_flow` calls. Buttons disable until the next state arrives. Test in Task 8 (`second click is ignored until next prompt`).

---

## File Structure

```
app/
├─ package.json                      (modify: deps + scripts)
├─ vitest.config.ts                  merges vite.config.ts; jsdom; svelteTesting
├─ playwright.config.ts              webServer = vite dev on 5179, Chromium, 880×640 @2x
├─ e2e/screenshots.spec.ts           writes ../assets/screenshots/*.png
└─ src/
   ├─ main.ts                        (modify) font imports, global css, mount App
   ├─ App.svelte                     (replace placeholder) shell + screen switch
   ├─ styles/tokens.css              design tokens (light + dark)
   ├─ styles/global.css              reset, base type, eink keyframes, reduced motion
   ├─ lib/
   │  ├─ ipc.ts                      contract types + Backend interface + Tauri impl
   │  ├─ platform.ts                 Platform interface + Tauri impl
   │  ├─ backend.ts                  getBackend()/getPlatform(): Tauri vs mock selection
   │  ├─ legal.ts                    RISK_NOTICE_VERSION, RISK_REQUIRED kinds
   │  ├─ links.ts                    every external URL
   │  ├─ flows.ts                    FLOW_STEPS, ALL_STEP_IDS, ALL_PROMPT_KINDS, ALL_ERROR_CODES
   │  ├─ copy.ts                     every user-facing string (Task 5 tables) + fill()
   │  ├─ format.ts                   formatBytes, shortSha, backupLine, formatChip, revealLabel
   │  ├─ screen.ts                   Route, ScreenId, resolveScreen()
   │  ├─ state.svelte.ts             AppState class
   │  ├─ context.ts                  setApp()/getApp()
   │  ├─ mock/
   │  │  ├─ engine.ts                createMockBackend(scenario), createMockPlatform(scenario)
   │  │  ├─ fixtures.ts              NOTE_MAX, STOCK_BACKUP, etc.
   │  │  ├─ fragments.ts             reusable Beat[] pieces (enterEdl, readPartitions, writeBoot…)
   │  │  └─ scenarios.ts             SCENARIOS: Record<string, Scenario>
   │  ├─ components/                 Button, DitherIcon, JobCard, StepFrame, Checklist,
   │  │                              SegmentedProgress, RefreshTransition, DetailsLog,
   │  │                              AppHeader, DeviceChip, PromptView, CloseGuardDialog, LiveStatus
   │  ├─ art/                        Dither.svelte (pattern defs), Tablet, DeepFlashCable, UsbAPort,
   │  │                              Hand, PressIndicator, PhoneMenu (⋮ settings sketch)
   │  └─ screens/                    Home, GateScreen, UsbDebugGuide, AlreadyRooted, RiskNotice,
   │                                 About, Backups, RescuePick, FlowScreen, CableGuide,
   │                                 DoneView, FailureView
   └─ test/setup.ts, test/render.ts  jest-dom, matchMedia stub, renderWithApp()
```

---

### Task 1: Tooling, fonts, design tokens and global CSS

**Files:**
- Modify: `app/package.json`
- Create: `app/vitest.config.ts`, `app/src/test/setup.ts`, `app/src/styles/tokens.css`, `app/src/styles/global.css`
- Modify: `app/src/main.ts`
- Test: `app/src/styles/tokens.test.ts`

**Interfaces:**
- Consumes: app-core Task 1 scaffold (`app/vite.config.ts`, `app/src/main.ts`, placeholder `App.svelte`).
- Produces: CSS custom properties listed below (every later component uses only these). npm scripts `test`, `check`, `dev:mock`, `screenshots`.

- [ ] **Step 1: Add dependencies and scripts**

`npm i @tauri-apps/api@^2 @tauri-apps/plugin-dialog@^2 @tauri-apps/plugin-opener@^2 @fontsource/ibm-plex-sans @fontsource/ibm-plex-mono` and `npm i -D vitest jsdom @testing-library/svelte @testing-library/jest-dom @playwright/test svelte-check`. Scripts:

```json
"test": "vitest run",
"check": "svelte-check --tsconfig ./tsconfig.json",
"dev:mock": "vite --open '/?mock=ready'",
"screenshots": "playwright test -c playwright.config.ts"
```

`vitest.config.ts`: `mergeConfig(resolvedViteConfig, defineConfig({ plugins: [svelteTesting()], test: { environment: 'jsdom', setupFiles: ['./src/test/setup.ts'], include: ['src/**/*.test.ts'] } }))`. If `vite.config.ts` exports a function (the Tauri template does), call it with `{ command: 'serve', mode: 'test' }` first. `setup.ts` imports `@testing-library/jest-dom/vitest` and stubs `window.matchMedia` (returns `matches: false`).

- [ ] **Step 2: Write the failing test** (reads the CSS files as text with `?raw`)

```ts
import tokens from './tokens.css?raw';
import global from './global.css?raw';
test('panel palette tokens', () => {
  expect(tokens).toMatch(/--panel:\s*#E8E9E4/i);
  expect(tokens).toMatch(/--ink:\s*#111111/i);
  expect(tokens).toMatch(/--muted:\s*#555555/i);
  expect(tokens).toMatch(/prefers-color-scheme:\s*dark/);
});
test('no remote fonts, radius or shadows', () => {
  for (const css of [tokens, global]) {
    expect(css).not.toMatch(/https?:\/\//);
    expect(css).not.toMatch(/box-shadow:\s*(?!none)/);
  }
  expect(global).toMatch(/@keyframes eink\{0%\{filter:invert\(1\) contrast\(3\)\}30%\{filter:brightness\(0\)\}55%\{filter:brightness\(3\) grayscale\(1\)\}80%\{filter:none\}\}/);
  expect(global).toMatch(/prefers-reduced-motion:\s*reduce/);
});
```

- [ ] **Step 3: Run it.** `npx vitest run src/styles` → FAIL (files missing).

- [ ] **Step 4: Write `tokens.css` and `global.css`**

Tokens on `:root` (mockup values; sizes scaled from the ~420px-wide mockup preview to the 880px window):

| Token | Light | Dark (`@media (prefers-color-scheme: dark)`) |
|---|---|---|
| `--panel` | `#E8E9E4` | `#111111` |
| `--ink` | `#111111` | `#E8E9E4` |
| `--muted` | `#555555` | `#A0A19C` |
| `--border` | `2px solid var(--ink)` | same |
| `--border-heavy` | `4px solid var(--ink)` | same |
| `--dither-3` | `repeating-conic-gradient(currentColor 0 25%, transparent 0 50%) 0 0/3px 3px` | same |
| `--dither-4` | `repeating-conic-gradient(var(--ink) 0 25%, transparent 0 50%) 0 0/4px 4px` | same |
| `--font-sans` | `"IBM Plex Sans", system-ui, sans-serif` | |
| `--font-mono` | `"IBM Plex Mono", ui-monospace, monospace` | |
| `--fs-display` | `600 32px/1.1 var(--font-sans)`, tracking `-0.02em` (`--track-display`) | |
| `--fs-heading` | `600 28px/1.12 var(--font-sans)`, tracking `-0.01em` | |
| `--fs-body` | `400 16px/1.5 var(--font-sans)` | |
| `--fs-instruction` | `600 22px/1.25 var(--font-sans)` (cable/USB guide panels) | |
| `--fs-label` | `500 12px/1 var(--font-mono)`, `letter-spacing:.12em`, uppercase, `opacity:.55` | |
| `--fs-meta` | `400 13px/1.4 var(--font-mono)`, colour `--muted` | |
| `--fs-chip` | `500 12px/1 var(--font-mono)`, padding `5px 9px` | |
| `--fs-wordmark` | `600 16px/1 var(--font-mono)`, `letter-spacing:.02em` | |
| `--space-1..6` | `4px 8px 12px 16px 24px 32px` | |
| `--gutter` | `28px` (screen padding, mockup `26px` rounded) | |

`global.css`: `*{box-sizing:border-box;margin:0;padding:0;border-radius:0}`, `body{background:var(--panel);color:var(--ink);font:var(--fs-body)}`, `:focus-visible{outline:2px solid var(--ink);outline-offset:3px}`, `.flash{animation:eink .52s steps(1,end)}` plus the `eink` keyframes copied verbatim from the mockup, `::selection{background:var(--ink);color:var(--panel)}`, and `@media (prefers-reduced-motion: reduce){.flash,.marching{animation:none}}`.

`main.ts` imports `@fontsource/ibm-plex-sans/400.css`, `/600.css`, `@fontsource/ibm-plex-mono/400.css`, `/500.css`, `/600.css`, then `tokens.css` and `global.css`.

- [ ] **Step 5: Run it.** `npx vitest run src/styles` → PASS. Also `npm run build`: the built `dist/` contains `.woff2` files, and `grep -r googleapis dist` finds nothing.

- [ ] **Step 6: Commit** `feat(ui): Panel design tokens, local Plex fonts, test tooling`

---

### Task 2: IPC types, Tauri backend and Platform

**Files:**
- Create: `app/src/lib/ipc.ts`, `app/src/lib/platform.ts`, `app/src/lib/legal.ts`, `app/src/lib/links.ts`
- Test: `app/src/lib/ipc.test.ts`

**Interfaces:**
- Produces (in `ipc.ts`): every contract type copied by hand, character for character: `Slot, DeviceStatus, Gate, DeviceInfo, BackupFile, BackupSummary, FlowKind, FlowOptions, StepId, FlowStatus, PromptOption, Prompt, ErrorCode, FlowError, FlowResult, FlowState, Progress, LogLine, AppInfo`. It also adds:

```ts
export type PromptKind = Prompt['kind'];
export interface Settings { hasRescueCable: boolean | null; acceptedRiskVersion: number | null; }
export type Unlisten = () => void;
export interface Backend {
  getAppInfo(): Promise<AppInfo>;                                   // get_app_info
  getDevice(): Promise<DeviceStatus>;                               // get_device
  listBackups(serial?: string): Promise<BackupSummary[]>;           // list_backups {serial}
  startFlow(kind: FlowKind, options: FlowOptions): Promise<string>; // start_flow {kind, options}
  answerPrompt(flowId: string, promptId: string, optionId: string): Promise<void>; // answer_prompt
  cancelFlow(flowId: string): Promise<void>;                        // cancel_flow {flowId}
  getLog(flowId?: string): Promise<string>;                         // get_log {flowId}
  revealPath(path: string): Promise<void>;                          // reveal_path {path}
  getSettings(): Promise<Settings>;                                 // get_settings
  setSettings(settings: Settings): Promise<Settings>;               // set_settings {settings}
  onDeviceStatus(cb: (s: DeviceStatus) => void): Promise<Unlisten>; // device://status
  onFlowState(cb: (s: FlowState) => void): Promise<Unlisten>;       // flow://state
  onFlowProgress(cb: (p: Progress) => void): Promise<Unlisten>;     // flow://progress
  onFlowLog(cb: (l: LogLine) => void): Promise<Unlisten>;           // flow://log
}
export function createTauriBackend(): Backend;
export function toFlowError(e: unknown): FlowError; // passes a FlowError-shaped value through; otherwise {code:"internal", message: COPY fallback "Something went wrong.", detail: String(e), suggestedAction: null}
```

`setSettings` always sends the **full** `Settings` object (the caller reads, merges and writes). The contract doesn't say whether partial objects are accepted, so this works either way.

- Produces (in `platform.ts`):

```ts
export interface Platform {
  pickBootImage(): Promise<string | null>;        // dialog open({multiple:false,directory:false,filters:[{name:'Boot image',extensions:['img']}]})
  openExternal(url: string): Promise<void>;       // plugin-opener openUrl
  onCloseRequested(allow: () => boolean): Promise<Unlisten>; // getCurrentWindow().onCloseRequested; preventDefault when allow() is false
  closeWindow(): Promise<void>;                   // getCurrentWindow().destroy()
  copyText(text: string): Promise<void>;          // navigator.clipboard.writeText
}
export function createTauriPlatform(): Platform;
```

- `legal.ts`: `export const RISK_NOTICE_VERSION = 1; export const RISK_REQUIRED: FlowKind[] = ['root','unroot','rescue','reinstallBootService'];`
- `links.ts`: `REPO='https://github.com/n0dal/inkroot'`, `DISCLAIMER_URL=REPO+'/blob/main/DISCLAIMER.md'`, `NOTICES_URL=REPO+'/blob/main/THIRD_PARTY_NOTICES'`, `BOOT_SERVICE_DOCS_URL=REPO+'/blob/main/boot-service/README.md'`, `newFirmwareIssueUrl(displayId: string, incremental: string): string` → `REPO+'/issues/new?title='+encodeURIComponent('Firmware support: '+displayId+' ('+incremental+')')`, `ACTIVITY_LAUNCHER_FDROID='https://f-droid.org/packages/de.szalkowski.activitylauncher/'`, `ACTIVITY_LAUNCHER_PLAY='https://play.google.com/store/apps/details?id=de.szalkowski.activitylauncher'`.

- [ ] **Step 1: Write the failing test.** Mock `@tauri-apps/api/core` and `@tauri-apps/api/event` with `vi.mock`.

```ts
test('commands use contract names and arg keys', async () => {
  const b = createTauriBackend();
  await b.listBackups('SN1');        expect(invoke).toHaveBeenLastCalledWith('list_backups', { serial: 'SN1' });
  await b.startFlow('root', {});     expect(invoke).toHaveBeenLastCalledWith('start_flow', { kind: 'root', options: {} });
  await b.answerPrompt('f','p','have'); expect(invoke).toHaveBeenLastCalledWith('answer_prompt', { flowId:'f', promptId:'p', optionId:'have' });
  await b.cancelFlow('f');           expect(invoke).toHaveBeenLastCalledWith('cancel_flow', { flowId: 'f' });
  await b.getLog();                  expect(invoke).toHaveBeenLastCalledWith('get_log', {});
  await b.setSettings({ hasRescueCable: true, acceptedRiskVersion: 1 });
  expect(invoke).toHaveBeenLastCalledWith('set_settings', { settings: { hasRescueCable: true, acceptedRiskVersion: 1 } });
  // also: get_app_info, get_device, reveal_path {path}, get_settings with no args
});
test('events use contract names and unwrap payload', async () => {
  const cb = vi.fn(); await createTauriBackend().onFlowState(cb);
  expect(listen).toHaveBeenCalledWith('flow://state', expect.any(Function));
  listenHandlers['flow://state']({ payload: { flowId: 'x' } }); expect(cb).toHaveBeenCalledWith({ flowId: 'x' });
  // same for device://status, flow://progress, flow://log
});
test('toFlowError passes FlowError through and wraps strings', () => {
  const fe = { code:'busy', message:'m', detail:'d', suggestedAction:null };
  expect(toFlowError(fe)).toEqual(fe);
  expect(toFlowError('boom')).toMatchObject({ code: 'internal', detail: 'boom' });
});
```

- [ ] **Step 2: Run it.** `npx vitest run src/lib/ipc.test.ts` → FAIL.
- [ ] **Step 3: Implement** `ipc.ts`, `platform.ts`, `legal.ts`, `links.ts` per the Interfaces. `getLog(undefined)` sends `{}`, not `{flowId: undefined}`.
- [ ] **Step 4: Run it.** → PASS. Then `npm run check` → 0 errors.
- [ ] **Step 5: Commit** `feat(ui): typed IPC layer and platform adapter for the backend contract`

---

### Task 3: Mock backend, fixtures and scenarios

**Files:**
- Create: `app/src/lib/mock/engine.ts`, `fixtures.ts`, `fragments.ts`, `scenarios.ts`, `app/src/lib/backend.ts`
- Test: `app/src/lib/mock/engine.test.ts`

**Interfaces:**
- Consumes: `Backend`, `Platform`, contract types (Task 2), `RISK_NOTICE_VERSION`, `RISK_REQUIRED`.
- Produces:

```ts
export type Beat =
  | { wait: number }                                   // ms, divided by speed
  | { state: Partial<FlowState> }                      // merged into the snapshot, then emitted on flow://state
  | { progress: Omit<Progress, 'flowId'> }
  | { log: string; stream?: LogLine['stream'] }        // default 'stdout'
  | { device: DeviceStatus }                           // emitted on device://status
  | { prompt: Omit<Prompt, 'id'>; branches: Record<string, Beat[]> } // status→waitingForUser; pause; on answer run branches[optionId] (missing branch = continue)
  | { fail: Omit<FlowError, 'detail'> & { detail?: string } }        // status→failed, ends script
  | { done: FlowResult };                                            // status→done, step 'done', ends script
export interface Scenario {
  name: string; description: string;
  appInfo?: Partial<AppInfo>;               // merged over { version:'0.1.0-mock', demo:true, os:'macos', backupRoot:'/Users/you/Documents/Inkroot' }
  device: DeviceStatus;
  deviceScript?: Beat[];                    // runs on connect (e.g. none → unauthorized → adb)
  backups: BackupSummary[];
  settings?: Partial<Settings>;             // default { hasRescueCable:null, acceptedRiskVersion:null }
  flows: Partial<Record<FlowKind, Beat[]>>;
  startErrors?: Partial<Record<FlowKind, FlowError>>;
  pickFile?: string | null;                 // what Platform.pickBootImage returns; default '/Users/you/Downloads/boot_b.stock.img'
  frozen?: { flow: FlowState; progress?: Progress; logs?: string[] }; // emitted once at connect, no script (screenshots)
}
export function createMockBackend(s: Scenario, opts?: { speed?: number }): Backend;
export function createMockPlatform(s: Scenario): Platform; // openExternal → window.open(url,'_blank'); onCloseRequested → 'beforeunload'; closeWindow no-op; copyText → clipboard, resolves even if clipboard is missing
export const SCENARIOS: Record<string, Scenario>;
// backend.ts
export function selectBackend(loc: Location, w: Window): { backend: Backend; platform: Platform; mock: string | null };
```

`selectBackend` picks mock when `?mock=` is present or `'__TAURI_INTERNALS__' in window` is false. The scenario is `SCENARIOS[name] ?? SCENARIOS.ready` (unknown name → `console.warn`), and speed is `?speed=` (default 1).

Engine rules (the same rules the real backend has): `startFlow` rejects with `startErrors[kind]` if one is set. It also rejects `{code:'busy'}` while a flow is non-terminal, and `{code:'internal', message:'Please read and accept the risk notice first.'}` for `root|unroot|rescue` unless `settings.acceptedRiskVersion === RISK_NOTICE_VERSION`. Otherwise it creates `flowId = 'mock-' + n`, emits an initial state (`status:'running'`, `stepIndex:1`, `stepCount` from `FLOW_STEP_COUNT[kind]` = backup 4, importBackup 3, root 4, rescue 4, unroot 3, reinstallBootService 2) and runs the script with `setTimeout`. `answerPrompt` ignores a stale `promptId`. `cancelFlow` rejects while `writing` and otherwise emits `status:'cancelled'`. `setSettings` replaces and returns. `getLog` joins the emitted log lines. `listBackups(serial)` filters by serial and sorts newest first. Every `state` beat recomputes `prompt/error/result` so they stay null unless status matches.

`fixtures.ts`:
- `NOTE_MAX: DeviceInfo = { serial:'NM24B0A1C3', model:'NoteMax', profileId:'note-max', displayName:'Note Max', buildId:'TKQ1.230721.002', displayId:'TKQ1.230721.002', incremental:'226', firmwareLabel:'4.2', slot:'b', battery:82, rooted:false, magiskVersion:null, bootServiceRunning:false }`
- `NOTE_MAX_ROOTED = {...NOTE_MAX, rooted:true, magiskVersion:'30.7', bootServiceRunning:true}`
- `STOCK_BACKUP: BackupSummary` with id `'NM24B0A1C3/2026-10-25_1042'`, `slot:'b'`, `stock:true`, `verified:true`, `source:'inkroot'`, `path:'/Users/you/Documents/Inkroot/NM24B0A1C3/2026-10-25_1042'`, and files `boot_a.img` (sha `1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09`), `boot_b.img` (sha `2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc`), both 100663296 bytes, and `vbmeta_a.img`/`vbmeta_b.img` at 65536 bytes.
- `ROOTED_BACKUP` (not stock), `IMPORTED_RESCUE_ONLY` (`source:'imported'`, `stock:false`).

`fragments.ts` holds Beat[] builders shared by scenarios: `checkDevice(idx)`, `safetyCopy(idx, {alreadyHave})`, `enterEdl(idx)` (device → `{kind:'edl'}`), `readPartitions(idx)` (progress across 4 partitions, about 400 KB-ish beats, totals in bytes), `writeBoot(idx, slot)` (`state {step:'write', writing:true}`, progress 0→100663296 in 12 beats labelled `boot_b`, then `{writing:false}`), `verifyWrite`, `reset`, `waitBoot` (device → adb), `cableCheckPrompt()` (with the `continueWithout` → `cableCheckConfirm` prompt branch, whose `back` branch re-runs `cableCheckPrompt`), plus realistic log lines (e.g. `edl --memory=ufs w boot_b patched-boot.img`).

`SCENARIOS` (each name is also a `?mock=` value):

| name | device | notes |
|---|---|---|
| `ready` | adb `NOTE_MAX` | `[STOCK_BACKUP]`; happy scripts for all six kinds |
| `no-backup` | adb `NOTE_MAX` | no backups; root runs the full safety copy |
| `accepted` | as `ready` | settings `{hasRescueCable:true, acceptedRiskVersion:1}` (`ready` itself has `acceptedRiskVersion:1`; `no-backup` and every gate scenario have it `null`) |
| `no-device` | none | |
| `plug-in` | none | deviceScript: wait 4000 → unauthorized → wait 4000 → adb `NOTE_MAX` |
| `unauthorized` | unauthorized | |
| `edl` | edl | rescue happy |
| `unknown-model` | adb `{...NOTE_MAX, model:'TabUltraCPro', profileId:null, displayName:'TabUltraCPro'}`, gate `unknownModel` | |
| `unknown-firmware` | adb `{...NOTE_MAX, displayId:'TKQ1.230721.003', incremental:'240', firmwareLabel:'TKQ1.230721.003'}`, gate `unknownFirmware` | |
| `low-battery` | adb `{...NOTE_MAX, battery:18}`, gate `lowBattery` | |
| `rooted` | adb `NOTE_MAX_ROOTED` | `[STOCK_BACKUP]` |
| `rooted-no-stock` | adb `NOTE_MAX_ROOTED` | `[ROOTED_BACKUP]` |
| `root-hash-mismatch` | ready | root write → `retryWrite` prompt; `retry` → write again → success |
| `root-boot-timeout` | ready | waitBoot → `keepWaitingOrRescue`; `keepWaiting` → prompt again after 6 s |
| `root-patch-failed` | ready | fail `patchFailed`, suggestedAction `retry` |
| `root-busy` | ready | `startErrors.root = {code:'busy', …}` |
| `backup-edl-timeout` | ready | backup enterEdl → `edlNotFound`; `retry` → fail `edlTimeout` |
| `import-not-stock` | ready | importBackup fail `notStock` |
| `import-bad-image` | ready | importBackup fail `badImage` |
| `import-rescue-only` | none | importBackup done with `IMPORTED_RESCUE_ONLY` |
| `rescue-edl-not-found` | none | rescue waitEdl → `edlNotFound` after 6 s; `retry` → found |
| `rescue-back` / `rescue-not-seen` / `rescue-home-screen` / `rescue-back-in-edl` | edl | rescue ends with `rescueOutcome` tabletBack (device, not rooted) / notSeen / homeScreenSeen (after the `tabletStarting` answer) / backInEdl |
| `shot-rescue-waiting` / `shot-rescue-back` / `shot-rescue-not-seen` / `shot-rescue-back-in-edl` | — | frozen rescue step 4 and done variants with a long log (e2e/layout.spec.ts) |
| `reinstall-hook-absent` | rooted | reinstall checkDevice → state `{kind:'root', stepIndex:2, stepCount:4}` → continues the root script |
| `windows-driver` | edl, os `windows` | rescue starts with `installDriver` prompt |
| `linux-udev` | edl, os `linux` | rescue starts with `installUdev` prompt |
| `shot-root-writing` | adb `NOTE_MAX` | frozen root `write`, stepIndex 3/4, writing true, progress 61865984/100663296 `boot_b` |
| `shot-rescue-cable` | none | frozen rescue `cableGuide`, stepIndex 2/4 |
| `shot-root-done` | adb `NOTE_MAX_ROOTED` | frozen root done, result `{backup: STOCK_BACKUP, magiskVersion:'30.7'}` |

Each `shot-*` and `ready` scenario uses `settings {hasRescueCable:null, acceptedRiskVersion:1}` so screenshots skip the risk screen.

- [ ] **Step 1: Write the failing tests** (fake timers, `speed: 1`)

```ts
test('happy root emits ordered steps then done', async () => { /* ready + accepted settings; collect flow://state; advance; answer cableCheck 'have'; expect steps include ['checkDevice','patch','cableCheck','write','verifyWrite','reset','waitBoot','verifyRoot','done'] in order and last status 'done' with result.magiskVersion '30.7' */ });
test('risk not accepted rejects root', async () => { await expect(createMockBackend(SCENARIOS['no-backup']).startFlow('root', {})).rejects.toMatchObject({ code:'internal', message:'Please read and accept the risk notice first.' }); });
test('busy while a flow runs', /* start backup, start backup again → rejects {code:'busy'} */);
test('cancel rejected while writing', /* shot-root-writing frozen: cancelFlow rejects */);
test('stale promptId ignored', /* answerPrompt with wrong id → state unchanged */);
test('every prompt kind is reachable in some scenario', () => { /* walk all Beat trees; set of prompt.kind equals ALL_PROMPT_KINDS */ });
test('every non-shot scenario starts', /* for each scenario: connect, getDevice equals scenario.device */);
test('selectBackend uses mock with ?mock and falls back to ready', /* fake location '?mock=nope' → mock 'ready' */);
```

- [ ] **Step 2: Run it.** `npx vitest run src/lib/mock` → FAIL.
- [ ] **Step 3: Implement** the engine, fixtures, fragments and scenarios. The engine walks the Beat list with a cursor and a stack, so branches can be nested. Create `src/lib/flows.ts` here with `ALL_PROMPT_KINDS` and `FLOW_STEP_COUNT` (Task 5 adds the rest).
- [ ] **Step 4: Run it.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): scripted mock backend with scenarios for every flow and failure`

---

### Task 4: AppState, screen resolution, job start and close guard logic

**Files:**
- Create: `app/src/lib/screen.ts`, `app/src/lib/state.svelte.ts`, `app/src/lib/context.ts`, `app/src/lib/format.ts`, `app/src/test/render.ts`
- Test: `app/src/lib/screen.test.ts`, `app/src/lib/state.test.ts`, `app/src/lib/format.test.ts`

**Interfaces:**
- Consumes: `Backend`, `Platform`, `toFlowError`, `RISK_NOTICE_VERSION`, `RISK_REQUIRED`, `createMockBackend`, `SCENARIOS`.
- Produces:

```ts
// screen.ts
export type Route =
  | { screen: 'home' } | { screen: 'about' } | { screen: 'backups' } | { screen: 'usbGuide' }
  | { screen: 'alreadyRooted' }
  | { screen: 'rescuePick'; preselect?: { backupId?: string; slot?: Slot } }
  | { screen: 'risk'; then: { kind: FlowKind; options: FlowOptions } };
export type ScreenId = 'flow' | 'risk' | 'about' | 'backups' | 'rescuePick' | 'usbGuide' | 'alreadyRooted' | 'home'
  | 'gate-none' | 'gate-unauthorized' | 'gate-edl' | 'gate-unknownModel' | 'gate-unknownFirmware' | 'gate-lowBattery';
export function resolveScreen(i: { device: DeviceStatus; flow: FlowState | null; route: Route }): ScreenId;
```

Priority, first match wins:
1. `flow !== null` → `flow` (the flow stays on screen even when terminal, until dismissed).
2. route `risk|about|backups|rescuePick` → that screen.
3. route `usbGuide` and device kind is `none|unauthorized` → `usbGuide`. Any other kind falls through: this is the auto-advance.
4. device `none|unauthorized|edl` → `gate-<kind>`.
5. device `adb` with a gate → `gate-<gate>`.
6. route `alreadyRooted` and `info.rooted` → `alreadyRooted`.
7. otherwise → `home`.

```ts
// state.svelte.ts
export class AppState {
  device = $state<DeviceStatus>({ kind: 'none' });
  flow = $state<FlowState | null>(null);
  progress = $state<Progress | null>(null);
  logs = $state<LogLine[]>([]);                // capped at 5000, oldest dropped
  settings = $state<Settings>({ hasRescueCable: null, acceptedRiskVersion: null });
  info = $state<AppInfo | null>(null);
  backups = $state<BackupSummary[]>([]);
  route = $state<Route>({ screen: 'home' });
  closeBlocked = $state(false);
  inFlight = $state(false);                    // a start/answer/cancel call is pending
  screen = $derived(resolveScreen({ device: this.device, flow: this.flow, route: this.route }));
  get deviceInfo(): DeviceInfo | null;
  get stockBackup(): BackupSummary | null;     // newest verified && stock for deviceInfo.serial
  constructor(backend: Backend, platform: Platform);
  connect(): Promise<Unlisten>;                // load info/device/settings/backups, subscribe 4 events, install close guard
  navigate(route: Route): void;
  startJob(kind: FlowKind, options?: FlowOptions): Promise<void>;
  acceptRisk(): Promise<void>;                 // writes acceptedRiskVersion, then starts route.then
  answer(optionId: string): Promise<void>;
  cancel(): Promise<void>;
  dismissFlow(): void;                         // flow=null, progress=null, records flowId as dismissed, route→afterEnd ?? home, refresh backups
  setHasRescueCable(v: boolean): Promise<void>;
  importBackup(): Promise<void>;               // platform.pickBootImage(); if a path, startJob('importBackup', { imagePath }); cancelled dialog → nothing
  copyLog(): Promise<boolean>;
  confirmClose(): Promise<void>;               // platform.closeWindow()
}
// context.ts
export function setApp(app: AppState): void; export function getApp(): AppState;
// format.ts
export function formatBytes(n: number): string;      // 1024-based, one decimal, "MB"/"KB"/"GB": 100663296 → "96.0 MB"
export function shortSha(sha: string): string;       // first 4 + "…" + last 3
export function backupLine(b: BackupSummary | null): string;
export function formatChip(s: DeviceStatus): string;
export function revealLabel(os: AppInfo['os']): string; // "Show in Finder" | "Show in Explorer" | "Show in folder"
// test/render.ts
export function renderWithApp<C>(component: C, opts?: { scenario?: string; props?: object; app?: AppState }): Promise<RenderResult & { app: AppState }>;
```

Behaviour that tests pin down:
- `startJob`: if `kind ∈ RISK_REQUIRED` and `settings.acceptedRiskVersion !== RISK_NOTICE_VERSION`, set `route = {screen:'risk', then:{kind, options}}` and return without calling the backend. Otherwise `inFlight = true` and call `backend.startFlow`. On rejection, set `flow` to a synthetic failed state `{flowId:'local-'+n, kind, step:'checkDevice', stepIndex:1, stepCount:1, status:'failed', error: toFlowError(e), prompt:null, result:null, writing:false}`. Remember `lastStart = {kind, options, serial}` for "Try again".
- **reinstallBootService needs the acknowledgement too** (my call: the contract lets it turn into a root write mid-flow).
- `flow://state`: ignore if `flowId` was dismissed. Otherwise replace `flow`, clear `inFlight`, and reset `progress` when `step` changes. When the status becomes terminal, refresh backups. If `afterEnd` is set (the `goToRescue` path), call `dismissFlow()`.
- `flow://progress`: ignore unless `p.flowId === flow?.flowId`.
- `answer(optionId)`: does nothing if `inFlight` or there is no prompt; otherwise `inFlight = true`. Special cases:
  - `cableCheck/have` also calls `setHasRescueCable(true)`.
  - `cableCheckConfirm/confirmContinue` calls `setHasRescueCable(false)`.
  - `keepWaitingOrRescue/goToRescue` sets `afterEnd = {screen:'rescuePick', preselect:{backupId: lastStart.options.backupId ?? stockBackupFor(lastStart.serial)?.id, slot: thatBackup?.slot}}`, answers, then calls `cancelFlow` (ignoring rejection). My call: the contract doesn't say the backend ends the flow on `goToRescue`.
- `cancel()`: does nothing while `flow.writing`.
- Close guard: `platform.onCloseRequested(() => { if (this.flow?.writing) { this.closeBlocked = true; return false; } return true; })`.
- `formatChip`:
  - adb → `● ${displayName.toUpperCase()} / FW ${firmwareLabel} / SLOT_${slot.toUpperCase()}`, plus ` / ROOT` when rooted. Unknown model → `● ${model.toUpperCase()} / UNSUPPORTED`.
  - none → `○ NO TABLET`; unauthorized → `◐ WAITING FOR ALLOW`; edl → `● RESCUE MODE`.
- `backupLine(b)` → `backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc` (uses `boot_${b.slot}.img`; `✓` only when verified && stock, else `backup · not original · …`). For `null` it returns `no backup yet · we'll make one first`.

- [ ] **Step 1: Write the failing tests**

```ts
// screen.test.ts
const adb = (o = {}, gate = null) => ({ kind: 'adb', info: { ...NOTE_MAX, ...o }, gate });
test.each([
  [{ kind: 'none' }, 'gate-none'], [{ kind: 'unauthorized' }, 'gate-unauthorized'], [{ kind: 'edl' }, 'gate-edl'],
  [adb({}, 'unknownModel'), 'gate-unknownModel'], [adb({}, 'unknownFirmware'), 'gate-unknownFirmware'],
  [adb({}, 'lowBattery'), 'gate-lowBattery'], [adb(), 'home'],
])('device %o → %s', (device, id) => expect(resolveScreen({ device, flow: null, route: { screen: 'home' } })).toBe(id));
test('flow outranks device gates', () => expect(resolveScreen({ device: { kind: 'none' }, flow: runningFlow, route: { screen: 'about' } })).toBe('flow'));
test('usbGuide auto-advances when adb appears', () => {
  expect(resolveScreen({ device: { kind: 'unauthorized' }, flow: null, route: { screen: 'usbGuide' } })).toBe('usbGuide');
  expect(resolveScreen({ device: adb(), flow: null, route: { screen: 'usbGuide' } })).toBe('home');
});
test('alreadyRooted only when rooted', /* route alreadyRooted + adb({rooted:false}) → 'home'; rooted → 'alreadyRooted' */);
test('rescuePick reachable with no device', /* device none + route rescuePick → 'rescuePick' */);

// state.test.ts (mock backend, fake timers)
test('root without acceptance routes to risk and does not call backend', /* spy startFlow not called; app.route.screen === 'risk' */);
test('acceptRisk writes version 1 then starts the pending flow', /* setSettings called with {hasRescueCable:null, acceptedRiskVersion:1}; flow.kind==='root' */);
test('start rejection becomes a failed flow', /* scenario root-busy → app.flow.status==='failed', error.code==='busy', app.screen==='flow' */);
test('ignores events for dismissed flows', /* dismissFlow, then emit flow://state same flowId → app.flow===null */);
test('ignores progress for other flowId', /* emit progress flowId 'other' → app.progress===null */);
test('cancel is a no-op while writing', /* shot-root-writing; spy cancelFlow not called */);
test('close is blocked while writing and allowed otherwise', /* capture allow fn from platform.onCloseRequested; writing → false and closeBlocked true; done → true */);
test('goToRescue routes to rescuePick with backup and slot preselected', /* root-boot-timeout → answer goToRescue → route {screen:'rescuePick', preselect:{backupId: STOCK_BACKUP.id, slot:'b'}} */);
test('cable answers persist hasRescueCable', /* have → settings.hasRescueCable true */);
test('importBackup: cancelled dialog starts nothing', /* pickFile null → startFlow not called */);

// format.test.ts
expect(formatBytes(100663296)).toBe('96.0 MB');
expect(shortSha('2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc')).toBe('2f05…8fc');
expect(backupLine(STOCK_BACKUP)).toBe('backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc');
expect(formatChip({ kind: 'adb', info: NOTE_MAX, gate: null })).toBe('● NOTE MAX / FW 4.2 / SLOT_B');
```

- [ ] **Step 2: Run them.** `npx vitest run src/lib` → FAIL.
- [ ] **Step 3: Implement** `screen.ts`, `format.ts`, `state.svelte.ts`, `context.ts`, `test/render.ts`. `renderWithApp` builds the mock backend and platform, `await app.connect()`, and renders with context `new Map([['inkroot', app]])`.
- [ ] **Step 4: Run them.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): app state, screen resolution, risk-gated job start, close guard`

---

### Task 5: Flow tables and the copy module

**Files:**
- Create/complete: `app/src/lib/flows.ts`, `app/src/lib/copy.ts`
- Test: `app/src/lib/copy.test.ts`

**Interfaces:**
- Produces:

```ts
// flows.ts
export const ALL_STEP_IDS: StepId[];          // all 20 literals from the contract union
export const ALL_PROMPT_KINDS: PromptKind[];  // all 8
export const ALL_ERROR_CODES: ErrorCode[];    // all 19
export const FLOW_KINDS: FlowKind[];          // all 6
export const FLOW_STEP_COUNT: Record<FlowKind, number> = { backup: 4, importBackup: 3, root: 4, rescue: 4, unroot: 3, reinstallBootService: 2 };
export const FLOW_STEPS: Record<FlowKind, StepId[]> = {
  backup: ['checkDevice','enterEdl','readPartitions','verifyBackup','rebootToSystem'],
  importBackup: ['pickImage','stockCheck','chooseSlot','saveImport'],
  root: ['checkDevice','enterEdl','readPartitions','verifyBackup','rebootToSystem','patch','cableCheck','write','verifyWrite','reset','waitBoot','verifyRoot'],
  rescue: ['pickImage','chooseSlot','cableGuide','waitEdl','write','verifyWrite','reset','waitBoot'],
  unroot: ['checkDevice','enterEdl','write','verifyWrite','reset','waitBoot'],
  reinstallBootService: ['checkDevice','pushScripts','waitBoot','verifyRoot'],
}; // 'done' is rendered by DoneView for every kind
// copy.ts
export interface StepCopy { heading: string; body: string; tech?: string }    // tech → Details panel only
export interface PromptCopy { label: string; heading: string; body: string; options: Record<string, string>; bullets?: string[] }
export const GROUP_LABELS: Record<FlowKind, string[]>;           // index = stepIndex-1
export const STEP_COPY: Record<FlowKind, Partial<Record<StepId, StepCopy>>>;
export const FALLBACK_STEP: StepCopy;                            // { heading:'Working on it', body:'This can take a minute. Keep your tablet plugged in.' }
export const PROMPT_COPY: Record<PromptKind, PromptCopy>;        // edlNotFound has rescue/other variants: PROMPT_COPY.edlNotFound + EDL_NOT_FOUND_AUTO
export const ERROR_HEADINGS: Record<ErrorCode, string>;
export const ACTION_LABELS: Record<NonNullable<FlowError['suggestedAction']>, string>;
export const DONE_COPY: Record<FlowKind, StepCopy & { checks?: string[] }>;
export const GATE_COPY, USB_GUIDE, CABLE_GUIDE, CABLE_BUY, RISK_COPY, HOME_COPY, ROOTED_COPY, BACKUPS_COPY, RESCUE_PICK_COPY, ABOUT_COPY, CLOSE_GUARD_COPY, COMMON;
export function fill(t: string, vars: Record<string, string | number>): string; // replaces {name}; leaves unknown keys visible
export function stepLabel(kind: FlowKind, stepIndex: number, stepCount: number): string; // "STEP 2 OF 4 · SAFETY COPY"
```

The tables below are the **exact copy**. `{x}` marks an interpolation. Labels are written in sentence case here and rendered uppercase by CSS, except where the text itself is mono/uppercase.

**GROUP_LABELS**

| kind | 1 | 2 | 3 | 4 |
|---|---|---|---|---|
| backup | Check | Get ready | Copy | Verify |
| importBackup | Open | Check | Save | |
| root | Check | Safety copy | Rooting | Restart |
| rescue | Your file | Cable | Writing | Starting up |
| unroot | Check | Restoring | Restart | |
| reinstallBootService | Check | Reinstall | | |

Done label `Done`; failed label `Stopped`. Out-of-range index → `STEP {i} OF {n}`.

**STEP_COPY** (shared rows are defined once and reused across kinds)

| kind · step | heading | body | tech (Details only) |
|---|---|---|---|
| * · checkDevice | Checking your tablet | Making sure it's a Note Max on supported software. | adb getprop: model, build id, fingerprint, slot, battery |
| backup · enterEdl | Getting your tablet ready | Your tablet will restart into a special mode. The screen may go blank. That's normal. | adb reboot edl → waiting for USB 05c6:9008 |
| backup · readPartitions | Making a safety copy | We're copying your tablet's startup software to this computer, so it can always be put back. Keep it plugged in. | edl --memory=ufs r boot_a, boot_b, vbmeta_a, vbmeta_b |
| backup · verifyBackup | Checking the copy | Reading it a second time to make sure the copy is perfect. | second read of the active boot slot, SHA-256 compare, ANDROID! magic |
| * · rebootToSystem | Restarting your tablet | It'll be back in a minute or two. | edl reset → waiting for adb |
| root · enterEdl | Getting your tablet ready | Your tablet will restart into a special mode. The screen may stay blank. That's normal. Don't unplug it. | adb reboot edl → 05c6:9008 |
| root · readPartitions | Saving your lifeline | Before we change anything, we copy your tablet's original startup software so it can always be put back. | edl --memory=ufs r boot_a/boot_b/vbmeta_a/vbmeta_b |
| root · verifyBackup | Checking the copy | Making sure your safety copy is perfect before we go on. | stock backup verified (SHA-256 re-read) |
| root · patch | Preparing root | Your tablet is building its rooted startup software from your safety copy. Nothing is changed yet. | Magisk 30.7 boot_patch.sh, KEEPVERITY=true KEEPFORCEENCRYPT=true, + overlay.d/init.bootsvc.rc |
| root · cableCheck | Before we write | One quick question first. | |
| root/rescue/unroot · write | Writing to your tablet | The screen may stay blank. That's normal. Don't unplug it. | edl --memory=ufs w boot_{slot} |
| unroot · write | Putting the original back | The screen may stay blank. That's normal. Don't unplug it. | edl --memory=ufs w boot_{slot} (stock) |
| * · verifyWrite | Double-checking | Reading it back to make sure every byte landed. Still don't unplug it. | read-back SHA-256 compare |
| * · reset | Restarting your tablet | Almost there. | edl reset |
| root · waitBoot | Waiting for your tablet to start | The first start after rooting can take a couple of minutes. The ONYX logo may stay up for a while. That's normal. | waiting for adb (up to ~3 min) |
| unroot · waitBoot | Waiting for your tablet to start | Then we'll check that root is gone. | waiting for adb; expect magisk -c to fail |
| reinstallBootService · waitBoot | Restarting your tablet | The boot service starts on its own when your tablet comes back. | adb reboot; waiting for adb |
| root/reinstall · verifyRoot | Checking root works | Making sure Magisk and the boot service are both running. | /debug_ramdisk/magisk -c = 30.7; bootsvc.log uid=0 u:r:magisk:s0 |
| importBackup · pickImage | Opening your file | Checking that it's a startup image from a Note Max. | ANDROID! magic, size 100,663,296 bytes |
| importBackup · stockCheck | Checking it's original | We only use untouched, original copies for rooting. This takes a few seconds. | known stock SHA-256 list, else magiskboot cpio test |
| importBackup · chooseSlot | Which slot is it from? | One quick question. | |
| importBackup · saveImport | Saving it to your library | Adding it to your safety copies. | manifest.json source: imported |
| rescue · pickImage | Checking your file | Making sure it's a good startup image for your tablet. | ANDROID! magic, size check |
| rescue · chooseSlot | Which slot should we fix? | One quick question. | |
| rescue · cableGuide / waitEdl | (rendered by CableGuide, see CABLE_GUIDE) | | waiting for USB 05c6:9008 |
| rescue · waitBoot (RescueWait) | Written and checked ✓ | Your tablet is starting up. This can take a few minutes. Checklist `RESCUE_WAIT`: "✓ Written to your tablet" · "✓ Checked — it matches your safety copy" · "… Waiting for your tablet to start (up to 5 min)"; button "Done — I can see my home screen" (answers `tabletStarting` → homeScreen) | watching adb devices quietly (up to 5 min); 9008 again = back in rescue mode |
| reinstallBootService · pushScripts | Reinstalling the boot service | Copying the boot service back onto your tablet. Your startup software isn't touched. | push bootsvc.sh + service.sh → /data/local/tmp |

**DONE_COPY**

| kind | heading | body | checks |
|---|---|---|---|
| backup | Your safety copy is saved. | It's in {path}. Keep a second copy somewhere safe, like a USB stick. | Startup software copied · Copy checked twice |
| importBackup | Your backup is imported. | Inkroot can now use it to root, rescue or restore your tablet. | |
| importBackup (result.backup.stock false) `DONE_IMPORT_RESCUE_ONLY` | Saved for rescue only. | We couldn't confirm this copy is original, so it can rescue a stuck tablet but can't be used for rooting. | |
| root | Your Note Max is rooted. | Magisk {magiskVersion} is running, and so is the boot service. Your backup is in Documents/Inkroot. Keep it. | Magisk daemon alive · Boot service running as root · Backup kept safe |
| root tip `ROOT_TIP` | One more thing | Onyx puts apps it thinks are idle to sleep, which can stop root apps from working. On your tablet, open Settings → Battery → App freeze, turn off "Freeze new apps", and allow your apps to start automatically. | |
| root link `ROOT_MORE` | What can I do with root? → | (opens BOOT_SERVICE_DOCS_URL) | |
| rescue · tabletBack `RESCUE_DONE` | Your tablet is back ✓ | We wrote your safety copy, checked it, and your tablet has started up again. + "It's rooted." / "It's not rooted." when `result.device` is known | |
| rescue · homeScreenSeen | Your tablet is back ✓ | We wrote your safety copy, checked it, and you've seen your tablet start up again. | |
| rescue · notSeen (also `DONE_COPY.rescue`) | Written and checked ✓ | Your tablet should be starting up now. Once you see its home screen, you're done. If it's still on the ONYX logo after 5 minutes, hold the power button for 10 seconds, or run Rescue again. Note: "Inkroot can't see your tablet when USB debugging is off, which is common after a rescue. That's okay." | |
| rescue · backInEdl | Your tablet went back to rescue mode | We wrote your safety copy and checked it, but your tablet restarted into rescue mode instead of starting up. Hold the power button for 10 seconds to restart it. If it lands in rescue mode again, run Rescue again with a different saved copy. Button: "Run Rescue again" | |
| rescue live `RESCUE_BACK` (notSeen, then the poller sees it) | It's back ✓ Your tablet started. | | |
| unroot | Your Note Max is back to stock. | Root is removed and the original startup software is back. Your safety copies are still in Documents/Inkroot. | Original startup software restored · Root removed |
| reinstallBootService | The boot service is running again. | It starts as root every time your tablet starts. | Boot service running as root |

**PROMPT_COPY**

| kind | label | heading | body / bullets | options (id → label) |
|---|---|---|---|---|
| cableCheck | Before we write | Do you have a rescue cable? | If anything goes wrong, getting back needs a Qualcomm deep-flash cable and a computer with a USB-A port. Do you have one? | have → Yes, I have one · whereToGet → Show me where to get one · continueWithout → Continue without one |
| cableCheckConfirm | Are you sure? | Continue without a rescue cable? | Rooting almost always goes fine. If it doesn't, your tablet could stay stuck until you get a cable. Your safety copy will be ready either way. Tick box: "I understand my tablet could be stuck until I get a cable." | confirmContinue → Continue without a cable · back → Go back |
| chooseSlot | One question | Which slot is this copy from? | Your tablet keeps two copies of its startup software, called A and B. {hint} where hint = "The file name says {S}, so we've picked that." / "Your tablet is using {S} right now." / "" | a → Slot A · b → Slot B |
| keepWaitingOrRescue | Still waiting | Your tablet hasn't restarted yet | That's okay. The first start sometimes takes longer. If it's still on the ONYX logo after a few more minutes, Rescue can put your safety copy back. | keepWaiting → Keep waiting · goToRescue → Go to Rescue |
| installDriver | One-time setup | One quick setup step | Windows needs a small driver to talk to your tablet in rescue mode. Windows will ask for permission once. | install → Install driver · cancel → Not now |
| installUdev | One-time setup | One quick setup step | Linux needs permission to talk to your tablet in rescue mode. You'll be asked for your password once. Afterwards, unplug the tablet and plug it back in. | install → Allow access · cancel → Not now |
| edlNotFound (rescue) | Not found yet | We can't see your tablet yet | bullets: "Check it's the deep-flash cable, not a normal one." · "Use a USB-A port on the computer itself, not an adapter or hub." · "Try again and keep holding the cable button a bit longer, about 15 seconds after the screen goes blank." · "A black screen is normal. Your tablet isn't broken." | retry → Try again · cancel → Stop |
| edlNotFound (other kinds) `EDL_NOT_FOUND_AUTO` | Not found yet | Your tablet didn't switch modes in time | Check the cable is firmly plugged in at both ends, then try again. Nothing has been changed on your tablet. | retry → Try again · cancel → Stop |
| retryWrite | Didn't check out | That write didn't check out | Nothing is broken. The copy on your tablet didn't match, so we stopped before restarting it. Unplug the cable, plug it back in firmly, then try again. | retry → Try again · cancel → Stop |

`CABLE_BUY` (the `whereToGet` panel): label `Rescue cable`, heading `Getting a rescue cable`, body `Search for "Qualcomm deep flash cable" or "EDL cable". It's a USB cable with a small button on it, and it costs a few dollars. A normal cable won't work.`, buttons `Copy search terms` (copies `Qualcomm deep flash cable`) and `Back`.

**ERROR_HEADINGS** (the body is `FlowError.message` as given)

noDevice `We lost your tablet` · unauthorized `Tap Allow on your tablet` · unknownModel `This tablet isn't supported` · unknownFirmware `This software version isn't supported yet` · lowBattery `Charge your tablet first` · alreadyRooted `Your tablet is already rooted` · notRooted `Your tablet isn't rooted` · noStockBackup `We need an original safety copy` · badImage `That file can't be used` · notStock `That copy isn't original` · edlTimeout `Your tablet didn't switch modes` · hashMismatch `That write didn't check out` · patchFailed `Preparing root didn't work` · bootTimeout `Your tablet hasn't restarted yet` · verifyFailed `Root didn't start` · toolMissing `Inkroot is missing a part` · driverMissing `A driver is missing` · busy `Inkroot is busy with another job` · internal `Something went wrong`.

**ACTION_LABELS**: retry `Try again` · goToRescue `Go to Rescue` · reconnect `I've reconnected it` · chargeDevice `OK, I'll charge it` · importBackup `Import a backup`. The secondary action is always `Back to start`. A failed flow also shows `COMMON.failedFoot`: `The full details are in Details below if you want to report this.`

**GATE_COPY**

| gate | label | heading | body | actions |
|---|---|---|---|---|
| none | Connect | Plug in your Note Max | Use a normal USB-C cable. Then turn on USB debugging on your tablet. It's a hidden setting, so we'll walk you through it. | primary `Show me how` → usbGuide · link `Tablet stuck on the ONYX logo? Rescue it →` → rescuePick · meta `waiting for tablet…` |
| unauthorized | Almost there | Tap Allow on your tablet | Your tablet is asking "Allow USB debugging?". Tick "Always allow from this computer", then tap Allow. | meta `Don't see it? Unplug the cable and plug it back in.` · link `Show me how` → usbGuide |
| edl | Rescue mode | Your tablet is in rescue mode | It's connected in a special mode used for repairs. If it's stuck, Rescue can put a good copy back. | primary `Rescue my tablet` → rescuePick |
| unknownModel | Not supported | Inkroot only supports the BOOX Note Max right now. | We found a {model}. Other tablets need their own testing before Inkroot can work with them safely. Nothing on your tablet was changed. | |
| unknownFirmware | Not supported yet | This software version isn't supported yet | Your Note Max is running a software version Inkroot hasn't been tested on. Using it could leave your tablet unable to start, so we've stopped here. Nothing on your tablet was changed. | secondary `Ask for support on GitHub` → newFirmwareIssueUrl · meta `fw {displayId} · build {incremental}` |
| lowBattery | Charge first | Charge to at least 30 % first | Your tablet is at {battery} %. Rooting takes a while, and your tablet mustn't run out of power halfway. This screen moves on by itself once it's charged enough. | meta `battery {battery} % · need 30 %` |

**USB_GUIDE** (illustrated, one instruction per panel; label `Turn on USB debugging · {i} of {n}`)

| id | heading | body | buttons |
|---|---|---|---|
| a1 | Open Settings, then Apps | On your tablet, open Settings and tap Apps. | Next |
| a2 | Tap ⋮ in the top-right corner | It's the menu with three dots. | Next |
| a3 | Turn on USB Debug Mode | If you see it, switch it on. Then plug your tablet into this computer. | primary `It's on` → allow · secondary `I can't find it` → b1 |
| b1 | Get Activity Launcher | It's a free, open-source app that opens Android's hidden settings. Install it from F-Droid or Google Play. On a BOOX you may need to turn on Google Play first, in Settings → Apps. | primary `Next` · links `F-Droid`, `Google Play` |
| b2 | Open Android Settings | In Activity Launcher, search for "Settings" and open the one with com.android.settings written under it. | Next |
| b3 | Tap Build number 7 times | Go to About tablet and tap Build number seven times, until it says you're a developer. | Next |
| b4 | Turn on USB debugging | Go back, open System → Developer options, and switch on USB debugging. Then plug your tablet into this computer. | primary `It's on` → allow |
| allow | Tap Allow on your tablet | When your tablet asks "Allow USB debugging?", tick "Always allow from this computer" and tap Allow. | (none; auto-advances) |

Header line on a1: `Method A · Onyx menu`. On b1: `Method B · If you can't find it`. The live line at the foot of every panel (`LiveStatus`): `waiting for tablet…` (none), `tablet found · tap Allow on it` (unauthorized; the guide also jumps to `allow`), `Found it ✓` (adb, shown just before the auto-advance). Back is on every panel (`Back`) except a1, which has `Close`.

**CABLE_GUIDE** (rescue step 2/3; label `Step {i} of 4 · Cable · {p} of 6`)

| id | heading | body | art | buttons |
|---|---|---|---|---|
| c1 | You need a deep-flash cable | It's a USB cable with a small button on it. Search for "Qualcomm deep flash cable" or "EDL cable". They cost a few dollars. A normal USB cable won't work. | DeepFlashCable, button highlighted | primary `I have one` · secondary `Copy search terms` |
| c2 | Use a real USB-A port | Plug the cable's big end into a USB-A port on the computer itself. USB-C adapters and hubs often don't work. Note: "No USB-A port? Borrow a computer that has one, like a desktop or an older laptop, and run Inkroot there. There are versions for Windows and Linux." | UsbAPort + crossed-out adapter | `Next` |
| c3 | Hold the cable's button | Press the small button on the cable and keep holding it. Don't let go yet. | Hand + PressIndicator on cable | `Next` |
| c4 | Plug it into your tablet | Still holding the button, plug the cable into your tablet. | Tablet + cable | `Next` |
| c5 | Hold the power button for 15 seconds | Keep holding both buttons until the screen goes completely blank, about 15 seconds. | Tablet power button + PressIndicator | secondary `Start a 15 s timer` (stepped countdown `15 … 0`) · `Next` |
| c6 | Keep holding the cable button 10 more seconds | Let go of the power button. Keep holding the cable button for about 10 more seconds, then let go. | Hand on cable | secondary `Start a 10 s timer` |

The footer is always visible on the guide: reassurance `A black screen is normal. Your tablet isn't broken, and this is fixable.` and the live line `Waiting for your tablet…` → `Found it ✓` (device `edl`, or flow step past `waitEdl`).

**RISK_COPY**: label `Before you start`, heading `Please read this first`, bullets:
1. `Changing your tablet's software can damage it permanently or leave it unable to start.`
2. `It may void your warranty and could cause data loss.`
3. `Inkroot is free community software, provided as-is with no warranty. Its authors accept no liability for any damage.`
4. `Inkroot isn't affiliated with or endorsed by Onyx International / BOOX, Qualcomm, Google or the Magisk project.`
5. `Only use it on a tablet you own. Rooting may be restricted where you live, and checking is up to you.`

Checkbox `I understand and accept these risks; I'm responsible for my device`, link `Read the full disclaimer`, buttons `Continue` (primary) and `Back`.

**HOME_COPY**: heading `Pick a job.`. Cards: Root `magisk 30.7` (rooted: `rooted ✓`), Rescue `stuck / bootloop`, Stock `undo root` (not rooted: `not rooted`, disabled). Backup line from `backupLine()`, then link `Backups →`. Foot link `Trouble connecting? USB debugging help`.

**ROOTED_COPY**: label `Rooted`, heading `Already rooted ✓`, body `Magisk {magiskVersion} is running{svc}.` where svc = `, and so is the boot service` / `, but the boot service isn't`. Options: `Reinstall boot service` (desc `Fixes the boot service if it stopped.`) and `Return to stock` (desc `Puts the original startup software back.`). No-stock note: `You don't have an original safety copy on file. A copy made now would include root, so it can't be used to return to stock. If you saved a stock boot image when you first rooted, import it.`, button `Import a backup`. Back: `Back`.

**BACKUPS_COPY**: label `Library`, heading `Your safety copies`, body `Saved on this computer in {backupRoot}. Inkroot never uploads them.`. Row meta: `{serial} · {date} · slot {S} · {original ✓|not original} · {verified|not verified}{ · imported}`. Buttons: `Back up now`, `Import a backup…`, `Back`, row `{revealLabel}`. Empty: `No safety copies yet. Back up your tablet, or import a copy you made before.`. Rooted note: `Your tablet is rooted, so a new copy won't be original. It can still help with Rescue.`. Back-up-disabled hint: `Connect your Note Max to make a new copy.`

**RESCUE_PICK_COPY**: label `Rescue`, heading `Which copy should we put back?`, body `A tablet stuck on the ONYX logo is almost always fixable. Pick the safety copy you made of this tablet, or choose the file if you saved it somewhere else.`. Buttons: `Choose a file…`, `Continue`, `Back`. Empty: `There are no safety copies in Inkroot yet. If you have a boot image file, choose it below.`

**ABOUT_COPY**: label `About`, heading `Inkroot`, lines: `Version {version}` · `One-click root and rescue for the ONYX BOOX Note Max.` · `Inkroot collects no data. Nothing leaves your computer.` · `Not affiliated with, endorsed by or sponsored by Onyx International / BOOX, Qualcomm, Google or the Magisk project. Product names are used only to describe compatibility.` · `Free software under the MIT license. Magisk and edl are GPLv3 and ship as separate programs.` · `Safety copies: {backupRoot}`. Links: `Disclaimer`, `Licenses & third-party notices`, `Source code`. Demo chip: `DEMO MODE`.

**CLOSE_GUARD_COPY**: heading `Inkroot is writing to your tablet`, body `Closing now could leave your tablet unable to start. It'll be done in a few minutes.`, buttons `Keep Inkroot open` (primary) and `Close anyway` (danger).

**COMMON**: `Cancel`, `Finish`, `Back to start`, `Details`, `Copy log`, `Copied ✓`, `Couldn't copy. Select the text instead.`, `Backups`, `About`.

- [ ] **Step 1: Write the failing tests**

```ts
test('ALL_* lists match the contract unions', () => {
  expect(new Set(ALL_STEP_IDS).size).toBe(20); expect(ALL_PROMPT_KINDS).toHaveLength(8); expect(ALL_ERROR_CODES).toHaveLength(19);
  expect(new Set([...Object.values(FLOW_STEPS).flat(), 'done'])).toEqual(new Set(ALL_STEP_IDS));
});
test.each(FLOW_KINDS.flatMap(k => FLOW_STEPS[k].map(s => [k, s])))('copy exists for %s · %s', (k, s) => {
  if (k === 'rescue' && (s === 'cableGuide' || s === 'waitEdl')) return expect(CABLE_GUIDE.length).toBe(6);
  const c = STEP_COPY[k][s]!; expect(c.heading.length).toBeGreaterThan(0); expect(c.body.length).toBeGreaterThan(0);
});
test.each(FLOW_KINDS)('group labels cover stepCount for %s', k => expect(GROUP_LABELS[k]).toHaveLength(FLOW_STEP_COUNT[k]));
test.each(FLOW_KINDS)('done copy for %s', k => expect(DONE_COPY[k].heading).toBeTruthy());
test.each(ALL_PROMPT_KINDS)('prompt copy for %s labels every contract option', k => {
  const ids = { cableCheck:['have','whereToGet','continueWithout'], cableCheckConfirm:['confirmContinue','back'], chooseSlot:['a','b'],
    keepWaitingOrRescue:['keepWaiting','goToRescue'], installDriver:['install','cancel'], installUdev:['install','cancel'],
    edlNotFound:['retry','cancel'], retryWrite:['retry','cancel'] }[k];
  expect(Object.keys(PROMPT_COPY[k].options).sort()).toEqual([...ids].sort());
});
test.each(ALL_ERROR_CODES)('error heading for %s', c => expect(ERROR_HEADINGS[c]).toBeTruthy());
test('stepLabel', () => expect(stepLabel('root', 2, 4)).toBe('STEP 2 OF 4 · SAFETY COPY'));
test('main-path copy has no jargon', () => {
  const main = JSON.stringify([STEP_COPY, DONE_COPY, PROMPT_COPY, GATE_COPY, HOME_COPY].map(stripTech));
  for (const w of ['adb', 'EDL', '9008', 'fastboot', 'partition', 'SHA', 'verity', 'sideload']) expect(main).not.toMatch(new RegExp(`\\b${w}\\b`));
});
test('voice anchors', () => {
  expect(STEP_COPY.root.write!.body).toBe("The screen may stay blank. That's normal. Don't unplug it.");
  expect(GATE_COPY.unknownModel.heading).toBe('Inkroot only supports the BOOX Note Max right now.');
  expect(RISK_COPY.checkbox).toBe("I understand and accept these risks; I'm responsible for my device");
});
```

(`stripTech` drops the `tech` fields. "EDL cable" is allowed only in `CABLE_GUIDE`/`CABLE_BUY`, which are left out of the jargon scan as search terms the user needs.)

- [ ] **Step 2: Run it.** `npx vitest run src/lib/copy.test.ts` → FAIL.
- [ ] **Step 3: Implement** `flows.ts` and `copy.ts` with the tables above, verbatim.
- [ ] **Step 4: Run it.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): flow step tables and complete user-facing copy`

---

### Task 6: Primitive components

**Files:**
- Create in `app/src/lib/components/`: `Button.svelte`, `DitherIcon.svelte`, `JobCard.svelte`, `StepFrame.svelte`, `Checklist.svelte`, `SegmentedProgress.svelte`, `RefreshTransition.svelte`, `DetailsLog.svelte`, `AppHeader.svelte`, `DeviceChip.svelte`, `LiveStatus.svelte`, `icons.ts`
- Test: `app/src/lib/components/primitives.test.ts`

**Interfaces (props, Svelte 5 `$props()`):**
- `Button { variant?: 'primary'|'secondary'|'danger' = 'primary'; disabled?: boolean; onclick?: () => void; type?: 'button'|'submit'; children: Snippet }`. Primary: ink block, panel text, `--border`. Secondary: transparent + `--border`. Danger: transparent, `--border-heavy`, a 16px `warning` DitherIcon before the label, weight 600. Disabled: dashed border, `--muted` text, `cursor:not-allowed`, `aria-disabled`. Padding `12px 18px`, `600 15px` sans.
- `icons.ts`: `export type IconName = 'root'|'rescue'|'stock'|'warning'|'check'|'backup'|'info'; export const ICON_PATHS: Record<IconName,string>` (24×24 viewBox). root/rescue/stock are the three mockup paths verbatim. warning `M12 2 1 21h22L12 2zm1 7v6h-2V9zm0 8v2h-2v-2z`; check `M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z`; backup `M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2z`; info `M12 2a10 10 0 100 20 10 10 0 000-20zm1 15h-2v-6h2zm0-8h-2V7h2z`.
- `DitherIcon { name: IconName; size?: number = 34; cell?: 2|3|4 = 3 }`: a `span[aria-hidden]` with `mask: url(data:image/svg+xml,…path…) center/contain no-repeat` (and `-webkit-mask`), background `repeating-conic-gradient(currentColor 0 25%,transparent 0 50%) 0 0/{cell}px {cell}px`, `opacity:.85` (mockup `.B .ch .ico`).
- `JobCard { title: string; desc: string; icon: IconName; primary?: boolean; disabled?: boolean; onclick: () => void }`: a `<button>` with `--border`, padding `14px 12px 12px`, min-height `150px`, column gap `8px`, 40px icon, title `600 16px` pushed to the bottom (`margin-top:auto`), desc mono `400 12px` `opacity:.7`. Primary is inverted (ink background).
- `Checklist { items: { label: string; done: boolean }[] }`: 14px square, `2px` ink border, done = 8px ink inner square (mockup `.checks`).
- `StepFrame { label: string; heading: string; body?: string; art?: Snippet; children?: Snippet; actions?: Snippet; tone?: 'normal'|'danger' }`: label `--fs-label`, `h1` `--fs-heading`, `p` body max-width 52ch, art centred and flexible, actions at the bottom in a row with primary right-aligned. Danger tone gives the frame a `--border-heavy` inset box.
- `SegmentedProgress { done?: number; total?: number; segments?: number = 24; meta?: string }`: `role="progressbar"`, `aria-valuemin=0 aria-valuemax=100 aria-valuenow`. Filled segments are solid ink and the next one is `--dither-3`. Indeterminate (no total): one dithered segment hops with `steps(1)` animation, class `marching`. Meta row is mono: left `{label} · {done} / {total} MB` (via formatBytes), right empty.
- `RefreshTransition { key: unknown; children: Snippet }`: `{#key key}<div class="flash">…{/key}`.
- `DetailsLog { lines: LogLine[]; tech?: string; onCopy: () => Promise<boolean> }`: a toggle button `Details ▸/▾` (mono label). The expanded `<pre>` holds `tech` first, then lines formatted `{ts.slice(11,19)} {stream==='stderr'?'!':' '} {line}` (last 500 shown), max-height `180px`, scroll, auto-scroll to bottom. `Copy log` shows `Copied ✓` for 2 s, or the failure text.
- `DeviceChip { status: DeviceStatus }`: ink block, panel text, `--fs-chip`, text = `formatChip(status)`, `aria-live="polite"`.
- `AppHeader { device: DeviceStatus; demo: boolean; navLocked: boolean; onHome: () => void; onNav: (s: 'backups'|'about') => void }`: wordmark button (16px `.mk` square `--dither-4` + 2px border, then `INKROOT`), nav text links `Backups`, `About` (hidden while `navLocked`), `DEMO MODE` outline chip when demo, DeviceChip at the right.
- `LiveStatus { state: 'waiting'|'found'|'attention'; text: string }`: mono line with a stepped 3-dot ellipsis while waiting (static under reduced motion), and `✓` / `●` for the other states. `aria-live="polite"`.

- [ ] **Step 1: Write the failing tests**

```ts
test('danger button has heavy border class and warning icon', /* render Button variant danger → has class 'danger'; contains [data-icon="warning"] */);
test('disabled button does not fire', /* click → onclick not called; aria-disabled="true" */);
test('segmented progress fills proportionally', /* done 50,total 100,segments 24 → 12 [data-filled] and aria-valuenow 50 */);
test('segmented progress is indeterminate without total', /* has .marching, no aria-valuenow */);
test('refresh transition remounts with flash on key change', /* rerender key 1→2: new element !== old, has class flash */);
test('details log hides raw output until expanded and copies', /* line text not visible; click Details → visible; click Copy log → onCopy called, shows "Copied ✓" */);
test('device chip text', /* adb NOTE_MAX → "● NOTE MAX / FW 4.2 / SLOT_B" */);
test('job card primary is inverted', /* primary → class primary */);
```

- [ ] **Step 2: Run them.** `npx vitest run src/lib/components` → FAIL.
- [ ] **Step 3: Implement** each component per its props. Styles are scoped and use only Task 1 tokens.
- [ ] **Step 4: Run them.** → PASS. `npm run check` → 0 errors.
- [ ] **Step 5: Commit** `feat(ui): Panel primitives (buttons, dither icons, progress, step frame, details log, header)`

---

### Task 7: Illustrations

**Files:**
- Create in `app/src/lib/art/`: `DitherDefs.svelte`, `Tablet.svelte`, `DeepFlashCable.svelte`, `UsbAPort.svelte`, `Hand.svelte`, `PressIndicator.svelte`, `MenuSketch.svelte`, `index.ts`
- Test: `app/src/lib/art/art.test.ts`

**Interfaces:**
- `DitherDefs { id: string; cell?: 2|3 = 2 }`: `<pattern id={id} width={cell*2} height={cell*2} patternUnits="userSpaceOnUse">` with two ink squares (a checkerboard).
- Every art component: `{ width?: number; highlight?: string /* part name */; title: string }`. It renders `<svg role="img" aria-label={title} viewBox=… fill="none" stroke="currentColor" stroke-width="2">`, uses `$props.id()` for a unique pattern id, and highlighted parts are filled `url(#id)` (dither). Line art only: straight lines and simple arcs, `stroke-linecap="square"`.
  - `Tablet { highlight?: 'screen'|'power'|'port'; screen?: 'blank'|'logo'|'content'|'prompt'|'menu' }`: generic portrait slate, 3:4, an uneven bezel, power button on the top edge, USB-C port on the bottom edge. `logo` is a plain small rectangle, **never** the ONYX/BOOX mark. `prompt` draws an "Allow?" dialog sketch with a tick box.
  - `DeepFlashCable { highlight?: 'button'|'usbA'|'usbC' }`: USB-A plug, cable, an inline box with a round button, USB-C plug.
  - `UsbAPort { variant: 'port'|'adapterCrossed' }`: computer side panel with a USB-A port. `adapterCrossed` adds a USB-C→A dongle with an X.
  - `Hand { pose: 'press'|'hold' }`: a simple outline thumb/finger, used with `PressIndicator`.
  - `PressIndicator { seconds?: number }`: concentric stepped arcs plus an optional mono `{n} s` text.
  - `MenuSketch { variant: 'onyxApps'|'kebab'|'toggle'|'activityLauncher'|'buildNumber'|'devOptions' }`: schematic list rows (lines) with the target row dithered and a ⋮ glyph for `kebab`. No real UI screenshots.

- [ ] **Step 1: Write the failing tests**

```ts
test.each(ALL_ART)('%s is inline vector, labelled, no raster', (name, C) => {
  const { container } = render(C, { title: name });
  const svg = container.querySelector('svg')!; expect(svg.getAttribute('role')).toBe('img'); expect(svg).toHaveAttribute('aria-label', name);
  expect(container.querySelector('image, img, foreignObject')).toBeNull();
  expect(container.innerHTML).not.toMatch(/BOOX|ONYX/i);
});
test('two instances get distinct pattern ids', /* render Tablet highlight screen twice; pattern ids differ; fills reference own id */);
test('highlight applies dither fill', /* DeepFlashCable highlight button → [data-part=button] fill matches url(#…) */);
```

- [ ] **Step 2: Run them.** `npx vitest run src/lib/art` → FAIL.
- [ ] **Step 3: Implement** the SVGs. Every part carries a `data-part` attribute. Keep each file under about 80 lines of markup.
- [ ] **Step 4: Run them.** → PASS. Look at them in the browser through `?mock=shot-rescue-cable` after Task 11 (a visual check, not a gate).
- [ ] **Step 5: Commit** `feat(ui): original line-and-dither illustrations for cable, ports, tablet and menus`

---

### Task 8: PromptView

**Files:**
- Create: `app/src/lib/components/PromptView.svelte`, `app/src/lib/components/CableBuyPanel.svelte`
- Test: `app/src/lib/components/PromptView.test.ts`

**Interfaces:**
- `PromptView { prompt: Prompt; kind: FlowKind; device: DeviceStatus; imageName?: string; disabled: boolean; onAnswer: (optionId: string) => void; onOpen: (url: string) => void; onCopy: (text: string) => void }`.
- Renders `PROMPT_COPY[prompt.kind]` inside a `StepFrame`, using `EDL_NOT_FOUND_AUTO` when `kind !== 'rescue'`. Buttons come from `prompt.options`, **in backend order**: the label is `PROMPT_COPY[kind].options[id] ?? option.label`, and the variant is `option.emphasis`. Focus goes to the first `primary` option on mount, never to a `danger` one.
- `chooseSlot` hint: `/boot_([ab])/i` on `imageName` → "file name says"; else device slot → "using right now"; else empty.
- `cableCheck` + `whereToGet` is handled **locally**: it shows `CableBuyPanel` (DeepFlashCable art + CABLE_BUY copy) and does **not** call `onAnswer`. `Back` returns to the three options. My call: the contract lists the option but not what the backend does after it.
- `cableCheckConfirm`: the `confirmContinue` button stays disabled until the tick box is checked.
- `edlNotFound` (rescue) shows the bullets plus small `UsbAPort variant="adapterCrossed"` art. `installDriver`/`installUdev` show `info` DitherIcon art. `retryWrite` uses `DeepFlashCable highlight="usbC"`.
- After any answer the component sets local `answered = true` (all buttons disabled) until `prompt.id` changes.

- [ ] **Step 1: Write the failing tests**

```ts
const mk = (kind, ids, emph = {}) => ({ id: 'p1', kind, options: ids.map(id => ({ id, label: 'raw-' + id, emphasis: emph[id] ?? 'secondary' })) });
test.each(ALL_PROMPT_KINDS)('%s renders heading and every option', kind => {
  /* render with contract option ids; expect heading PROMPT_COPY[kind].heading (or EDL_NOT_FOUND_AUTO for non-rescue edlNotFound);
     every option button present with copy label; clicking first non-local option calls onAnswer(id) */
});
test('whereToGet opens the buy panel without answering', /* click "Show me where to get one" → "Getting a rescue cable" visible; onAnswer not called; Back → options again */);
test('continue without cable needs the tick', /* cableCheckConfirm: "Continue without a cable" disabled; tick box → enabled → click → onAnswer('confirmContinue') */);
test('danger option is never auto-focused', /* cableCheck with continueWithout danger, have primary → document.activeElement is "Yes, I have one" */);
test('second click is ignored until next prompt', /* click retry twice → onAnswer called once; rerender with prompt id p2 → enabled again */);
test('chooseSlot hint from file name', /* imageName boot_a.stock.img → "The file name says A, so we've picked that." */);
test('edlNotFound copy differs for rescue vs root', /* kind rescue → bullets; kind root → "Your tablet didn't switch modes in time" */);
```

- [ ] **Step 2: Run them.** → FAIL.
- [ ] **Step 3: Implement** `PromptView.svelte` and `CableBuyPanel.svelte`.
- [ ] **Step 4: Run them.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): PromptView for every prompt kind with deliberate no-cable confirmation`

---

### Task 9: Home, gate screens, USB debugging guide, Already rooted

**Files:**
- Create in `app/src/lib/screens/`: `Home.svelte`, `GateScreen.svelte`, `UsbDebugGuide.svelte`, `AlreadyRooted.svelte`
- Test: `app/src/lib/screens/entry.test.ts`

**Interfaces:**
- All screens take no props and read `getApp()`.
- `Home`: `HOME_COPY` heading (`--fs-display`), backup line (`--fs-meta`) from `backupLine(app.stockBackup ?? newest backup for serial)`, then a 3-column grid of `JobCard`s, 10px gap (mockup `.B .choices`):
  - Root (primary, icon root) → `app.deviceInfo.rooted ? navigate({screen:'alreadyRooted'}) : startJob('root', { backupId: app.stockBackup?.id })`.
  - Rescue (icon rescue) → `navigate({screen:'rescuePick'})`.
  - Stock (icon stock, disabled unless rooted) → `startJob('unroot', { backupId: app.stockBackup?.id })`.
  - `Backups →` link; foot link → `navigate({screen:'usbGuide'})`.
- `GateScreen { gate: 'none'|'unauthorized'|'edl'|'unknownModel'|'unknownFirmware'|'lowBattery' }`: `GATE_COPY[gate]` in a StepFrame with art (`Tablet` screen `blank` for none, `prompt` for unauthorized, `blank` for edl, `content` otherwise, plus a `warning` DitherIcon for the blocked gates). The unknownFirmware button calls `platform.openExternal(newFirmwareIssueUrl(...))`.
- `UsbDebugGuide`: local `panel: 'a1'|'a2'|'a3'|'b1'|'b2'|'b3'|'b4'|'allow'` (starts at `a1`) with transitions as in the USB_GUIDE table. An `$effect` jumps to `allow` when `app.device.kind === 'unauthorized'`. Leaving the guide when adb/edl appears comes from `resolveScreen` (Task 4); `App.svelte` resets the route (Task 11). Art per panel: a1 `MenuSketch onyxApps`, a2 `kebab`, a3 `toggle`, b1 `activityLauncher`, b2 `activityLauncher`, b3 `buildNumber`, b4 `devOptions`, allow `Tablet screen="prompt"`. `LiveStatus` sits at the foot. Panels change inside `RefreshTransition key={panel}`.
- `AlreadyRooted`: ROOTED_COPY, two `JobCard`-style options (`Reinstall boot service` → `startJob('reinstallBootService', {})`, `Return to stock` → `startJob('unroot', { backupId: app.stockBackup?.id })`). The no-stock note plus `Import a backup` (→ `app.importBackup()`, Task 4) shows when `app.stockBackup === null`.

- [ ] **Step 1: Write the failing tests** (use `renderWithApp(App, { scenario })` so routing is real)

```ts
test.each([
  ['no-device', 'Plug in your Note Max'], ['unauthorized', 'Tap Allow on your tablet'], ['edl', 'Your tablet is in rescue mode'],
  ['unknown-model', 'Inkroot only supports the BOOX Note Max right now.'], ['unknown-firmware', "This software version isn't supported yet"],
  ['low-battery', 'Charge to at least 30 % first'],
])('scenario %s shows gate "%s"', async (s, h) => { await renderWithApp(App, { scenario: s }); expect(screen.getByRole('heading', { name: h })).toBeVisible(); });
test('gate-none offers Rescue', /* click "Tablet stuck on the ONYX logo? Rescue it →" → heading "Which copy should we put back?" */);
test('low battery shows the level', /* "Your tablet is at 18 %" */);
test('unknown firmware opens a prefilled issue', /* click "Ask for support on GitHub" → platform.openExternal called with url containing "Firmware%20support%3A%20TKQ1.230721.003" */);
test('home shows the job cards and backup line', /* ready → "Pick a job.", "backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc", buttons Root/Rescue/Stock; Stock disabled */);
test('usb guide method B path', /* no-device → Show me how → Next ×2 → "I can't find it" → heading "Get Activity Launcher" */);
test('usb guide auto-advances when the tablet appears', /* scenario plug-in, open guide; advance 4000 → heading "Tap Allow on your tablet" (allow panel); advance 4000 → "Pick a job." */);
test('usb guide reachable from home', /* ready → "Trouble connecting? USB debugging help" → shows "Open Settings, then Apps" */);
test('rooted device: Root card leads to Already rooted with both options', /* rooted → click Root → "Already rooted ✓", "Reinstall boot service", "Return to stock"; no import note */);
test('rooted without stock backup points to import', /* rooted-no-stock → note text + "Import a backup" */);
```

Note on the plug-in test: the guide is open while the device is `none` → `unauthorized`, and `resolveScreen` keeps `usbGuide` during `unauthorized`, so the guide's `allow` panel heading shows rather than the gate.

- [ ] **Step 2: Run them.** → FAIL.
- [ ] **Step 3: Implement** the four screens.
- [ ] **Step 4: Run them.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): home, device gates, illustrated USB debugging guide, already-rooted screen`

---

### Task 10: Risk notice, About, Backups library + import, Rescue picker

**Files:**
- Create in `app/src/lib/screens/`: `RiskNotice.svelte`, `About.svelte`, `Backups.svelte`, `RescuePick.svelte`
- Test: `app/src/lib/screens/library.test.ts`

**Interfaces:**
- Consumes `AppState.importBackup()` (Task 4).
- `RiskNotice`: RISK_COPY in a StepFrame with `tone="danger"` and a `warning` icon. A native checkbox (styled as a 2px square, ticked = ink inner square) bound to local `accepted` (starts `false`). `Continue` is `disabled={!accepted || app.inFlight}` and calls `app.acceptRisk()`. `Back` → `navigate({screen:'home'})`. The disclaimer link → `openExternal(DISCLAIMER_URL)`.
- `About`: ABOUT_COPY with `app.info.version`/`backupRoot`; links via `openExternal` to `DISCLAIMER_URL`, `NOTICES_URL`, `REPO`.
- `Backups`: lists `app.backups` (all serials, newest first, grouped under a mono serial heading). Row = BACKUPS_COPY meta + a `revealLabel(os)` button → `backend.revealPath(b.path)`. `Back up now` → `startJob('backup', {})`, disabled unless `device.kind==='adb' && gate===null` (otherwise shows the hint). `Import a backup…` → `app.importBackup()`. The rooted note shows when `deviceInfo?.rooted`.
- `RescuePick`: radio list of backups (all, grouped by serial; each row shows slot and `original ✓`/`not original`). Local `choice: { backupId: string } | { imagePath: string } | null`, initialised from `route.preselect.backupId`, else the newest backup for the last-seen serial, else null. `Choose a file…` → `platform.pickBootImage()` sets `{imagePath}` and shows the file name mono. `Continue` (disabled while null) → `startJob('rescue', { ...choice, slot: route.preselect?.slot })`.

- [ ] **Step 1: Write the failing tests**

```ts
test('risk gate: Continue disabled until ticked, then sets version and starts', async () => {
  const { app } = await renderWithApp(App, { scenario: 'no-backup' }); // acceptedRiskVersion null
  await click(screen.getByRole('button', { name: /Root/ }));
  const cont = screen.getByRole('button', { name: 'Continue' }); expect(cont).toBeDisabled();
  await click(screen.getByRole('checkbox', { name: "I understand and accept these risks; I'm responsible for my device" }));
  expect(cont).toBeEnabled(); await click(cont);
  expect(app.settings.acceptedRiskVersion).toBe(1); expect(app.flow?.kind).toBe('root');
});
test('risk notice also gates rescue and unroot but not backup or import', /* rescue → risk screen; Backups → Back up now → flow starts directly */);
test('risk notice links the disclaimer', /* click "Read the full disclaimer" → openExternal(DISCLAIMER_URL) */);
test('about states privacy and non-affiliation', /* "Inkroot collects no data. Nothing leaves your computer." and /Not affiliated with/ and "Version 0.1.0-mock" */);
test('import uses the file dialog and starts importBackup', /* Backups → Import a backup… → flow.kind 'importBackup'; startFlow called with {imagePath:'/Users/you/Downloads/boot_b.stock.img'} */);
test('rescue picker preselects from route and passes slot', /* navigate rescuePick preselect {backupId, slot:'b'} (accepted settings) → Continue → startFlow('rescue', {backupId, slot:'b'}) */);
test('rescue picker with no backups needs a file', /* no-device scenario, no backups → Continue disabled; Choose a file… → enabled */);
test('reveal button calls reveal_path', /* Backups → "Show in Finder" → revealPath(STOCK_BACKUP.path) */);
```

- [ ] **Step 2: Run them.** → FAIL.
- [ ] **Step 3: Implement** the four screens.
- [ ] **Step 4: Run them.** → PASS.
- [ ] **Step 5: Commit** `feat(ui): risk acknowledgement, about, backup library with import, rescue picker`

---

### Task 11: FlowScreen, CableGuide, Done/Failure views, close guard dialog, App shell

**Files:**
- Create: `app/src/lib/screens/FlowScreen.svelte`, `CableGuide.svelte`, `DoneView.svelte`, `FailureView.svelte`, `app/src/lib/components/CloseGuardDialog.svelte`
- Replace: `app/src/App.svelte`
- Test: `app/src/lib/screens/flow.test.ts`

**Interfaces:**
- `App.svelte`:
  - `onMount` → `selectBackend` → `new AppState` → `setApp` → `connect()`. A centred mono `starting…` shows until `info` loads.
  - Shell: `AppHeader` (navLocked = `app.flow !== null`) + `<main>` with `RefreshTransition key={app.screen + (app.flow?.step ?? '') + (app.flow?.status ?? '')}` + footer `DetailsLog` (lines filtered to `app.flow?.flowId` when a flow exists, else all; `tech` = the current StepCopy.tech or `error.detail`) + `CloseGuardDialog` when `app.closeBlocked`.
  - Screen switch on `app.screen`, with gate ids mapped to `GateScreen`.
  - `$effect`: if `app.route.screen === 'usbGuide' && app.screen !== 'usbGuide'`, set the route to home (the auto-advance).
- `FlowScreen` (reads `app.flow`) branches:
  - `status 'done'` → `DoneView`.
  - `'failed'` → `FailureView`.
  - `'cancelled'` → StepFrame `Stopped` / heading `Stopped.` / body `Nothing else will happen. You can start again whenever you're ready.` + `Back to start`. Add these strings to `copy.ts` as `CANCELLED_COPY`, with a copy test.
  - `prompt` → `PromptView`.
  - `kind==='rescue' && (step==='cableGuide'||step==='waitEdl')` → `CableGuide`.
  - Otherwise StepFrame with `stepLabel(...)`, STEP_COPY (or FALLBACK_STEP), `SegmentedProgress` from `app.progress` (indeterminate if null), art (`Tablet screen="blank"` for write/verifyWrite/enterEdl, `PressIndicator` for waitBoot, `Checklist` for root `verifyBackup`/`readPartitions` with items `Firmware {firmwareLabel} recognised`, `Active slot: {S}`, `Startup software saved`, `Copy checked`), and a secondary `Cancel` button only when `!flow.writing`. While writing, show a mono line in its place: `writing · please keep it plugged in`.
- `CableGuide`: local `panel` index 0..5 over CABLE_GUIDE, art per row, countdown buttons (stepped `setInterval` 1 s, text only), footer reassurance + `LiveStatus` (`found` when `app.device.kind==='edl'` or step is `waitEdl` with device edl), and a `Cancel` secondary.
- `DoneView`: DONE_COPY[kind] (`DONE_IMPORT_RESCUE_ONLY` when importBackup and `result.backup.stock === false`). Then:
  - `Checklist` of checks (all done).
  - root: `ROOT_TIP` box (2px border, `info` icon) + `ROOT_MORE` link.
  - backup: path + `revealLabel` → `revealPath(result.backup.path)`.
  - rescue: `LiveStatus` from `app.device.kind==='adb'`.
  - Primary `Finish` → `dismissFlow()`.
- `FailureView`: label `Stopped`, heading `ERROR_HEADINGS[code]`, body `error.message`, primary from `suggestedAction`:
  - retry → `startJob(lastStart.kind, lastStart.options)` after dismissing.
  - goToRescue → dismiss + rescuePick preselect.
  - importBackup → dismiss + `importBackup()`.
  - reconnect/chargeDevice → dismiss.
  - Secondary `Back to start` → dismiss. Foot: `COMMON.failedFoot`.
- `CloseGuardDialog`: modal (`role="alertdialog"`, focus trapped on `Keep Inkroot open`), CLOSE_GUARD_COPY. `Close anyway` (danger) → `app.confirmClose()`. Keep → `closeBlocked = false`.

- [ ] **Step 1: Write the failing tests** (mock scenarios, fake timers, `speed` large)

```ts
test('flow shows step label, heading and progress', /* shot-root-writing → "STEP 3 OF 4 · ROOTING", "Writing to your tablet", progressbar aria-valuenow 61 */);
test('writing hides Cancel and blocks close', /* shot-root-writing → no "Cancel" button; trigger captured close handler → alertdialog "Inkroot is writing to your tablet" */);
test('Cancel visible when not writing and calls cancel_flow', /* ready, start backup at checkDevice → click Cancel → cancelFlow called → "Stopped." */);
test('full root happy path renders done with freeze tip', /* accepted; Root → cableCheck → "Yes, I have one" → … → "Your Note Max is rooted." + "One more thing" + checklist 3 items */);
test('boot timeout offers Keep waiting / Go to Rescue and Rescue preselects backup', /* root-boot-timeout → "Your tablet hasn't restarted yet"; Go to Rescue → "Which copy should we put back?" with STOCK_BACKUP radio checked */);
test('hash mismatch prompt then success', /* root-hash-mismatch → "That write didn't check out" → Try again → done */);
test('failure view uses heading table and suggested action', /* root-patch-failed → "Preparing root didn't work" + "Try again" button; Back to start → home */);
test('rescue shows cable guide with live status and found state', /* shot-rescue-cable → "You need a deep-flash cable", "Waiting for your tablet…", "A black screen is normal…"; emit device edl → "Found it ✓" */);
test('rescue edl not found shows troubleshooting', /* rescue-edl-not-found → after 6 s "We can't see your tablet yet" with bullets */);
test('reinstall that switches to root keeps rendering', /* reinstall-hook-absent → after switch, label "STEP 2 OF 4 · SAFETY COPY" */);
test('backup done reveals folder', /* backup happy → "Your safety copy is saved." → "Show in Finder" → revealPath(path) */);
test('import rescue-only done copy', /* import-rescue-only → "Saved for rescue only." */);
test('unknown step falls back without crashing', /* inject state with step 'pushScripts' kind 'backup' → "Working on it" */);
test('every screen has Details and Copy log', /* for scenarios ready,no-device,shot-root-writing: "Details" button present; expand → "Copy log" */);
```

- [ ] **Step 2: Run them.** `npx vitest run src/lib/screens/flow.test.ts` → FAIL.
- [ ] **Step 3: Implement** the components and `App.svelte`.
- [ ] **Step 4: Run them.** → PASS. Run the full suite: `npm test` → all PASS, `npm run check` → 0 errors. Manual: `npm run dev:mock`, click through `ready`, `no-device`, `plug-in`, `root-boot-timeout`, `rescue-edl-not-found` with `&speed=5`. The refresh flash plays on each step and is absent with OS reduced motion.
- [ ] **Step 5: Commit** `feat(ui): generic flow screen, illustrated cable guide, done/failure views, close guard`

---

### Task 12: Playwright screenshots

**Files:**
- Create: `app/playwright.config.ts`, `app/e2e/screenshots.spec.ts`
- Output: `assets/screenshots/home.png`, `root-step.png`, `rescue-cable.png`, `done.png`, `home-dark.png` (repo root)

**Interfaces:**
- Consumes: scenarios `ready`, `shot-root-writing`, `shot-rescue-cable`, `shot-root-done`.
- Produces: the five PNG filenames above, exactly. The release+web plan consumes them.

`playwright.config.ts`: `webServer: { command: 'npx vite --port 5179 --strictPort', url: 'http://localhost:5179', reuseExistingServer: true }`, `use: { baseURL: 'http://localhost:5179', viewport: { width: 880, height: 640 }, deviceScaleFactor: 2, reducedMotion: 'reduce' }`, one Chromium project, `testDir: 'e2e'`. First-time setup: `npx playwright install chromium`.

- [ ] **Step 1: Write the spec**

```ts
const OUT = path.resolve(__dirname, '../../assets/screenshots');
const shots = [
  ['home.png', 'ready', 'light', 'Pick a job.'],
  ['root-step.png', 'shot-root-writing', 'light', 'Writing to your tablet'],
  ['rescue-cable.png', 'shot-rescue-cable', 'light', 'You need a deep-flash cable'],
  ['done.png', 'shot-root-done', 'light', 'Your Note Max is rooted.'],
  ['home-dark.png', 'ready', 'dark', 'Pick a job.'],
] as const;
for (const [file, scenario, scheme, heading] of shots) test(file, async ({ page }) => {
  await page.emulateMedia({ colorScheme: scheme, reducedMotion: 'reduce' });
  await page.goto(`/?mock=${scenario}&speed=1000`);
  await expect(page.getByRole('heading', { name: heading })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  expect(await page.evaluate(() => document.fonts.check('600 16px "IBM Plex Mono"'))).toBe(true);
  await page.screenshot({ path: path.join(OUT, file) });
});
test('makes no network requests outside localhost', /* page.on('request') during home → every url starts with http://localhost:5179 */);
```

- [ ] **Step 2: Run it.** `npm run screenshots` → FAIL before the config exists, then PASS. `ls ../assets/screenshots` lists the five files.
- [ ] **Step 3: Check them by eye** against mockup direction B: panel background, hard borders, dithered icons, inverted chip, and dark-mode inversion in `home-dark.png`.
- [ ] **Step 4: Commit** `feat(ui): Playwright screenshot script for README/site assets` (include the PNGs).

---

## Spec decisions made in this plan (builder's calls, flag to reviewer)

- `RISK_NOTICE_VERSION = 1` is duplicated in the UI (`legal.ts`) because no command exposes it. It must match the backend constant.
- The risk acknowledgement also gates `reinstallBootService`, because it can turn into a root write.
- `cableCheck → whereToGet` is handled entirely in the UI, with no `answer_prompt`. The UI persists `hasRescueCable` (`true` on `have`, `false` on `confirmContinue`). Whether to skip the prompt next time is backend logic.
- On `goToRescue`, the UI answers and then calls `cancel_flow` itself before routing to the rescue picker.
- `set_settings` always sends the full `Settings` object.
- External links and the file dialog need app-core to register `tauri-plugin-opener` and `tauri-plugin-dialog`, and capabilities `dialog:allow-open`, `opener:allow-open-url`, `core:window:allow-destroy`, `core:window:allow-close`.
- `THIRD_PARTY_NOTICES` and `DISCLAIMER.md` are linked on GitHub (`main`), not bundled. The release plan may switch these to bundled files.
- Home always shows when a supported device is connected. A rooted device makes the Root card lead to "Already rooted"; Stock is disabled when not rooted.
- No ETA on progress bars (YAGNI).
