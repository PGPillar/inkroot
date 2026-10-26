import type { FlowKind } from './ipc';

export const RISK_NOTICE_VERSION = 1;
export const RISK_REQUIRED: FlowKind[] = ['root', 'unroot', 'rescue', 'reinstallBootService'];
