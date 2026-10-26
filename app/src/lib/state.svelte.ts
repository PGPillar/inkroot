import {
  toFlowError,
  type AppInfo, type Backend, type BackupSummary, type DeviceInfo, type DeviceStatus, type FlowKind,
  type FlowOptions, type FlowState, type LogLine, type Progress, type Settings, type Unlisten,
} from './ipc';
import { LOG_NOTES, fill } from './copy';
import { FLOW_STEPS } from './flows';
import { RISK_NOTICE_VERSION, RISK_REQUIRED } from './legal';
import type { Platform } from './platform';
import { resolveScreen, type Route } from './screen';

const LOG_CAP = 5000;

const isTerminal = (f: FlowState) => f.status === 'done' || f.status === 'failed' || f.status === 'cancelled';
const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** Rescue steps that only happen once the tablet was found in rescue mode. */
const AFTER_WAIT_EDL = new Set<string>([...FLOW_STEPS.rescue.slice(FLOW_STEPS.rescue.indexOf('waitEdl') + 1), 'done']);

export interface LastStart {
  kind: FlowKind;
  options: FlowOptions;
  serial: string | null;
}

/** All UI state: what the backend reports plus where the user is. Screens read it through getApp(). */
export class AppState {
  device = $state<DeviceStatus>({ kind: 'none' });
  flow = $state<FlowState | null>(null);
  progress = $state<Progress | null>(null);
  logs = $state<LogLine[]>([]);
  settings = $state<Settings>({ hasRescueCable: null, acceptedRiskVersion: null });
  info = $state<AppInfo | null>(null);
  backups = $state<BackupSummary[]>([]);
  route = $state<Route>({ screen: 'home' });
  closeBlocked = $state(false);
  /** The last tablet seen over adb; flows keep showing its details while it is in rescue mode. */
  lastSeen = $state<DeviceInfo | null>(null);
  /** A start/answer/cancel call is pending. */
  inFlight = $state(false);
  /** The last link the system browser couldn't open; screens show a note next to it. */
  openFailed = $state<string | null>(null);
  screen = $derived(resolveScreen({ device: this.device, flow: this.flow, route: this.route }));

  /** What was last started, for "Try again" and for the Rescue preselect. */
  lastStart: LastStart | null = null;

  #backend: Backend;
  #platform: Platform;
  #dismissed = new Set<string>();
  /** Where to go once the current flow ends (set by goToRescue). */
  #afterEnd: Route | null = null;
  /** True while start_flow is pending: a state event for a new flowId may replace a finished flow. */
  #starting = false;
  /** The flowId start_flow last returned; its states are adopted even if they arrive after the call resolves. */
  #expectedFlowId: string | null = null;
  #localCount = 0;
  /** device://status events seen since connect() subscribed; a later snapshot mustn't overwrite them. */
  #deviceEvents = 0;

  constructor(backend: Backend, platform: Platform) {
    this.#backend = backend;
    this.#platform = platform;
  }

  /** For screens that open links or files themselves (openExternal, pickBootImage). */
  get platform(): Platform {
    return this.#platform;
  }

  get deviceInfo(): DeviceInfo | null {
    return this.device.kind === 'adb' ? this.device.info : null;
  }

  /**
   * The tablet is in rescue mode, or the rescue flow is already past waiting for it. The backend
   * stops polling the device while a flow runs, so the live status alone can lag behind.
   */
  get edlFound(): boolean {
    const f = this.flow;
    return this.device.kind === 'edl' || (f?.kind === 'rescue' && AFTER_WAIT_EDL.has(f.step));
  }

  get stockBackup(): BackupSummary | null {
    return this.#stockBackupFor(this.deviceInfo?.serial ?? null);
  }

