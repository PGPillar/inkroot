import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { DeviceStatus, FlowState, LogLine, Progress, PromptKind } from './ipc';
import { createMockBackend, createMockPlatform, type Scenario } from './mock/engine';
import { IMPORTED_RESCUE_ONLY, ROOTED_BACKUP, STOCK_BACKUP } from './mock/fixtures';
import { SCENARIOS } from './mock/scenarios';
import { AppState } from './state.svelte';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

async function setup(scenario: string | Scenario) {
  const s = typeof scenario === 'string' ? SCENARIOS[scenario] : scenario;
  const backend = createMockBackend(s, { speed: 1 });
  const platform = createMockPlatform(s);
  // Capture the listeners so tests can inject late or foreign events.
  const emit: {
    state?: (f: FlowState) => void;
    progress?: (p: Progress) => void;
    log?: (l: LogLine) => void;
  } = {};
  const { onFlowState, onFlowProgress, onFlowLog } = backend;
  backend.onFlowState = (cb) => ((emit.state = cb), onFlowState(cb));
  backend.onFlowProgress = (cb) => ((emit.progress = cb), onFlowProgress(cb));
  backend.onFlowLog = (cb) => ((emit.log = cb), onFlowLog(cb));
  let allowClose: (() => boolean) | null = null;
  const closeSpy = vi.spyOn(platform, 'onCloseRequested').mockImplementation(async (allow) => {
    allowClose = allow;
    return () => {};
  });
  const app = new AppState(backend, platform);
  const unlisten = await app.connect();
  await vi.advanceTimersByTimeAsync(1);
  return { app, backend, platform, emit, closeSpy, unlisten, allowClose: () => allowClose! };
}

