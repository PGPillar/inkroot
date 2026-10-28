import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, test, vi } from 'vitest';
import { CHOOSE_SLOT_HINTS, EDL_NOT_FOUND_AUTO, PROMPT_COPY } from '../copy';
import { ALL_PROMPT_KINDS } from '../flows';
import type { DeviceStatus, FlowKind, Prompt, PromptKind } from '../ipc';
import PromptView from './PromptView.svelte';

const IDS: Record<PromptKind, string[]> = {
  cableCheck: ['have', 'whereToGet', 'continueWithout'], cableCheckConfirm: ['confirmContinue', 'back'], chooseSlot: ['a', 'b'],
  keepWaitingOrRescue: ['keepWaiting', 'goToRescue'], installDriver: ['install', 'cancel'], installUdev: ['install', 'cancel'],
  edlNotFound: ['retry', 'cancel'], retryWrite: ['retry', 'cancel'], stillInEdl: ['retry', 'heldPower'],
  tabletStarting: ['homeScreen'],
};
const mk = (kind: PromptKind, ids = IDS[kind], emph: Record<string, 'primary' | 'secondary' | 'danger'> = {}, id = 'p1'): Prompt => ({
  id, kind, options: ids.map((i) => ({ id: i, label: 'raw-' + i, emphasis: emph[i] ?? 'secondary' })),
});
const none: DeviceStatus = { kind: 'none' };
const props = (prompt: Prompt, extra: Record<string, unknown> = {}) => ({
  prompt, kind: 'root' as FlowKind, device: none, disabled: false, onAnswer: vi.fn(), onOpen: vi.fn(), onCopy: vi.fn(), ...extra,
});

