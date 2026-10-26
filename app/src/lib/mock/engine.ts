import { FLOW_STEP_COUNT } from '../flows';
import type {
  AppInfo, Backend, BackupSummary, DeviceStatus, FlowError, FlowKind, FlowOptions, FlowResult,
  FlowState, LogLine, Progress, Prompt, Settings, Unlisten,
} from '../ipc';
import { RISK_NOTICE_VERSION } from '../legal';
import type { Platform } from '../platform';

/** A prompt branch: beats to run on an answer. A function is built lazily so a branch can re-ask (e.g. "back"). */
export type Branch = Beat[] | (() => Beat[]);
export type Beat =
  | { wait: number }
  | { state: Partial<FlowState> }
  | { progress: Omit<Progress, 'flowId'> }
  | { log: string; stream?: LogLine['stream'] }
  | { device: DeviceStatus }
  | { prompt: Omit<Prompt, 'id'>; branches: Record<string, Branch> }
  | { fail: Omit<FlowError, 'detail'> & { detail?: string } }
  | { done: FlowResult }
  | { cancelled: true }; // the user backed out at a prompt: status -> cancelled, ends script

export interface Scenario {
  name: string;
  description: string;
  appInfo?: Partial<AppInfo>;
  device: DeviceStatus;
  deviceScript?: Beat[];
  backups: BackupSummary[];
  settings?: Partial<Settings>;
  flows: Partial<Record<FlowKind, Beat[]>>;
  startErrors?: Partial<Record<FlowKind, FlowError>>;
  pickFile?: string | null;
  frozen?: { flow: FlowState; progress?: Progress; logs?: string[] };
}

// Mirrors the real backend, which gates only these three (the UI also gates reinstallBootService).
const BACKEND_RISK_GATED: FlowKind[] = ['root', 'unroot', 'rescue'];
const DEFAULT_PICK = '/Users/you/Downloads/boot_b.stock.img';
const rejection = (code: FlowError['code'], message: string): FlowError => ({ code, message, detail: '', suggestedAction: null });
const isTerminal = (s: FlowState) => s.status === 'done' || s.status === 'failed' || s.status === 'cancelled';

interface Frame { beats: Beat[]; i: number }
interface Pending { promptId: string; branches: Record<string, Branch> }