/** Advances fake time until the predicate holds (or fails after two minutes). */
async function until(pred: () => boolean, what: string) {
  for (let t = 0; t < 240; t++) {
    if (pred()) return;
    await vi.advanceTimersByTimeAsync(500);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const promptIs = (app: AppState, kind: PromptKind) => () => app.flow?.prompt?.kind === kind;

describe('connect', () => {
  test('loads info, device, settings and backups', async () => {
    const { app } = await setup('ready');
    expect(app.info?.demo).toBe(true);
    expect(app.device.kind).toBe('adb');
    expect(app.settings).toEqual({ hasRescueCable: null, acceptedRiskVersion: 1 });
    expect(app.backups).toEqual([STOCK_BACKUP]);
    expect(app.screen).toBe('home');
  });

  test('picks up a flow already running in the backend', async () => {
    const { app } = await setup('shot-root-writing');
    expect(app.flow?.flowId).toBe('mock-shot');
    expect(app.progress?.done).toBe(61865984);
    expect(app.screen).toBe('flow');
  });

  test('stockBackup is the newest verified stock backup for the connected tablet', async () => {
    const newer = { ...STOCK_BACKUP, id: 'NM24B0A1C3/2026-10-27_0800', createdAt: '2026-10-27T08:00:00Z' };
    const other = { ...STOCK_BACKUP, id: 'OTHER/2026-10-28_0800', serial: 'OTHER', createdAt: '2026-10-28T08:00:00Z' };
    const { app } = await setup({ ...SCENARIOS.ready, backups: [STOCK_BACKUP, ROOTED_BACKUP, IMPORTED_RESCUE_ONLY, newer, other] });
    expect(app.stockBackup?.id).toBe(newer.id);
    const { app: rooted } = await setup('rooted-no-stock');
    expect(rooted.stockBackup).toBeNull();
    const { app: none } = await setup('no-device');
    expect(none.deviceInfo).toBeNull();
    expect(none.stockBackup).toBeNull();
  });

  test('device events update the screen', async () => {
    const { app } = await setup('plug-in');
    expect(app.screen).toBe('gate-none');
    app.navigate({ screen: 'usbGuide' });
    expect(app.screen).toBe('usbGuide');
    await vi.advanceTimersByTimeAsync(4000);
    expect(app.device.kind).toBe('unauthorized');
    expect(app.screen).toBe('usbGuide');
    await vi.advanceTimersByTimeAsync(4000);
    expect(app.screen).toBe('home');
  });

  test('logs are capped at 5000, oldest dropped', async () => {
    const { app, emit } = await setup('ready');
    for (let i = 0; i < 5005; i++) emit.log!({ flowId: null, ts: '', stream: 'info', line: `line ${i}` });
    expect(app.logs).toHaveLength(5000);
    expect(app.logs[0].line).toBe('line 5');
    expect(app.logs[4999].line).toBe('line 5004');
  });
});

describe('startJob and risk acknowledgement', () => {
  test('root without acceptance routes to risk and does not call backend', async () => {
    const { app, backend } = await setup('no-backup');
    const start = vi.spyOn(backend, 'startFlow');
    await app.startJob('root', { backupId: 'x' });
    expect(start).not.toHaveBeenCalled();
    expect(app.route).toEqual({ screen: 'risk', then: { kind: 'root', options: { backupId: 'x' } } });
    expect(app.screen).toBe('risk');
  });

  test.each(['unroot', 'rescue', 'reinstallBootService'] as const)('%s also needs the acknowledgement', async (kind) => {
    const { app, backend } = await setup('no-backup');
    const start = vi.spyOn(backend, 'startFlow');
    await app.startJob(kind);
    expect(start).not.toHaveBeenCalled();
    expect(app.route.screen).toBe('risk');
  });

  test('backup does not need the acknowledgement', async () => {
    const { app } = await setup('no-backup');
    await app.startJob('backup');
    expect(app.flow?.kind).toBe('backup');
    expect(app.screen).toBe('flow');
  });

  test('acceptRisk writes version 1 then starts the pending flow', async () => {
    const { app, backend } = await setup('no-backup');
    const set = vi.spyOn(backend, 'setSettings');
    const start = vi.spyOn(backend, 'startFlow');
    await app.startJob('root', {});
    await app.acceptRisk();
    expect(set).toHaveBeenCalledWith({ hasRescueCable: null, acceptedRiskVersion: 1 });
    expect(start).toHaveBeenCalledWith('root', {});
    expect(app.settings.acceptedRiskVersion).toBe(1);
    expect(app.flow?.kind).toBe('root');
    expect(app.flow?.status).toBe('running');
    expect(app.screen).toBe('flow');
  });

  test('start rejection becomes a failed flow', async () => {
    const { app } = await setup('root-busy');
    await app.startJob('root', {});
    expect(app.flow).toMatchObject({
      kind: 'root', step: 'checkDevice', stepIndex: 1, stepCount: 1, status: 'failed',
      prompt: null, result: null, writing: false,
      error: { code: 'busy', message: 'Inkroot is already working on something. Please wait for it to finish.' },
    });
    expect(app.flow?.flowId).toMatch(/^local-\d+$/);
    expect(app.inFlight).toBe(false);
    expect(app.screen).toBe('flow');
  });

  test('a non-FlowError rejection still becomes a failed flow', async () => {
    const { app, backend } = await setup('ready');
    vi.spyOn(backend, 'startFlow').mockRejectedValue(new Error('ipc down'));
    await app.startJob('backup');
    expect(app.flow?.status).toBe('failed');
    expect(app.flow?.error?.code).toBe('internal');
    expect(app.flow?.error?.detail).toContain('ipc down');
  });

  test('remembers the last start for Try again', async () => {
    const { app } = await setup('ready');
    await app.startJob('root', { backupId: STOCK_BACKUP.id });
    expect(app.lastStart).toEqual({ kind: 'root', options: { backupId: STOCK_BACKUP.id }, serial: 'NM24B0A1C3' });
  });

  test('does not start a second job while one is active', async () => {
    const { app, backend } = await setup('ready');
    await app.startJob('backup');
    const start = vi.spyOn(backend, 'startFlow');
    await app.startJob('backup');
    expect(start).not.toHaveBeenCalled();
    expect(app.flow?.status).toBe('running');
  });

  test('Try again after a failure shows the new flow', async () => {
    const { app } = await setup('root-patch-failed');
    await app.startJob('root', {});
    await until(() => app.flow?.status === 'failed', 'failure');
    const first = app.flow!.flowId;
    await app.startJob('root', {});
    expect(app.flow?.flowId).not.toBe(first);
    expect(app.flow?.status).toBe('running');
  });
});

describe('flow events', () => {
  test('ignores events for dismissed flows', async () => {
    const { app, emit, backend } = await setup('ready');
    await app.startJob('backup');
    await until(() => app.flow?.status === 'done', 'backup done');
    const last = { ...app.flow! };
    const list = vi.spyOn(backend, 'listBackups');
    app.dismissFlow();
    expect(app.flow).toBeNull();
    expect(app.progress).toBeNull();
    expect(app.route).toEqual({ screen: 'home' });
    expect(list).toHaveBeenCalled();
    emit.state!(last);
    emit.state!({ ...last, status: 'running', step: 'readPartitions' });
    expect(app.flow).toBeNull();
    expect(app.screen).toBe('home');
  });

  test('ignores state for a foreign flow while one is on screen', async () => {
    const { app, emit } = await setup('ready');
    await app.startJob('backup');
    const mine = app.flow!.flowId;
    emit.state!({ ...{ ...app.flow! }, flowId: 'other', status: 'failed' });
    expect(app.flow?.flowId).toBe(mine);
    expect(app.flow?.status).toBe('running');
  });

  test('ignores progress for other flowId', async () => {
    const { app, emit } = await setup('ready');
    await app.startJob('backup');
    emit.progress!({ flowId: 'other', done: 1, total: 2, label: 'x' });
    expect(app.progress).toBeNull();
    emit.progress!({ flowId: app.flow!.flowId, done: 1, total: 2, label: 'x' });
    expect(app.progress).toEqual({ flowId: app.flow!.flowId, done: 1, total: 2, label: 'x' });
  });

  test('progress resets when the step changes', async () => {
    const { app } = await setup('ready');
    await app.startJob('backup');
    await until(() => app.progress !== null, 'progress');
    expect(app.flow?.step).toBe('readPartitions');
    await until(() => app.flow?.step === 'verifyBackup', 'verifyBackup');
    expect(app.progress).toBeNull();
  });

  test('a terminal state refreshes backups', async () => {
    const { app } = await setup('no-backup');
    expect(app.backups).toEqual([]);
    await app.startJob('backup');
    await until(() => app.flow?.status === 'done', 'backup done');
    await vi.advanceTimersByTimeAsync(1);
    expect(app.backups).toHaveLength(1);
  });

  test('a flow outranks device gates when the tablet drops off mid-flow', async () => {
    const { app } = await setup('ready');
    await app.startJob('backup');
    await until(() => app.device.kind === 'none', 'device drop');
    expect(app.screen).toBe('flow');
  });
});

describe('answer and cancel', () => {
  test('answers the current prompt once even when clicked twice', async () => {
    const { app, backend } = await setup('ready');
    await app.startJob('root', {});
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    const answer = vi.spyOn(backend, 'answerPrompt');
    const promptId = app.flow!.prompt!.id;
    await Promise.all([app.answer('have'), app.answer('have')]);
    expect(answer).toHaveBeenCalledTimes(1);
    expect(answer).toHaveBeenCalledWith(app.flow!.flowId, promptId, 'have');
    expect(app.inFlight).toBe(false);
  });

  test('answer without a prompt does nothing', async () => {
    const { app, backend } = await setup('ready');
    await app.startJob('backup');
    const answer = vi.spyOn(backend, 'answerPrompt');
    await app.answer('have');
    expect(answer).not.toHaveBeenCalled();
  });

  test('cable answers persist hasRescueCable', async () => {
    const { app } = await setup('ready');
    await app.startJob('root', {});
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    await app.answer('have');
    expect(app.settings.hasRescueCable).toBe(true);
  });

  test('confirming "continue without" persists hasRescueCable false', async () => {
    const { app } = await setup('ready');
    await app.startJob('root', {});
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    await app.answer('continueWithout');
    expect(app.settings.hasRescueCable).toBeNull();
    await until(promptIs(app, 'cableCheckConfirm'), 'cableCheckConfirm');
    await app.answer('confirmContinue');
    expect(app.settings.hasRescueCable).toBe(false);
  });

  test('goToRescue routes to rescuePick with backup and slot preselected', async () => {
    const { app, backend } = await setup('root-boot-timeout');
    await app.startJob('root', {});
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    await app.answer('have');
    await until(promptIs(app, 'keepWaitingOrRescue'), 'keepWaitingOrRescue');
    const cancel = vi.spyOn(backend, 'cancelFlow');
    await app.answer('goToRescue');
    await vi.advanceTimersByTimeAsync(10);
    expect(cancel).toHaveBeenCalled();
    expect(app.flow).toBeNull();
    expect(app.route).toEqual({ screen: 'rescuePick', preselect: { backupId: STOCK_BACKUP.id, slot: 'b' } });
    expect(app.screen).toBe('rescuePick');
  });

  test('goToRescue prefers the backup the flow was started with', async () => {
    const chosen = { ...STOCK_BACKUP, id: 'NM24B0A1C3/2026-10-20_0900', createdAt: '2026-10-20T09:00:00Z', slot: 'a' as const };
    const { app } = await setup({ ...SCENARIOS['root-boot-timeout'], backups: [STOCK_BACKUP, chosen] });
    await app.startJob('root', { backupId: chosen.id });
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    await app.answer('have');
    await until(promptIs(app, 'keepWaitingOrRescue'), 'keepWaitingOrRescue');
    await app.answer('goToRescue');
    await vi.advanceTimersByTimeAsync(10);
    expect(app.route).toEqual({ screen: 'rescuePick', preselect: { backupId: chosen.id, slot: 'a' } });
  });

  test('goToRescue ignores a cancel rejection', async () => {
    const { app, backend } = await setup('root-boot-timeout');
    await app.startJob('root', {});
    await until(promptIs(app, 'cableCheck'), 'cableCheck');
    await app.answer('have');
    await until(promptIs(app, 'keepWaitingOrRescue'), 'keepWaitingOrRescue');
    vi.spyOn(backend, 'cancelFlow').mockRejectedValue({ code: 'internal', message: 'no', detail: '', suggestedAction: null });
    await expect(app.answer('goToRescue')).resolves.toBeUndefined();
    await vi.advanceTimersByTimeAsync(10);
    expect(app.screen).toBe('rescuePick');
  });

  test('cancel is a no-op while writing', async () => {
    const { app, backend } = await setup('shot-root-writing');
    const cancel = vi.spyOn(backend, 'cancelFlow');
    await app.cancel();
    expect(cancel).not.toHaveBeenCalled();
  });

  test('cancel stops a running flow', async () => {
    const { app, backend } = await setup('ready');
    await app.startJob('backup');
    const cancel = vi.spyOn(backend, 'cancelFlow');
    await app.cancel();
    expect(cancel).toHaveBeenCalledWith(app.flow!.flowId);
    expect(app.flow?.status).toBe('cancelled');
    expect(app.screen).toBe('flow');
  });
});

describe('close guard, import, copy log', () => {
  test('close is blocked while writing and allowed otherwise', async () => {
    const writing = await setup('shot-root-writing');
    expect(writing.closeSpy).toHaveBeenCalledTimes(1);
    expect(writing.allowClose()()).toBe(false);
    expect(writing.app.closeBlocked).toBe(true);

    const done = await setup('shot-root-done');
    expect(done.allowClose()()).toBe(true);
    expect(done.app.closeBlocked).toBe(false);

    const idle = await setup('ready');
    expect(idle.allowClose()()).toBe(true);
  });

  test('confirmClose closes the window', async () => {
    const { app, platform } = await setup('shot-root-writing');
    const close = vi.spyOn(platform, 'closeWindow');
    await app.confirmClose();
    expect(close).toHaveBeenCalled();
  });

  test('importBackup: cancelled dialog starts nothing', async () => {
    const { app, backend } = await setup({ ...SCENARIOS.ready, pickFile: null });
    const start = vi.spyOn(backend, 'startFlow');
    await app.importBackup();
    expect(start).not.toHaveBeenCalled();
    expect(app.flow).toBeNull();
  });

  test('importBackup starts an import with the picked path', async () => {
    const { app, backend } = await setup('ready');
    const start = vi.spyOn(backend, 'startFlow');
    await app.importBackup();
    expect(start).toHaveBeenCalledWith('importBackup', { imagePath: '/Users/you/Downloads/boot_b.stock.img' });
    expect(app.flow?.kind).toBe('importBackup');
  });

  test('copyLog starts the copy inside the click, handing over the log still being fetched', async () => {
    const { app, platform } = await setup('shot-root-writing');
    const copy = vi.spyOn(platform, 'copyText');
    const done = app.copyLog();
    // Called synchronously, before getLog has resolved, so the clipboard write stays in the gesture.
    expect(copy).toHaveBeenCalledOnce();
    const handed = copy.mock.calls[0][0];
    expect(handed).toBeInstanceOf(Promise);
    expect(await done).toBe(true);
    expect(await handed).toContain('edl --memory=ufs w boot_b patched-boot.img');
  });

  test('copyLog reports failure', async () => {
    const { app, platform } = await setup('ready');
    vi.spyOn(platform, 'copyText').mockRejectedValue(new Error('denied'));
    expect(await app.copyLog()).toBe(false);
  });

  test('copyLog reports failure when the log cannot be fetched', async () => {
    const { app, backend } = await setup('ready');
    vi.spyOn(backend, 'getLog').mockRejectedValue(new Error('gone'));
    expect(await app.copyLog()).toBe(false);
  });

  test('unlisten stops event handling', async () => {
    const { app, unlisten } = await setup('plug-in');
    unlisten();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(app.device.kind).toBe('none');
  });
});

describe('real-backend ordering: start_flow resolves before the first state', () => {
  const late = (flowId: string, o: Partial<FlowState> = {}): FlowState => ({
    flowId, kind: 'root', step: 'write', stepIndex: 3, stepCount: 4, status: 'running',
    prompt: null, error: null, result: null, writing: true, ...o,
  });

  test('Try again with a failed flow on screen adopts the new flow', async () => {
    const { app, backend, emit, allowClose } = await setup('ready');
    const start = vi.spyOn(backend, 'startFlow')
      .mockRejectedValueOnce({ code: 'busy', message: 'busy', detail: '', suggestedAction: null })
      .mockResolvedValueOnce('real-1');
    await app.startJob('root', {});
    expect(app.flow?.status).toBe('failed');
    await app.startJob('root', {});
    expect(start).toHaveBeenCalledTimes(2);
    expect(app.inFlight).toBe(true);
    emit.state!(late('real-1'));
    expect(app.flow?.flowId).toBe('real-1');
    expect(app.inFlight).toBe(false);
    expect(allowClose()()).toBe(false);
    emit.progress!({ flowId: 'real-1', done: 1, total: 2, label: 'boot_b' });
    expect(app.progress?.label).toBe('boot_b');
    const cancel = vi.spyOn(backend, 'cancelFlow');
    emit.state!(late('real-1', { writing: false }));
    await app.cancel();
    expect(cancel).toHaveBeenCalledWith('real-1');
  });

  test('a second start before the first state arrives is ignored', async () => {
    const { app, backend, emit, allowClose } = await setup('ready');
    const start = vi.spyOn(backend, 'startFlow').mockResolvedValue('real-1');
    await app.startJob('root', {});
    expect(app.flow).toBeNull();
    expect(app.inFlight).toBe(true);
    await app.startJob('root', {});
    expect(start).toHaveBeenCalledTimes(1);
    expect(app.flow).toBeNull();
    emit.state!(late('real-1'));
    expect(app.flow?.flowId).toBe('real-1');
    expect(app.inFlight).toBe(false);
    expect(allowClose()()).toBe(false);
  });

  test('answer keeps inFlight until the next state; a rejection clears it', async () => {
    const { app, backend, emit } = await setup('ready');
    vi.spyOn(backend, 'startFlow').mockResolvedValue('real-1');
    await app.startJob('root', {});
    const prompt = { id: 'p1', kind: 'retryWrite' as const, options: [{ id: 'retry', label: 'Retry', emphasis: 'primary' as const }] };
    emit.state!(late('real-1', { status: 'waitingForUser', writing: false, prompt }));
    const answer = vi.spyOn(backend, 'answerPrompt').mockResolvedValue();
    await app.answer('retry');
    expect(answer).toHaveBeenCalledWith('real-1', 'p1', 'retry');
    expect(app.inFlight).toBe(true);
    await app.answer('retry');
    expect(answer).toHaveBeenCalledTimes(1);
    emit.state!(late('real-1', { status: 'running' }));
    expect(app.inFlight).toBe(false);

    emit.state!(late('real-1', { status: 'waitingForUser', writing: false, prompt: { ...prompt, id: 'p2' } }));
    answer.mockRejectedValueOnce(new Error('nope'));
    await app.answer('retry');
    expect(app.inFlight).toBe(false);
    expect(app.flow?.prompt?.id).toBe('p2');
  });
});

describe('acceptRisk guards', () => {
  test('a rejected setSettings becomes a failed flow', async () => {
    const { app, backend } = await setup('no-backup');
    await app.startJob('root', {});
    vi.spyOn(backend, 'setSettings').mockRejectedValue(new Error('disk full'));
    const start = vi.spyOn(backend, 'startFlow');
    await expect(app.acceptRisk()).resolves.toBeUndefined();
    expect(start).not.toHaveBeenCalled();
    expect(app.flow).toMatchObject({ kind: 'root', status: 'failed', error: { code: 'internal' } });
    expect(app.flow?.error?.detail).toContain('disk full');
    expect(app.inFlight).toBe(false);
    expect(app.screen).toBe('flow');
  });

  test('a double click writes settings and starts once', async () => {
    const { app, backend } = await setup('no-backup');
    await app.startJob('root', {});
    const set = vi.spyOn(backend, 'setSettings');
    const start = vi.spyOn(backend, 'startFlow');
    await Promise.all([app.acceptRisk(), app.acceptRisk()]);
    expect(set).toHaveBeenCalledTimes(1);
    expect(start).toHaveBeenCalledTimes(1);
    expect(app.flow?.kind).toBe('root');
  });
});

describe('things outside the backend failing', () => {
  test('a link the browser cannot open goes to Details and flags the link', async () => {
    const { app, platform } = await setup('ready');
    vi.spyOn(platform, 'openExternal').mockRejectedValue(new Error('not allowed by scope'));
    await app.openExternal('https://example.org/x');
    expect(app.openFailed).toBe('https://example.org/x');
    const last = app.logs.at(-1)!;
    expect(last.stream).toBe('info');
    expect(last.line).toContain('https://example.org/x');
    expect(last.line).toContain('not allowed by scope');
    app.navigate({ screen: 'about' });
    expect(app.openFailed).toBeNull();
  });

  test('a link that opens clears the flag', async () => {
    const { app, platform } = await setup('ready');
    const open = vi.spyOn(platform, 'openExternal').mockRejectedValueOnce(new Error('no'));
    await app.openExternal('https://example.org/x');
    open.mockResolvedValue();
    await app.openExternal('https://example.org/x');
    expect(app.openFailed).toBeNull();
  });

  test('reveal and the file picker never reject; failures go to Details', async () => {
    const { app, backend, platform } = await setup('ready');
    vi.spyOn(backend, 'revealPath').mockRejectedValue(new Error('no such folder'));
    vi.spyOn(platform, 'pickBootImage').mockRejectedValue(new Error('dialog broke'));
    const start = vi.spyOn(backend, 'startFlow');
    await expect(app.reveal('/tmp/b')).resolves.toBeUndefined();
    expect(app.logs.at(-1)!.line).toContain('no such folder');
    await expect(app.importBackup()).resolves.toBeUndefined();
    expect(app.logs.at(-1)).toMatchObject({ stream: 'info', line: expect.stringContaining('dialog broke') });
    expect(start).not.toHaveBeenCalled();
  });
});

describe('device snapshot vs events', () => {
  test('a device event that arrives before the snapshot is kept', async () => {
    const s = SCENARIOS['no-device'];
    const backend = createMockBackend(s, { speed: 1 });
    let emitDevice: ((d: DeviceStatus) => void) | undefined;
    const { onDeviceStatus } = backend;
    backend.onDeviceStatus = (cb) => ((emitDevice = cb), onDeviceStatus(cb));
    let resolveSnapshot!: (d: DeviceStatus) => void;
    vi.spyOn(backend, 'getDevice').mockReturnValue(new Promise((r) => (resolveSnapshot = r)));
    const app = new AppState(backend, createMockPlatform(s));
    const connected = app.connect();
    await vi.advanceTimersByTimeAsync(0);
    emitDevice!({ kind: 'edl' });
    resolveSnapshot({ kind: 'none' }); // stale: taken before the tablet showed up
    await connected;
    expect(app.device.kind).toBe('edl');
  });

  test('without events the snapshot applies', async () => {
    const { app } = await setup('no-device');
    expect(app.device.kind).toBe('none');
    const { app: ready } = await setup('ready');
    expect(ready.device.kind).toBe('adb');
  });
});

describe('edlFound', () => {
  const rescueAt = (step: FlowState['step']): FlowState => ({
    flowId: 'f-r', kind: 'rescue', step, stepIndex: 3, stepCount: 4, status: 'running',
    prompt: null, error: null, result: null, writing: false,
  });
  test('follows the device, and counts a rescue flow past waiting as found', async () => {
    const { app, emit } = await setup('no-device');
    expect(app.edlFound).toBe(false);
    emit.state!(rescueAt('waitEdl'));
    expect(app.edlFound).toBe(false);
    for (const step of ['write', 'verifyWrite', 'reset', 'done'] as const) {
      emit.state!(rescueAt(step));
      expect(app.edlFound, step).toBe(true);
    }
    expect(app.device.kind).toBe('none');
  });
  test('a non-rescue write is not "found"', async () => {
    const { app, emit } = await setup('no-device');
    emit.state!({ ...rescueAt('write'), kind: 'root' });
    expect(app.edlFound).toBe(false);
  });
});
