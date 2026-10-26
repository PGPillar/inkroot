import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { selectBackend } from '../backend';
import { ALL_PROMPT_KINDS } from '../flows';
import type { FlowState, PromptKind } from '../ipc';
import type { Beat, Branch } from './engine';
import { createMockBackend } from './engine';
import { SCENARIOS } from './scenarios';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

async function connect(name: string) {
  const backend = createMockBackend(SCENARIOS[name], { speed: 1 });
  const states: FlowState[] = [];
  await backend.onFlowState((s) => states.push(s));
  await backend.getDevice();
  await vi.advanceTimersByTimeAsync(1);
  return { backend, states };
}

describe('mock backend', () => {
  test('happy root emits ordered steps then done', async () => {
    const { backend, states } = await connect('ready');
    const answered = new Set<string>();
    await backend.onFlowState((s) => {
      if (s.prompt && !answered.has(s.prompt.id)) {
        answered.add(s.prompt.id);
        void backend.answerPrompt(s.flowId, s.prompt.id, 'have');
      }
    });
    await backend.startFlow('root', {});
    await vi.advanceTimersByTimeAsync(120_000);
    const steps = states.map((s) => s.step).filter((v, i, a) => a[i - 1] !== v);
    const wanted = ['checkDevice', 'patch', 'cableCheck', 'write', 'verifyWrite', 'reset', 'waitBoot', 'verifyRoot', 'done'];
    let at = -1;
    for (const w of wanted) {
      at = steps.indexOf(w as never, at + 1);
      expect(at, `step ${w} in ${steps.join(',')}`).toBeGreaterThan(-1);
    }
    const last = states[states.length - 1];
    expect(last.status).toBe('done');
    expect(last.result?.magiskVersion).toBe('30.7');
    expect(last.prompt).toBeNull();
    expect(await backend.getDevice()).toMatchObject({ kind: 'adb', info: { rooted: true } });
  });

  test('risk not accepted rejects root', async () => {
    await expect(createMockBackend(SCENARIOS['no-backup']).startFlow('root', {})).rejects.toMatchObject({
      code: 'internal',
      message: 'Please read and accept the risk notice first.',
    });
  });

  test('busy while a flow runs', async () => {
    const { backend } = await connect('ready');
    await backend.startFlow('backup', {});
    await expect(backend.startFlow('backup', {})).rejects.toMatchObject({ code: 'busy' });
  });

  test('startErrors reject before anything else', async () => {
    const { backend } = await connect('root-busy');
    await expect(backend.startFlow('root', {})).rejects.toMatchObject({ code: 'busy' });
  });

  test('reinstallBootService starts without the risk notice (backend gates only root|unroot|rescue)', async () => {
    const { backend } = await connect('rooted');
    await expect(backend.startFlow('reinstallBootService', {})).resolves.toMatch(/^mock-/);
  });

  test('cancel rejected while writing', async () => {
    const { backend } = await connect('shot-root-writing');
    await expect(backend.cancelFlow('mock-shot')).rejects.toMatchObject({ code: 'internal' });
  });

  test('cancel stops a running flow', async () => {
    const { backend, states } = await connect('ready');
    const id = await backend.startFlow('backup', {});
    await vi.advanceTimersByTimeAsync(1000);
    await backend.cancelFlow(id);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(states[states.length - 1].status).toBe('cancelled');
  });

  test('stale promptId ignored', async () => {
    const { backend, states } = await connect('ready');
    const id = await backend.startFlow('root', {});
    await vi.advanceTimersByTimeAsync(60_000);
    const waiting = states[states.length - 1];
    expect(waiting.status).toBe('waitingForUser');
    const count = states.length;
    await backend.answerPrompt(id, 'stale', 'have');
    expect(states).toHaveLength(count);
    await backend.answerPrompt(id, waiting.prompt!.id, 'continueWithout');
    await vi.advanceTimersByTimeAsync(10);
    expect(states[states.length - 1].prompt?.kind).toBe('cableCheckConfirm');
    // back re-asks the first question
    await backend.answerPrompt(id, states[states.length - 1].prompt!.id, 'back');
    await vi.advanceTimersByTimeAsync(10);
    expect(states[states.length - 1].prompt?.kind).toBe('cableCheck');
  });

  test('importing adds the backup, listBackups is newest first and filters by serial', async () => {
    const { backend } = await connect('ready');
    const id = await backend.startFlow('backup', {});
    await vi.advanceTimersByTimeAsync(60_000);
    const list = await backend.listBackups();
    expect(list.map((b) => b.id)).toEqual(['NM24B0A1C3/2026-10-26_0915', 'NM24B0A1C3/2026-10-25_1042']);
    expect(await backend.listBackups('other')).toEqual([]);
    expect(await backend.getLog(id)).toContain('edl --memory=ufs r boot_a');
  });

  test('every prompt kind is reachable in some scenario', () => {
    const kinds = new Set<PromptKind>();
    const walk = (beats: Beat[]) => {
      for (const b of beats) {
        if (!('prompt' in b)) continue;
        kinds.add(b.prompt.kind);
        for (const br of Object.values(b.branches) as Branch[]) if (Array.isArray(br)) walk(br);
      }
    };
    for (const s of Object.values(SCENARIOS)) {
      Object.values(s.flows).forEach(walk);
    }
    expect([...kinds].sort()).toEqual([...ALL_PROMPT_KINDS].sort());
  });

  test('every non-shot scenario starts', async () => {
    for (const s of Object.values(SCENARIOS)) {
      if (s.name.startsWith('shot-')) continue;
      const { backend } = await connect(s.name);
      expect(await backend.getDevice()).toEqual(s.device);
      expect((await backend.getSettings()).acceptedRiskVersion).toBe(s.settings?.acceptedRiskVersion ?? null);
    }
  });

  test('plug-in walks none, unauthorized, adb', async () => {
    const backend = createMockBackend(SCENARIOS['plug-in']);
    const seen: string[] = [];
    await backend.onDeviceStatus((d) => seen.push(d.kind));
    await backend.getDevice();
    await vi.advanceTimersByTimeAsync(9000);
    expect(seen).toEqual(['unauthorized', 'adb']);
  });

  test('reinstall with no hook switches kind to root under the same flowId', async () => {
    const { backend, states } = await connect('reinstall-hook-absent');
    const id = await backend.startFlow('reinstallBootService', {});
    await vi.advanceTimersByTimeAsync(10_000);
    const switched = states.find((s) => s.kind === 'root');
    expect(switched).toMatchObject({ flowId: id, stepIndex: 2, stepCount: 4 });
  });

  test('frozen scenario emits its state once at connect', async () => {
    const { states } = await connect('shot-root-writing');
    expect(states).toHaveLength(1);
    expect(states[0]).toMatchObject({ step: 'write', writing: true, stepIndex: 3 });
  });
});

describe('selectBackend', () => {
  test('uses mock with ?mock and falls back to ready', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(selectBackend({ search: '?mock=nope' }, {}).mock).toBe('ready');
    expect(warn).toHaveBeenCalled();
    expect(selectBackend({ search: '?mock=edl&speed=5' }, { __TAURI_INTERNALS__: {} }).mock).toBe('edl');
    expect(selectBackend({ search: '' }, {}).mock).toBe('ready');
    warn.mockRestore();
  });

  test('uses the real backend inside Tauri', () => {
    expect(selectBackend({ search: '' }, { __TAURI_INTERNALS__: {} }).mock).toBeNull();
  });
});
