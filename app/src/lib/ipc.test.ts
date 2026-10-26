import { beforeEach, describe, expect, test, vi } from 'vitest';

const listenHandlers: Record<string, (e: { payload: unknown }) => void> = {};

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, fn: (e: { payload: unknown }) => void) => {
    listenHandlers[name] = fn;
    return () => {};
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createTauriBackend, toFlowError } from './ipc';

beforeEach(() => vi.mocked(invoke).mockClear());

describe('tauri backend', () => {
  test('commands use contract names and arg keys', async () => {
    const b = createTauriBackend();
    await b.getAppInfo();
    expect(invoke).toHaveBeenLastCalledWith('get_app_info');
    await b.getDevice();
    expect(invoke).toHaveBeenLastCalledWith('get_device');
    await b.listBackups('SN1');
    expect(invoke).toHaveBeenLastCalledWith('list_backups', { serial: 'SN1' });
    await b.listBackups(undefined);
    expect(invoke).toHaveBeenLastCalledWith('list_backups', {});
    await b.startFlow('root', {});
    expect(invoke).toHaveBeenLastCalledWith('start_flow', { kind: 'root', options: {} });
    await b.answerPrompt('f', 'p', 'have');
    expect(invoke).toHaveBeenLastCalledWith('answer_prompt', { flowId: 'f', promptId: 'p', optionId: 'have' });
    await b.cancelFlow('f');
    expect(invoke).toHaveBeenLastCalledWith('cancel_flow', { flowId: 'f' });
    await b.getLog();
    expect(invoke).toHaveBeenLastCalledWith('get_log', {});
    await b.getLog('f');
    expect(invoke).toHaveBeenLastCalledWith('get_log', { flowId: 'f' });
    await b.revealPath('/x');
    expect(invoke).toHaveBeenLastCalledWith('reveal_path', { path: '/x' });
    await b.getSettings();
    expect(invoke).toHaveBeenLastCalledWith('get_settings');
    await b.setSettings({ hasRescueCable: true, acceptedRiskVersion: 1 });
    expect(invoke).toHaveBeenLastCalledWith('set_settings', {
      settings: { hasRescueCable: true, acceptedRiskVersion: 1 },
    });
  });

  test('events use contract names and unwrap payload', async () => {
    const b = createTauriBackend();
    const cases: [string, (cb: (x: any) => void) => Promise<unknown>][] = [
      ['device://status', (cb) => b.onDeviceStatus(cb)],
      ['flow://state', (cb) => b.onFlowState(cb)],
      ['flow://progress', (cb) => b.onFlowProgress(cb)],
      ['flow://log', (cb) => b.onFlowLog(cb)],
    ];
    for (const [name, sub] of cases) {
      const cb = vi.fn();
      await sub(cb);
      expect(listen).toHaveBeenCalledWith(name, expect.any(Function));
      listenHandlers[name]({ payload: { flowId: 'x' } });
      expect(cb).toHaveBeenCalledWith({ flowId: 'x' });
    }
  });
});

describe('toFlowError', () => {
  test('passes FlowError through and wraps strings', () => {
    const fe = { code: 'busy', message: 'm', detail: 'd', suggestedAction: null };
    expect(toFlowError(fe)).toEqual(fe);
    expect(toFlowError('boom')).toEqual({
      code: 'internal',
      message: 'Something went wrong.',
      detail: 'boom',
      suggestedAction: null,
    });
  });
});
