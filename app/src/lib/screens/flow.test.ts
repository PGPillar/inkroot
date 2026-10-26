import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { renderWithApp } from '../../test/render';
import Shell from '../Shell.svelte';
import type { DeviceStatus, FlowState } from '../ipc';
import { createMockBackend, createMockPlatform } from '../mock/engine';
import { NEW_BACKUP, STOCK_BACKUP } from '../mock/fixtures';
import { SCENARIOS } from '../mock/scenarios';
import { AppState } from '../state.svelte';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const heading = (name: string | RegExp) => screen.getByRole('heading', { name });
const hasHeading = (name: string | RegExp) => () => screen.queryByRole('heading', { name }) !== null;
const button = (name: string | RegExp) => screen.getByRole('button', { name });
const click = (name: string | RegExp) => fireEvent.click(button(name));

/** Renders the Shell on a mock backend, capturing the event callbacks and the close handler. */
async function setup(name: string, speed = 20) {
  const s = SCENARIOS[name];
  const backend = createMockBackend(s, { speed });
  const platform = createMockPlatform(s);
  const emit: { state?: (f: FlowState) => void; device?: (d: DeviceStatus) => void } = {};
  const { onFlowState, onDeviceStatus } = backend;
  backend.onFlowState = (cb) => ((emit.state = cb), onFlowState(cb));
  backend.onDeviceStatus = (cb) => ((emit.device = cb), onDeviceStatus(cb));
  let allowClose: (() => boolean) | null = null;
  vi.spyOn(platform, 'onCloseRequested').mockImplementation(async (allow) => {
    allowClose = allow;
    return () => {};
  });
  const app = new AppState(backend, platform);
  await app.connect();
  await vi.advanceTimersByTimeAsync(1);
  await renderWithApp(Shell, { app });
  return { app, backend, platform, emit, allowClose: () => allowClose! };
}

/** Advances fake time until the predicate holds (or fails after two simulated minutes). */
async function until(pred: () => boolean, what: string) {
  for (let t = 0; t < 1200; t++) {
    if (pred()) return;
    await vi.advanceTimersByTimeAsync(100);
  }
  throw new Error(`timed out waiting for ${what}`);
}