describe('PromptView', () => {
  test.each(ALL_PROMPT_KINDS)('%s renders heading and every option', async (k) => {
    const p = props(mk(k), { kind: 'rescue' });
    render(PromptView, p);
    expect(screen.getByRole('heading').textContent).toBe(PROMPT_COPY[k].heading);
    for (const [id, text] of Object.entries(PROMPT_COPY[k].options)) expect(screen.getByRole('button', { name: text })).toBeTruthy();
    if (k === 'cableCheck') return; // first option is 'have'; covered below
    if (k === 'cableCheckConfirm') await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: PROMPT_COPY[k].options[IDS[k][0]] }));
    expect(p.onAnswer).toHaveBeenCalledWith(IDS[k][0]);
  });

  test('cableCheck first option answers', async () => {
    const p = props(mk('cableCheck'));
    render(PromptView, p);
    await fireEvent.click(screen.getByRole('button', { name: 'Yes, I have one' }));
    expect(p.onAnswer).toHaveBeenCalledWith('have');
  });

  test('whereToGet opens the buy panel without answering', async () => {
    const p = props(mk('cableCheck'));
    render(PromptView, p);
    await fireEvent.click(screen.getByRole('button', { name: 'Show me where to get one' }));
    expect(screen.getByRole('heading', { name: 'Getting a rescue cable' })).toBeTruthy();
    expect(p.onAnswer).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy search terms' }));
    expect(p.onCopy).toHaveBeenCalledWith('Qualcomm deep flash cable');
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('button', { name: 'Yes, I have one' })).toBeTruthy();
  });

  test('continue without cable needs the tick', async () => {
    const p = props(mk('cableCheckConfirm', undefined, { back: 'primary', confirmContinue: 'danger' }));
    render(PromptView, p);
    const btn = screen.getByRole('button', { name: 'Continue without a cable' });
    expect(btn).toBeDisabled();
    await fireEvent.click(btn);
    expect(p.onAnswer).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('checkbox'));
    expect(btn).toBeEnabled();
    await fireEvent.click(btn);
    expect(p.onAnswer).toHaveBeenCalledWith('confirmContinue');
  });

  test('danger option is last and never auto-focused', () => {
    render(PromptView, props(mk('cableCheck', undefined, { have: 'primary', continueWithout: 'danger' })));
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Yes, I have one' }));
    const names = screen.getAllByRole('button').map((b) => b.textContent?.trim());
    expect(names[names.length - 1]).toBe('Continue without one');
  });

  test('danger listed first still moves after non-danger options', () => {
    render(PromptView, props(mk('cableCheck', ['continueWithout', 'have', 'whereToGet'], { continueWithout: 'danger' })));
    const names = screen.getAllByRole('button').map((b) => b.textContent?.trim());
    expect(names).toEqual(['Yes, I have one', 'Show me where to get one', 'Continue without one']);
    expect(document.activeElement).not.toBe(screen.getByRole('button', { name: 'Continue without one' }));
  });

  test('second click is ignored until next prompt', async () => {
    const p = props(mk('retryWrite', undefined, { retry: 'primary' }));
    const { rerender } = render(PromptView, p);
    const retry = screen.getByRole('button', { name: 'Try again' });
    await fireEvent.click(retry);
    await fireEvent.click(retry);
    expect(p.onAnswer).toHaveBeenCalledTimes(1);
    expect(retry).toBeDisabled();
    await rerender({ ...p, prompt: mk('retryWrite', undefined, { retry: 'primary' }, 'p2') });
    expect(screen.getByRole('button', { name: 'Try again' })).toBeEnabled();
  });

  test('focus moves to the next prompt\'s primary after an answer', async () => {
    const mkp = (id: string) => mk('retryWrite', undefined, { retry: 'primary' }, id);
    const p = props(mkp('p1'));
    const { rerender } = render(PromptView, p);
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await rerender({ ...p, prompt: mkp('p2') });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Try again' }));
  });

  test('disabled prop disables every button', () => {
    render(PromptView, props(mk('retryWrite'), { disabled: true }));
    for (const b of screen.getAllByRole('button')) expect(b).toBeDisabled();
  });

  describe('chooseSlot hint', () => {
    const slotDevice = { kind: 'adb', info: { slot: 'b' } } as unknown as DeviceStatus;
    test('from file name', () => {
      render(PromptView, props(mk('chooseSlot'), { imageName: 'boot_a.stock.img', device: slotDevice }));
      expect(screen.getByText(/The file name says A, so we've picked that\./)).toBeTruthy();
    });
    test('a file merely named boot_backup is not read as slot B', () => {
      render(PromptView, props(mk('chooseSlot'), { imageName: 'boot_backup.img', device: slotDevice }));
      expect(screen.getByText(/Your tablet is using B right now\./)).toBeTruthy();
      expect(screen.queryByText(/The file name says/)).toBeNull();
    });
    test('boot_b.img is read as slot B', () => {
      render(PromptView, props(mk('chooseSlot'), { imageName: 'boot_b.img' }));
      expect(screen.getByText(/The file name says B, so we've picked that\./)).toBeTruthy();
    });
    test('from device slot', () => {
      render(PromptView, props(mk('chooseSlot'), { device: slotDevice }));
      expect(screen.getByText(/Your tablet is using B right now\./)).toBeTruthy();
    });
    test('empty hint leaves no placeholder', () => {
      render(PromptView, props(mk('chooseSlot')));
      const body = screen.getByText(/Your tablet keeps two copies/);
      expect(body.textContent).toBe(PROMPT_COPY.chooseSlot.body.replace(' {hint}', ''));
      expect(CHOOSE_SLOT_HINTS.none).toBe('');
    });
  });

  test('rescue footer shows found when told so, even with no live device status', () => {
    render(PromptView, props(mk('edlNotFound'), { kind: 'rescue', found: true }));
    expect(screen.getByText('Found it ✓')).toBeTruthy();
  });

  test('edlNotFound copy differs for rescue vs root', () => {
    const { unmount } = render(PromptView, props(mk('edlNotFound'), { kind: 'rescue' }));
    expect(screen.getAllByRole('listitem')).toHaveLength(PROMPT_COPY.edlNotFound.bullets!.length);
    unmount();
    render(PromptView, props(mk('edlNotFound'), { kind: 'root' }));
    expect(screen.getByRole('heading').textContent).toBe(EDL_NOT_FOUND_AUTO.heading);
    expect(screen.getByText(EDL_NOT_FOUND_AUTO.body)).toBeTruthy();
    expect(screen.queryAllByRole('listitem')).toHaveLength(0);
  });
});