export function createMockBackend(s: Scenario, opts: { speed?: number } = {}): Backend {
  const speed = opts.speed && opts.speed > 0 ? opts.speed : 1;
  const info: AppInfo = { version: '0.1.0-mock', demo: true, os: 'macos', backupRoot: '/Users/you/Documents/Inkroot', ...s.appInfo };
  let settings: Settings = { hasRescueCable: null, acceptedRiskVersion: null, ...s.settings };
  let device: DeviceStatus = s.device;
  let backups = [...s.backups];
  const logs: LogLine[] = [];
  let state: FlowState | null = null;
  let flowCount = 0;
  let promptCount = 0;
  let frames: Frame[] = [];
  let pending: Pending | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let connected = false;

  const subs = {
    device: new Set<(d: DeviceStatus) => void>(),
    state: new Set<(f: FlowState) => void>(),
    progress: new Set<(p: Progress) => void>(),
    log: new Set<(l: LogLine) => void>(),
  };
  const emit = <T>(set: Set<(v: T) => void>, v: T) => set.forEach((cb) => cb(v));
  const sleep = (ms: number, fn: () => void) => setTimeout(fn, ms / speed);

  function apply(patch: Partial<FlowState>) {
    const next = { ...state!, ...patch };
    // prompt/error/result only exist while the status says so.
    if (next.status !== 'waitingForUser') next.prompt = null;
    if (next.status !== 'failed') next.error = null;
    if (next.status !== 'done') next.result = null;
    state = next;
    emit(subs.state, { ...next });
  }
  function log(line: string, stream: LogLine['stream'] = 'stdout', flowId: string | null = state?.flowId ?? null) {
    const l: LogLine = { flowId, ts: new Date().toISOString(), stream, line };
    logs.push(l);
    emit(subs.log, l);
  }
  function setDevice(d: DeviceStatus) {
    device = d;
    emit(subs.device, d);
  }

  function finish() {
    frames = [];
    pending = null;
  }
  function upsertBackup(b: BackupSummary) {
    backups = [...backups.filter((x) => x.id !== b.id), b];
  }

  /** Runs beats until one waits, prompts or ends the script. */
  function pump() {
    timer = null;
    while (frames.length) {
      const frame = frames[frames.length - 1];
      if (frame.i >= frame.beats.length) {
        frames.pop();
        continue;
      }
      const beat = frame.beats[frame.i++];
      if ('wait' in beat) {
        timer = sleep(beat.wait, pump);
        return;
      } else if ('state' in beat) apply(beat.state);
      else if ('progress' in beat) emit(subs.progress, { flowId: state!.flowId, ...beat.progress });
      else if ('log' in beat) log(beat.log, beat.stream);
      else if ('device' in beat) setDevice(beat.device);
      else if ('prompt' in beat) {
        const prompt: Prompt = { ...beat.prompt, id: `prompt-${++promptCount}` };
        pending = { promptId: prompt.id, branches: beat.branches };
        apply({ status: 'waitingForUser', prompt });
        return;
      } else if ('fail' in beat) {
        apply({ status: 'failed', writing: false, error: { detail: '', ...beat.fail } });
        return finish();
      } else if ('done' in beat) {
        if (beat.done.backup) upsertBackup(beat.done.backup);
        apply({ status: 'done', step: 'done', stepIndex: state!.stepCount, writing: false, result: beat.done });
        return finish();
      } else {
        apply({ status: 'cancelled', writing: false });
        return finish();
      }
    }
  }

  function connect() {
    if (connected) return;
    connected = true;
    setTimeout(() => {
      if (s.frozen) {
        const { flow, progress, logs: lines } = s.frozen;
        state = flow;
        emit(subs.state, { ...flow });
        lines?.forEach((l) => log(l, 'stdout', flow.flowId));
        if (progress) emit(subs.progress, progress);
      }
      if (s.deviceScript) runDeviceScript(s.deviceScript);
    }, 0);
  }
  function runDeviceScript(beats: Beat[]) {
    const rest = [...beats];
    const step = () => {
      while (rest.length) {
        const b = rest.shift()!;
        if ('wait' in b) return void sleep(b.wait, step);
        if ('device' in b) setDevice(b.device);
        else if ('log' in b) log(b.log, b.stream, null);
      }
    };
    step();
  }

  async function subscribe<T>(set: Set<(v: T) => void>, cb: (v: T) => void): Promise<Unlisten> {
    set.add(cb);
    connect();
    return () => void set.delete(cb);
  }

  return {
    async getAppInfo() {
      return { ...info };
    },
    async getDevice() {
      connect();
      return device;
    },
    async listBackups(serial) {
      return backups
        .filter((b) => serial === undefined || b.serial === serial)
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    },
    async startFlow(kind, options: FlowOptions) {
      void options;
      const startError = s.startErrors?.[kind];
      if (startError) throw startError;
      if (state && !isTerminal(state)) throw rejection('busy', 'Inkroot is already working on something.');
      if (BACKEND_RISK_GATED.includes(kind) && settings.acceptedRiskVersion !== RISK_NOTICE_VERSION) {
        throw rejection('internal', 'Please read and accept the risk notice first.');
      }
      const flowId = `mock-${++flowCount}`;
      state = {
        flowId, kind, step: 'checkDevice', stepIndex: 1, stepCount: FLOW_STEP_COUNT[kind],
        status: 'running', prompt: null, error: null, result: null, writing: false,
      };
      emit(subs.state, { ...state });
      frames = [{ beats: s.flows[kind] ?? [{ done: {} }], i: 0 }];
      pending = null;
      timer = sleep(0, pump);
      return flowId;
    },
    async answerPrompt(flowId, promptId, optionId) {
      if (!state || state.flowId !== flowId || state.status !== 'waitingForUser' || !pending || pending.promptId !== promptId) return;
      if (!state.prompt?.options.some((o) => o.id === optionId)) return;
      const branch = pending.branches[optionId];
      pending = null;
      apply({ status: 'running' });
      if (branch) frames.push({ beats: typeof branch === 'function' ? branch() : branch, i: 0 });
      timer = sleep(0, pump);
    },
    async cancelFlow() {
      if (state?.writing) {
        throw rejection('internal', 'Please wait until the tablet has finished saving.');
      }
      if (!state || isTerminal(state)) return;
      if (timer) clearTimeout(timer);
      timer = null;
      finish();
      apply({ status: 'cancelled' });
    },
    async getLog(flowId) {
      return logs
        .filter((l) => flowId === undefined || l.flowId === flowId)
        .map((l) => `${l.ts} [${l.stream}] ${l.line}`)
        .join('\n');
    },
    async revealPath() {},
    async getSettings() {
      return { ...settings };
    },
    async setSettings(next) {
      settings = { ...next };
      return { ...settings };
    },
    onDeviceStatus: (cb) => subscribe(subs.device, cb),
    onFlowState: (cb) => subscribe(subs.state, cb),
    onFlowProgress: (cb) => subscribe(subs.progress, cb),
    onFlowLog: (cb) => subscribe(subs.log, cb),
  };
}

export function createMockPlatform(s: Scenario): Platform {
  return {
    async pickBootImage() {
      return 'pickFile' in s ? (s.pickFile ?? null) : DEFAULT_PICK;
    },
    async openExternal(url) {
      window.open(url, '_blank');
    },
    async onCloseRequested(allow) {
      const handler = (e: BeforeUnloadEvent) => {
        if (!allow()) {
          e.preventDefault();
          e.returnValue = '';
        }
      };
      window.addEventListener('beforeunload', handler);
      return () => window.removeEventListener('beforeunload', handler);
    },
    async closeWindow() {},
    async copyText(text) {
      // A text that can't be fetched is a failed copy; a missing clipboard is not.
      const value = await text;
      try {
        await navigator.clipboard?.writeText(value);
      } catch {
        // No clipboard in this browser; the mock still resolves.
      }
    },
  };
}