  #stockBackupFor(serial: string | null): BackupSummary | null {
    if (serial === null) return null;
    const usable = this.backups.filter((b) => b.serial === serial && b.verified && b.stock);
    return usable.reduce<BackupSummary | null>((best, b) => (best && best.createdAt >= b.createdAt ? best : b), null);
  }

  async connect(): Promise<Unlisten> {
    const b = this.#backend;
    // Subscribe first so nothing emitted while loading is missed.
    const unlistens = await Promise.all([
      b.onDeviceStatus((d) => {
        this.#deviceEvents++;
        this.#setDevice(d);
      }),
      b.onFlowState((f) => this.#onFlowState(f)),
      b.onFlowProgress((p) => {
        if (p.flowId === this.flow?.flowId) this.progress = p;
      }),
      b.onFlowLog((l) => this.#onLog(l)),
      this.#platform.onCloseRequested(() => {
        if (this.flow?.writing) {
          this.closeBlocked = true;
          return false;
        }
        return true;
      }),
    ]);
    const [info, device, settings, backups] = await Promise.all([
      b.getAppInfo(), b.getDevice(), b.getSettings(), b.listBackups(),
    ]);
    this.info = info;
    // The backend only emits on change, so an event that beat the snapshot is the newer truth.
    if (this.#deviceEvents === 0) this.#setDevice(device);
    this.settings = settings;
    this.backups = backups;
    return () => unlistens.forEach((u) => u());
  }

  #setDevice(d: DeviceStatus) {
    this.device = d;
    if (d.kind === 'adb') this.lastSeen = d.info;
  }

  #onFlowState(f: FlowState) {
    if (this.#dismissed.has(f.flowId)) return;
    const prev = this.flow;
    const same = prev?.flowId === f.flowId;
    // A different flowId only takes over an empty screen or one being replaced by a start we made.
    if (prev && !same) {
      if (!this.#starting && f.flowId !== this.#expectedFlowId) return;
      this.#dismissed.add(prev.flowId);
    }
    if (!same || prev!.step !== f.step) this.progress = null;
    this.flow = f;
    this.inFlight = false;
    if (isTerminal(f)) {
      if (!(same && isTerminal(prev!))) void this.#refreshBackups();
      if (this.#afterEnd) this.dismissFlow();
    }
  }

  #onLog(l: LogLine) {
    this.logs.push(l);
    if (this.logs.length > LOG_CAP) this.logs.splice(0, this.logs.length - LOG_CAP);
  }

  async #refreshBackups() {
    try {
      this.backups = await this.#backend.listBackups();
    } catch {
      // Keep the list we have; the next refresh will try again.
    }
  }

  navigate(route: Route): void {
    this.route = route;
    this.openFailed = null;
  }

  /** Adds a line of our own to Details, for things that went wrong on this side of the backend. */
  #note(line: string) {
    this.#onLog({ flowId: this.flow?.flowId ?? null, ts: new Date().toISOString(), stream: 'info', line });
  }

  /** Opens a link in the system browser. If that fails, says so in Details and next to the link. */
  async openExternal(url: string): Promise<void> {
    this.openFailed = null;
    try {
      await this.#platform.openExternal(url);
    } catch (e) {
      this.#note(fill(LOG_NOTES.openFailed, { url, error: errorText(e) }));
      this.openFailed = url;
    }
  }

  async startJob(kind: FlowKind, options: FlowOptions = {}): Promise<void> {
    if (this.inFlight || (this.flow && !isTerminal(this.flow))) return;
    if (RISK_REQUIRED.includes(kind) && this.settings.acceptedRiskVersion !== RISK_NOTICE_VERSION) {
      // A finished flow would outrank the risk screen, so put it away first.
      if (this.flow) this.#clearFlow();
      this.route = { screen: 'risk', then: { kind, options } };
      return;
    }
    this.lastStart = { kind, options, serial: this.deviceInfo?.serial ?? null };
    this.#afterEnd = null;
    // inFlight stays true after start_flow resolves, until the first state for the new flow arrives
    // (the backend may emit it before or after the call returns). Only a rejection clears it here.
    this.inFlight = true;
    this.#starting = true;
    try {
      this.#expectedFlowId = await this.#backend.startFlow(kind, options);
    } catch (e) {
      this.#failLocally(kind, e);
    } finally {
      this.#starting = false;
    }
  }

  /** Shows a request the backend refused as a failed flow, so the user sees why nothing started. */
  #failLocally(kind: FlowKind, e: unknown) {
    if (this.flow) this.#dismissed.add(this.flow.flowId);
    this.progress = null;
    this.flow = {
      flowId: `local-${++this.#localCount}`, kind, step: 'checkDevice', stepIndex: 1, stepCount: 1,
      status: 'failed', error: toFlowError(e), prompt: null, result: null, writing: false,
    };
    this.inFlight = false;
  }

  async acceptRisk(): Promise<void> {
    const route = this.route;
    if (this.inFlight || route.screen !== 'risk') return;
    this.inFlight = true;
    try {
      this.settings = await this.#backend.setSettings({ ...this.settings, acceptedRiskVersion: RISK_NOTICE_VERSION });
    } catch (e) {
      this.#failLocally(route.then.kind, e);
      return;
    }
    this.inFlight = false;
    await this.startJob(route.then.kind, route.then.options);
  }

  async answer(optionId: string): Promise<void> {
    const flow = this.flow;
    const prompt = flow?.prompt;
    if (this.inFlight || !flow || !prompt) return;
    // Cleared by the next state for this flow (brief: flow://state clears inFlight), or on rejection.
    this.inFlight = true;
    const toRescue = prompt.kind === 'keepWaitingOrRescue' && optionId === 'goToRescue';
    let cable: boolean | null = null;
    if (prompt.kind === 'cableCheck' && optionId === 'have') cable = true;
    if (prompt.kind === 'cableCheckConfirm' && optionId === 'confirmContinue') cable = false;
    try {
      if (toRescue) this.#afterEnd = this.#rescueRoute();
      await this.#backend.answerPrompt(flow.flowId, prompt.id, optionId);
      // The contract doesn't promise the backend ends the flow on goToRescue, so end it ourselves.
      if (toRescue) await this.#backend.cancelFlow(flow.flowId).catch(() => {});
      // Remembering the cable answer is a convenience; failing to save it must not undo the answer.
      if (cable !== null) await this.setHasRescueCable(cable).catch(() => {});
    } catch {
      // A rejected answer leaves the prompt on screen to try again.
      if (toRescue) this.#afterEnd = null;
      this.inFlight = false;
    }
  }

  #rescueRoute(): Route {
    const last = this.lastStart;
    const backupId = last?.options.backupId ?? this.#stockBackupFor(last?.serial ?? null)?.id;
    const backup = this.backups.find((b) => b.id === backupId);
    return { screen: 'rescuePick', preselect: { backupId, slot: backup?.slot } };
  }

  async cancel(): Promise<void> {
    const flow = this.flow;
    if (!flow || flow.writing || isTerminal(flow) || this.inFlight) return;
    this.inFlight = true;
    try {
      await this.#backend.cancelFlow(flow.flowId);
    } catch {
      // Rejected (e.g. a write just began); the flow carries on and stays on screen.
    } finally {
      this.inFlight = false;
    }
  }

  /** Puts the flow away for good: later events for it are ignored. */
  dismissFlow(): void {
    this.#clearFlow();
    this.route = this.#afterEnd ?? { screen: 'home' };
    this.#afterEnd = null;
    void this.#refreshBackups();
  }

  /** Puts a finished flow away and opens Rescue with the matching safety copy picked. */
  goToRescue(): void {
    const route = this.#rescueRoute();
    this.#afterEnd = null;
    this.dismissFlow();
    this.route = route;
  }

  #clearFlow() {
    if (this.flow) this.#dismissed.add(this.flow.flowId);
    this.flow = null;
    this.progress = null;
  }

  async setHasRescueCable(v: boolean): Promise<void> {
    this.settings = await this.#backend.setSettings({ ...this.settings, hasRescueCable: v });
  }

  /** Asks for a boot image file and imports it. Never rejects; a failed file picker goes to Details. */
  async importBackup(): Promise<void> {
    let imagePath: string | null;
    try {
      imagePath = await this.#platform.pickBootImage();
    } catch (e) {
      this.#note(fill(LOG_NOTES.pickFailed, { error: errorText(e) }));
      return;
    }
    if (imagePath) await this.startJob('importBackup', { imagePath });
  }

  /** Shows a backup folder in Finder / Explorer / the file manager. Never rejects; a failure goes to Details. */
  async reveal(path: string): Promise<void> {
    try {
      await this.#backend.revealPath(path);
    } catch (e) {
      this.#note(fill(LOG_NOTES.revealFailed, { path, error: errorText(e) }));
    }
  }

  /**
   * Copies the log. The clipboard write has to start inside the click (WebKit only allows it during
   * the user gesture), so the log is handed over as a promise instead of being awaited first.
   */
  async copyLog(): Promise<boolean> {
    try {
      const text = this.#backend.getLog(this.flow?.flowId);
      await this.#platform.copyText(text);
      return true;
    } catch {
      return false;
    }
  }

  async confirmClose(): Promise<void> {
    await this.#platform.closeWindow();
  }
}
