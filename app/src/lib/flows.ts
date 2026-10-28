import type { ErrorCode, FlowKind, PromptKind, StepId } from './ipc';

export const ALL_STEP_IDS: StepId[] = [
  'checkDevice', 'enterEdl', 'readPartitions', 'verifyBackup', 'rebootToSystem',
  'pickImage', 'stockCheck', 'chooseSlot', 'saveImport',
  'patch', 'cableCheck', 'write', 'verifyWrite', 'reset', 'waitBoot', 'verifyRoot',
  'cableGuide', 'waitEdl', 'pushScripts', 'done',
];

export const ALL_ERROR_CODES: ErrorCode[] = [
  'noDevice', 'unauthorized', 'unknownModel', 'unknownFirmware', 'lowBattery',
  'alreadyRooted', 'notRooted', 'noStockBackup', 'badImage', 'notStock',
  'edlTimeout', 'edlNotAnswering', 'hashMismatch', 'patchFailed', 'bootTimeout', 'verifyFailed',
  'toolMissing', 'driverMissing', 'busy', 'internal',
];

export const FLOW_KINDS: FlowKind[] = ['backup', 'importBackup', 'root', 'rescue', 'unroot', 'reinstallBootService'];


export const ALL_PROMPT_KINDS: PromptKind[] = [
  'cableCheck',
  'cableCheckConfirm',
  'chooseSlot',
  'keepWaitingOrRescue',
  'installDriver',
  'installUdev',
  'edlNotFound',
  'retryWrite',
  'connectionLostAfterWrite',
  'stillInEdl',
  'tabletStarting',
];

export const FLOW_STEP_COUNT: Record<FlowKind, number> = {
  backup: 4,
  importBackup: 3,
  root: 4,
  rescue: 4,
  unroot: 3,
  reinstallBootService: 2,
};

/** Steps each flow can visit, in order. 'done' is rendered by DoneView for every kind. */
export const FLOW_STEPS: Record<FlowKind, StepId[]> = {
  backup: ['checkDevice', 'enterEdl', 'readPartitions', 'verifyBackup', 'rebootToSystem'],
  importBackup: ['pickImage', 'stockCheck', 'chooseSlot', 'saveImport'],
  root: ['checkDevice', 'enterEdl', 'readPartitions', 'verifyBackup', 'rebootToSystem', 'patch', 'cableCheck', 'write', 'verifyWrite', 'reset', 'waitBoot', 'verifyRoot'],
  rescue: ['pickImage', 'chooseSlot', 'cableGuide', 'waitEdl', 'write', 'verifyWrite', 'reset', 'waitBoot'],
  unroot: ['checkDevice', 'enterEdl', 'write', 'verifyWrite', 'reset', 'waitBoot'],
  reinstallBootService: ['checkDevice', 'pushScripts', 'waitBoot', 'verifyRoot'],
};