describe('flow screen', () => {
  test('flow shows step label, heading and progress', async () => {
    await setup('shot-root-writing');
    expect(screen.getByText('STEP 3 OF 4 · ROOTING')).toBeVisible();
    expect(heading('Writing to your tablet')).toBeVisible();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '61');
    expect(screen.getByText('boot_b · 59.0 MB / 96.0 MB')).toBeVisible();
  });

  test('writing hides Cancel and blocks close', async () => {
    const x = await setup('shot-root-writing');
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();
    expect(screen.getByText('writing · please keep it plugged in')).toBeVisible();
    expect(x.allowClose()()).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    const dialog = screen.getByRole('alertdialog', { name: 'Inkroot is writing to your tablet' });
    expect(within(dialog).getByRole('button', { name: 'Keep Inkroot open' })).toHaveFocus();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Keep Inkroot open' }));
    expect(x.app.closeBlocked).toBe(false);
    expect(screen.queryByRole('alertdialog')).toBeNull();

    x.allowClose()();
    await vi.advanceTimersByTimeAsync(1);
    const close = vi.spyOn(x.platform, 'closeWindow');
    await click(/Close anyway/);
    expect(close).toHaveBeenCalledOnce();
  });

  test('close guard keeps focus inside the dialog', async () => {
    const x = await setup('shot-root-writing');
    x.allowClose()();
    await vi.advanceTimersByTimeAsync(1);
    const keep = button('Keep Inkroot open');
    const anyway = button(/Close anyway/);
    // The danger button is never first in tab order.
    expect(keep.compareDocumentPosition(anyway) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    anyway.focus();
    await fireEvent.keyDown(anyway, { key: 'Tab' });
    expect(keep).toHaveFocus();
    await fireEvent.keyDown(keep, { key: 'Tab', shiftKey: true });
    expect(anyway).toHaveFocus();
    await fireEvent.keyDown(anyway, { key: 'Escape' });
    expect(x.app.closeBlocked).toBe(false);
  });

  test('close guard goes away when the write ends, and Keep returns focus', async () => {
    const x = await setup('shot-root-writing');
    const details = button(/^Details/);
    details.focus();
    x.allowClose()();
    await vi.advanceTimersByTimeAsync(1);
    expect(button('Keep Inkroot open')).toHaveFocus();
    await click('Keep Inkroot open');
    await vi.advanceTimersByTimeAsync(1);
    expect(details).toHaveFocus();

    x.allowClose()();
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.getByRole('alertdialog')).toBeVisible();
    x.emit.state!({ ...x.app.flow!, step: 'verifyWrite', writing: false });
    await vi.advanceTimersByTimeAsync(1);
    expect(x.app.closeBlocked).toBe(false);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  test('prompts keep the primary on the right and danger on the left, in DOM order primary first', async () => {
    await setup('accepted');
    await click(/^Root/);
    await until(hasHeading('Do you have a rescue cable?'), 'cable check');
    const names = screen.getAllByRole('button').map((b) => b.textContent?.trim()).filter((n) => n && /one/.test(n));
    expect(names).toEqual(['Yes, I have one', 'Show me where to get one', 'Continue without one']);
    expect(button('Yes, I have one')).toHaveFocus();
    expect(screen.getByRole('img', { name: 'Do you have a rescue cable?' })).toBeVisible();
  });

  test('Cancel visible when not writing and calls cancel_flow', async () => {
    const x = await setup('ready', 1);
    const cancel = vi.spyOn(x.backend, 'cancelFlow');
    await x.app.startJob('backup');
    await vi.advanceTimersByTimeAsync(10);
    expect(heading('Checking your tablet')).toBeVisible();
    await click('Cancel');
    await vi.advanceTimersByTimeAsync(10);
    expect(cancel).toHaveBeenCalledOnce();
    expect(heading('Stopped.')).toBeVisible();
    expect(screen.getByText("Nothing else will happen. You can start again whenever you're ready.")).toBeVisible();
    await click('Back to start');
    expect(heading('Pick a job.')).toBeVisible();
  });

  test('full root happy path renders done with freeze tip', async () => {
    const x = await setup('accepted');
    await click(/^Root/);
    await until(hasHeading('Do you have a rescue cable?'), 'cable check');
    await click('Yes, I have one');
    await until(hasHeading('Your Note Max is rooted.'), 'done');
    expect(screen.getByText('Done')).toBeVisible();
    expect(screen.getByText(/Magisk 30\.7 is running/)).toBeVisible();
    expect(heading('One more thing')).toBeVisible();
    for (const c of ['Magisk daemon alive', 'Boot service running as root', 'Backup kept safe']) {
      expect(screen.getByText(c)).toBeVisible();
    }
    const open = vi.spyOn(x.platform, 'openExternal').mockResolvedValue();
    await click('What can I do with root? →');
    expect(open).toHaveBeenCalledWith(expect.stringContaining('boot-service'));
    open.mockRejectedValueOnce(new Error('url not allowed'));
    await click('What can I do with root? →');
    await vi.waitFor(() => expect(screen.getByText("Couldn't open your browser. The link is in Details.")).toBeVisible());
    await click('Finish');
    expect(x.app.flow).toBeNull();
    expect(heading('Pick a job.')).toBeVisible();
  });

  test('root safety-copy step shows the checklist', async () => {
    await setup('ready', 1);
    await click(/^Root/);
    await until(hasHeading('Checking the copy'), 'verifyBackup');
    expect(screen.getByText('STEP 2 OF 4 · SAFETY COPY')).toBeVisible();
    expect(screen.getByText('Firmware 4.2 recognised')).toBeVisible();
    expect(screen.getByText('Active slot: B')).toBeVisible();
    expect(screen.getByText('Copy checked')).toBeVisible();
  });

  test('boot timeout offers Keep waiting / Go to Rescue and Rescue preselects backup', async () => {
    await setup('root-boot-timeout');
    await click(/^Root/);
    await until(hasHeading('Do you have a rescue cable?'), 'cable check');
    await click('Yes, I have one');
    await until(hasHeading("Your tablet hasn't restarted yet"), 'boot timeout');
    expect(button('Keep waiting')).toBeVisible();
    await click('Go to Rescue');
    await until(hasHeading('Which copy should we put back?'), 'rescue pick');
    const checked = screen.getAllByRole('radio').filter((r) => (r as HTMLInputElement).checked);
    expect(checked).toHaveLength(1);
    expect(checked[0].closest('label')).toHaveTextContent(STOCK_BACKUP.createdAt.slice(0, 10));
  });

  test('hash mismatch prompt then success', async () => {
    await setup('root-hash-mismatch');
    await click(/^Root/);
    await until(hasHeading('Do you have a rescue cable?'), 'cable check');
    await click('Yes, I have one');
    await until(hasHeading("That write didn't check out"), 'retry prompt');
    await click('Try again');
    await until(hasHeading('Your Note Max is rooted.'), 'done');
  });

  test('failure view uses heading table and suggested action', async () => {
    const x = await setup('root-patch-failed');
    await click(/^Root/);
    await until(hasHeading("Preparing root didn't work"), 'failure');
    expect(screen.getByText('Stopped')).toBeVisible();
    expect(screen.getByText("We couldn't prepare the file for your tablet.")).toBeVisible();
    expect(screen.getByText('The full details are in Details below if you want to report this.')).toBeVisible();
    const start = vi.spyOn(x.app, 'startJob');
    await click('Try again');
    expect(start).toHaveBeenCalledWith('root', { backupId: STOCK_BACKUP.id });
    await until(hasHeading("Preparing root didn't work"), 'second failure');
    await click('Back to start');
    expect(heading('Pick a job.')).toBeVisible();
  });

  test('failure for a tablet found in rescue mode goes to Rescue', async () => {
    const x = await setup('ready');
    x.emit.state!({
      flowId: 'mock-x', kind: 'backup', step: 'checkDevice', stepIndex: 1, stepCount: 4, status: 'failed',
      prompt: null, result: null, writing: false,
      error: { code: 'noDevice', message: 'Your tablet is in rescue mode.', detail: '05c6:9008', suggestedAction: 'goToRescue' },
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(heading('We lost your tablet')).toBeVisible();
    await click('Go to Rescue');
    expect(x.app.flow).toBeNull();
    expect(heading('Which copy should we put back?')).toBeVisible();
  });

  test('unknown error codes and a missing last start fall back safely', async () => {
    const x = await setup('ready');
    x.emit.state!({
      flowId: 'mock-y', kind: 'root', step: 'patch', stepIndex: 3, stepCount: 4, status: 'failed',
      prompt: null, result: null, writing: false,
      error: { code: 'somethingNew' as never, message: 'New trouble.', detail: '', suggestedAction: 'retry' },
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(heading('Something went wrong')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Try again' })).toBeNull();
    expect(button('Back to start')).toBeVisible();
  });

  test('rescue shows cable guide with live status and found state', async () => {
    const x = await setup('shot-rescue-cable');
    expect(heading('You need a deep-flash cable')).toBeVisible();
    expect(screen.getByText('Step 2 of 4 · Cable · 1 of 6')).toBeVisible();
    expect(screen.getByText('Waiting for your tablet…')).toBeVisible();
    expect(screen.getByText("A black screen is normal. Your tablet isn't broken, and this is fixable.")).toBeVisible();
    x.emit.device!({ kind: 'edl' });
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.getByText('Found it ✓')).toBeVisible();
  });

  test('cable guide walks all six panels with timers', async () => {
    const x = await setup('shot-rescue-cable');
    const copy = vi.spyOn(x.platform, 'copyText');
    await click('Copy search terms');
    expect(copy).toHaveBeenCalledWith('Qualcomm deep flash cable');
    await click('I have one');
    expect(heading('Use a real USB-A port')).toBeVisible();
    expect(screen.getByText(/No USB-A port\?/)).toBeVisible();
    await click('Next');
    expect(heading("Hold the cable's button")).toBeVisible();
    await click('Back');
    expect(heading('Use a real USB-A port')).toBeVisible();
    await click('Next');
    await click('Next');
    expect(heading('Plug it into your tablet')).toBeVisible();
    await click('Next');
    expect(heading('Hold the power button for 15 seconds')).toBeVisible();
    await click('Start a 15 s timer');
    await vi.advanceTimersByTimeAsync(1000);
    expect(button(/14 s/)).toBeVisible();
    await vi.advanceTimersByTimeAsync(14000);
    expect(button("Time's up ✓")).toBeVisible();
    await click('Next');
    expect(heading('Keep holding the cable button 10 more seconds')).toBeVisible();
    expect(screen.getByText('Step 2 of 4 · Cable · 6 of 6')).toBeVisible();
    expect(button('Start a 10 s timer')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
    const cancel = vi.spyOn(x.backend, 'cancelFlow');
    await click('Cancel');
    expect(cancel).toHaveBeenCalledOnce();
  });

  test('the guide keeps its panel when the step moves on to waiting', async () => {
    const x = await setup('shot-rescue-cable');
    await click('I have one');
    await click('Next');
    expect(heading("Hold the cable's button")).toBeVisible();
    x.emit.state!({ ...x.app.flow!, step: 'waitEdl', stepIndex: 3 });
    await vi.advanceTimersByTimeAsync(1);
    expect(heading("Hold the cable's button")).toBeVisible();
    expect(screen.getByText('Step 3 of 4 · Cable · 3 of 6')).toBeVisible();
  });

  test('rescue edl not found shows troubleshooting', async () => {
    await setup('rescue-edl-not-found', 1);
    await click('Tablet stuck on the ONYX logo? Rescue it →');
    await fireEvent.click(screen.getAllByRole('radio')[0]);
    await click('Continue');
    await until(hasHeading('You need a deep-flash cable'), 'cable guide');
    await click('I have one');
    for (let i = 0; i < 3; i++) await click('Next');
    expect(heading('Hold the power button for 15 seconds')).toBeVisible();
    await vi.advanceTimersByTimeAsync(6000);
    await until(hasHeading("We can't see your tablet yet"), 'not found');
    expect(screen.getByText("Check it's the deep-flash cable, not a normal one.")).toBeVisible();
    expect(screen.getByText(/Use a USB-A port on the computer itself/)).toBeVisible();
    expect(screen.getByText('Waiting for your tablet…')).toBeVisible();
    expect(screen.getByText("A black screen is normal. Your tablet isn't broken, and this is fixable.")).toBeVisible();
    await click('Try again');
    await vi.advanceTimersByTimeAsync(1);
    // Back on the guide, from holding the cable's button again.
    expect(heading("Hold the cable's button")).toBeVisible();
    await until(() => screen.queryByText('Found it ✓') !== null || hasHeading('Writing to your tablet')(), 'found');
  });

  test('reinstall that switches to root keeps rendering', async () => {
    const x = await setup('reinstall-hook-absent', 1);
    await x.app.startJob('reinstallBootService');
    await until(() => x.app.flow?.kind === 'root', 'switch to root');
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.getByText('STEP 2 OF 4 · SAFETY COPY')).toBeVisible();
  });

  test('backup done reveals folder', async () => {
    const x = await setup('ready');
    const reveal = vi.spyOn(x.backend, 'revealPath');
    await x.app.startJob('backup');
    await until(hasHeading('Your safety copy is saved.'), 'backup done');
    expect(screen.getByText(new RegExp(NEW_BACKUP.path))).toBeVisible();
    expect(screen.getByText('Startup software copied')).toBeVisible();
    await click('Show in Finder');
    expect(reveal).toHaveBeenCalledWith(NEW_BACKUP.path);
  });

  test('import rescue-only done copy', async () => {
    const x = await setup('import-rescue-only');
    await x.app.importBackup();
    await until(hasHeading('Which slot is this copy from?'), 'slot');
    await click('Slot B');
    await until(hasHeading('Saved for rescue only.'), 'import done');
  });

  test('rescue done waits for the tablet to come back', async () => {
    const x = await setup('ready');
    x.emit.state!({
      flowId: 'mock-r', kind: 'rescue', step: 'done', stepIndex: 4, stepCount: 4, status: 'done',
      prompt: null, error: null, result: {}, writing: false,
    });
    x.emit.device!({ kind: 'none' });
    await vi.advanceTimersByTimeAsync(1);
    expect(heading('Unplug the cable and start your tablet.')).toBeVisible();
    expect(screen.getByText('waiting for it to start…')).toBeVisible();
    x.emit.device!(SCENARIOS.ready.device);
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.getByText("It's back ✓ Your tablet started.")).toBeVisible();
  });

  test('unknown step falls back without crashing', async () => {
    const x = await setup('ready');
    x.emit.state!({
      flowId: 'mock-z', kind: 'backup', step: 'pushScripts', stepIndex: 2, stepCount: 4, status: 'running',
      prompt: null, error: null, result: null, writing: false,
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(heading('Working on it')).toBeVisible();
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
  });

  test.each(['ready', 'no-device', 'shot-root-writing'])('every screen has Details and Copy log (%s)', async (name) => {
    const x = await setup(name);
    const copy = vi.spyOn(x.platform, 'copyText');
    await click(/^Details/);
    await click('Copy log');
    expect(copy).toHaveBeenCalledOnce();
    if (name === 'shot-root-writing') expect(screen.getByText(/edl --memory=ufs w boot_b patched-boot\.img/)).toBeVisible();
  });
});
