import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { FALLBACK_ERROR_MESSAGE } from './copy';

export type Slot = 'a' | 'b';

export type DeviceStatus =
  | { kind: 'none' }
  | { kind: 'unauthorized' }
  | { kind: 'edl' }
  | { kind: 'adb'; info: DeviceInfo; gate: Gate | null };

export type Gate = 'unknownModel' | 'unknownFirmware' | 'lowBattery';

export interface DeviceInfo {
  serial: string;
  model: string;
  profileId: string | null;
  displayName: string;
  buildId: string;
  displayId: string;
  incremental: string;
  firmwareLabel: string;
  slot: Slot;
  battery: number;
  rooted: boolean;
  magiskVersion: string | null;
  bootServiceRunning: boolean;
}

export interface BackupFile {
  name: string;
  size: number;
  sha256: string;
}
export interface BackupSummary {
  id: string;
  serial: string;
  createdAt: string;
  model: string;
  firmware: string;
  slot: Slot;
  stock: boolean;
  verified: boolean;
  source: 'inkroot' | 'imported';
  path: string;
  files: BackupFile[];
}

export type FlowKind = 'backup' | 'importBackup' | 'root' | 'rescue' | 'unroot' | 'reinstallBootService';

export interface FlowOptions {
  backupId?: string;
  imagePath?: string;
  slot?: Slot;
}

export type StepId =
  | 'checkDevice' | 'enterEdl' | 'readPartitions' | 'verifyBackup' | 'rebootToSystem'
  | 'pickImage' | 'stockCheck' | 'chooseSlot' | 'saveImport'
  | 'patch' | 'cableCheck' | 'write' | 'verifyWrite' | 'reset' | 'waitBoot' | 'verifyRoot'
  | 'cableGuide' | 'waitEdl' | 'pushScripts' | 'done';

export type FlowStatus = 'running' | 'waitingForUser' | 'done' | 'failed' | 'cancelled';

export interface PromptOption {
  id: string;
  label: string;
  emphasis: 'primary' | 'secondary' | 'danger';
}
export interface Prompt {
  id: string;
  kind:
    | 'cableCheck'
    | 'cableCheckConfirm'
    | 'chooseSlot'
    | 'keepWaitingOrRescue'
    | 'installDriver'
    | 'installUdev'
    | 'edlNotFound'
    | 'retryWrite';
  options: PromptOption[];
}

export type ErrorCode =
  | 'noDevice' | 'unauthorized' | 'unknownModel' | 'unknownFirmware' | 'lowBattery'
  | 'alreadyRooted' | 'notRooted' | 'noStockBackup' | 'badImage' | 'notStock'
  | 'edlTimeout' | 'hashMismatch' | 'patchFailed' | 'bootTimeout' | 'verifyFailed'
  | 'toolMissing' | 'driverMissing' | 'busy' | 'internal';

export interface FlowError {
  code: ErrorCode;
  message: string;
  detail: string;
  suggestedAction: 'retry' | 'goToRescue' | 'reconnect' | 'chargeDevice' | 'importBackup' | null;
}

export interface FlowResult {
  backup?: BackupSummary;
  magiskVersion?: string;
}

export interface FlowState {
  flowId: string;
  kind: FlowKind;
  step: StepId;
  stepIndex: number;
  stepCount: number;
  status: FlowStatus;
  prompt: Prompt | null;
  error: FlowError | null;
  result: FlowResult | null;
  writing: boolean;
}

export interface Progress {
  flowId: string;
  done: number;
  total: number;
  label: string;
}
export interface LogLine {
  flowId: string | null;
  ts: string;
  stream: 'stdout' | 'stderr' | 'info';
  line: string;
}

export interface AppInfo {
  version: string;
  demo: boolean;
  os: 'macos' | 'windows' | 'linux';
  backupRoot: string;
}

export type PromptKind = Prompt['kind'];
export interface Settings {
  hasRescueCable: boolean | null;
  acceptedRiskVersion: number | null;
}
export type Unlisten = () => void;

export interface Backend {
  getAppInfo(): Promise<AppInfo>;
  getDevice(): Promise<DeviceStatus>;
  listBackups(serial?: string): Promise<BackupSummary[]>;
  startFlow(kind: FlowKind, options: FlowOptions): Promise<string>;
  answerPrompt(flowId: string, promptId: string, optionId: string): Promise<void>;
  cancelFlow(flowId: string): Promise<void>;
  getLog(flowId?: string): Promise<string>;
  revealPath(path: string): Promise<void>;
  getSettings(): Promise<Settings>;
  setSettings(settings: Settings): Promise<Settings>;
  onDeviceStatus(cb: (s: DeviceStatus) => void): Promise<Unlisten>;
  onFlowState(cb: (s: FlowState) => void): Promise<Unlisten>;
  onFlowProgress(cb: (p: Progress) => void): Promise<Unlisten>;
  onFlowLog(cb: (l: LogLine) => void): Promise<Unlisten>;
}

function subscribe<T>(name: string, cb: (payload: T) => void): Promise<Unlisten> {
  return listen<T>(name, (event) => cb(event.payload));
}

export function createTauriBackend(): Backend {
  return {
    getAppInfo: () => invoke<AppInfo>('get_app_info'),
    getDevice: () => invoke<DeviceStatus>('get_device'),
    listBackups: (serial) => invoke<BackupSummary[]>('list_backups', serial === undefined ? {} : { serial }),
    startFlow: (kind, options) => invoke<string>('start_flow', { kind, options }),
    answerPrompt: (flowId, promptId, optionId) => invoke<void>('answer_prompt', { flowId, promptId, optionId }),
    cancelFlow: (flowId) => invoke<void>('cancel_flow', { flowId }),
    getLog: (flowId) => invoke<string>('get_log', flowId === undefined ? {} : { flowId }),
    revealPath: (path) => invoke<void>('reveal_path', { path }),
    getSettings: () => invoke<Settings>('get_settings'),
    setSettings: (settings) => invoke<Settings>('set_settings', { settings }),
    onDeviceStatus: (cb) => subscribe<DeviceStatus>('device://status', cb),
    onFlowState: (cb) => subscribe<FlowState>('flow://state', cb),
    onFlowProgress: (cb) => subscribe<Progress>('flow://progress', cb),
    onFlowLog: (cb) => subscribe<LogLine>('flow://log', cb),
  };
}

export function toFlowError(e: unknown): FlowError {
  if (
    typeof e === 'object' &&
    e !== null &&
    typeof (e as FlowError).code === 'string' &&
    typeof (e as FlowError).message === 'string' &&
    typeof (e as FlowError).detail === 'string' &&
    'suggestedAction' in e
  ) {
    return e as FlowError;
  }
  return { code: 'internal', message: FALLBACK_ERROR_MESSAGE, detail: String(e), suggestedAction: null };
}
