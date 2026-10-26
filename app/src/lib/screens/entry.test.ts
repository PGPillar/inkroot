import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { renderWithApp } from '../../test/render';
import App from '../../App.svelte';
import Shell from '../Shell.svelte';
import { createMockBackend, createMockPlatform } from '../mock/engine';
import { SCENARIOS } from '../mock/scenarios';
import { AppState } from '../state.svelte';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const heading = (name: string) => screen.getByRole('heading', { name });
const button = (name: string | RegExp) => screen.getByRole('button', { name });
const click = (name: string | RegExp) => fireEvent.click(button(name));

describe('gates', () => {
  test.each([
    ['no-device', 'Plug in your Note Max'],
    ['unauthorized', 'Tap Allow on your tablet'],
    ['edl', 'Your tablet is in rescue mode'],
    ['unknown-model', 'Inkroot only supports the BOOX Note Max right now.'],
    ['unknown-firmware', "This software version isn't supported yet"],
    ['low-battery', 'Charge to at least 30 % first'],
  ])('scenario %s shows gate "%s"', async (s, h) => {
    await renderWithApp(Shell, { scenario: s });
    expect(heading(h)).toBeVisible();
  });

  test('gate-none offers Rescue', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'no-device' });
    await click('Tablet stuck on the ONYX logo? Rescue it →');
    expect(app.screen).toBe('rescuePick');
    expect(heading('Which copy should we put back?')).toBeVisible();
  });

  test('edl gate offers Rescue', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'edl' });
    await click('Rescue my tablet');
    expect(app.screen).toBe('rescuePick');
  });

  test('unknown model names what was found', async () => {
    await renderWithApp(Shell, { scenario: 'unknown-model' });
    expect(screen.getByText(/We found a TabUltraCPro\./)).toBeVisible();
  });

  test('low battery shows the level', async () => {
    await renderWithApp(Shell, { scenario: 'low-battery' });
    expect(screen.getByText(/Your tablet is at 18 %/)).toBeVisible();
    expect(screen.getByText('battery 18 % · need 30 %')).toBeVisible();
  });

  test('unknown firmware opens a prefilled issue', async () => {
    const s = SCENARIOS['unknown-firmware'];
    const platform = createMockPlatform(s);
    const open = vi.spyOn(platform, 'openExternal').mockResolvedValue();
    const app = new AppState(createMockBackend(s), platform);
    await app.connect();
    await renderWithApp(Shell, { app });
    expect(screen.getByText('fw TKQ1.230721.003 · build 240')).toBeVisible();
    await click('Ask for support on GitHub');
    expect(open).toHaveBeenCalledOnce();
    expect(open.mock.calls[0][0]).toContain('Firmware%20support%3A%20TKQ1.230721.003');
  });
});

test('a refused support link shows a note under the firmware line', async () => {
  const s = SCENARIOS['unknown-firmware'];
  const platform = createMockPlatform(s);
  vi.spyOn(platform, 'openExternal').mockRejectedValue(new Error('url not allowed'));
  const app = new AppState(createMockBackend(s), platform);
  await app.connect();
  await renderWithApp(Shell, { app });
  await click('Ask for support on GitHub');
  await vi.waitFor(() => expect(screen.getByText("Couldn't open your browser. The link is in Details.")).toBeVisible());
  expect(app.logs.at(-1)!.line).toContain('github.com');
});

test('a refused Activity Launcher link shows a note', async () => {
  const s = SCENARIOS['no-device'];
  const platform = createMockPlatform(s);
  vi.spyOn(platform, 'openExternal').mockRejectedValue(new Error('url not allowed'));
  const app = new AppState(createMockBackend(s), platform);
  await app.connect();
  await renderWithApp(Shell, { app });
  await click('Show me how');
  await click('Next');
  await click('Next');
  await click("I can't find it");
  await click('F-Droid');
  await vi.waitFor(() => expect(screen.getByText("Couldn't open your browser. The link is in Details.")).toBeVisible());
});

describe('home', () => {
  test('home shows the job cards and backup line', async () => {
    await renderWithApp(Shell, { scenario: 'ready' });
    expect(heading('Pick a job.')).toBeVisible();
    expect(screen.getByText('backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc')).toBeVisible();
    expect(button(/^Root/)).toBeEnabled();
    expect(button(/^Rescue/)).toBeEnabled();
    expect(button(/^Stock/)).toBeDisabled();
    expect(button('Backups →')).toBeVisible();
  });

  test('without a backup the line says one will be made', async () => {
    await renderWithApp(Shell, { scenario: 'no-backup' });
    expect(screen.getByText("no backup yet · we'll make one first")).toBeVisible();
  });

  test('Root starts the job with the stock backup', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'ready' });
    const start = vi.spyOn(app, 'startJob');
    await click(/^Root/);
    expect(start).toHaveBeenCalledWith('root', { backupId: SCENARIOS.ready.backups[0].id });
  });

  test('Rescue card opens the rescue picker', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'ready' });
    await click(/^Rescue/);
    expect(app.screen).toBe('rescuePick');
  });

  test('Backups link opens the library', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'ready' });
    await click('Backups →');
    expect(app.screen).toBe('backups');
  });
});

describe('usb debugging guide', () => {
  test('usb guide method B path', async () => {
    await renderWithApp(Shell, { scenario: 'no-device' });
    await click('Show me how');
    expect(heading('Open Settings, then Apps')).toBeVisible();
    expect(screen.getByText('Turn on USB debugging · 1 of 4')).toBeVisible();
    await click('Next');
    await click('Next');
    expect(heading('Turn on USB Debug Mode')).toBeVisible();
    await click("I can't find it");
    expect(heading('Get Activity Launcher')).toBeVisible();
    expect(screen.getByText('Turn on USB debugging · 1 of 5')).toBeVisible();
    await click('Back');
    expect(heading('Turn on USB Debug Mode')).toBeVisible();
  });

  test('Activity Launcher links open externally', async () => {
    const s = SCENARIOS['no-device'];
    const platform = createMockPlatform(s);
    const open = vi.spyOn(platform, 'openExternal').mockResolvedValue();
    const app = new AppState(createMockBackend(s), platform);
    await app.connect();
    await renderWithApp(Shell, { app });
    await click('Show me how');
    await click('Next');
    await click('Next');
    await click("I can't find it");
    await click('F-Droid');
    expect(open.mock.calls[0][0]).toContain('f-droid.org');
  });

  test('Close on the first panel goes back', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'no-device' });
    await click('Show me how');
    await click('Close');
    expect(app.screen).toBe('gate-none');
  });

  test('usb guide auto-advances when the tablet appears', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'plug-in' });
    await click('Show me how');
    expect(heading('Open Settings, then Apps')).toBeVisible();
    await vi.advanceTimersByTimeAsync(4000);
    expect(heading('Tap Allow on your tablet')).toBeVisible();
    expect(app.screen).toBe('usbGuide');
    expect(screen.getByText('tablet found · tap Allow on it')).toBeVisible();
    await vi.advanceTimersByTimeAsync(4000);
    expect(heading('Pick a job.')).toBeVisible();
    expect(app.route.screen).toBe('home');
  });

  test('Back from the allow panel works while the tablet waits for Allow', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'unauthorized' });
    await click('Show me how');
    expect(heading('Tap Allow on your tablet')).toBeVisible();
    expect(app.screen).toBe('usbGuide');
    await click('Back');
    expect(heading('Open Settings, then Apps')).toBeVisible();
    await click('Close');
    expect(app.screen).toBe('gate-unauthorized');
  });

  test('usb guide reachable from home', async () => {
    await renderWithApp(Shell, { scenario: 'ready' });
    await click('Trouble connecting? USB debugging help');
    expect(heading('Open Settings, then Apps')).toBeVisible();
    expect(screen.getByText('Found it ✓')).toBeVisible();
    await click('Close');
    expect(heading('Pick a job.')).toBeVisible();
  });
});

describe('already rooted', () => {
  test('rooted device: Root card leads to Already rooted with both options', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'rooted' });
    expect(button(/^Stock/)).toBeEnabled();
    await click(/^Root/);
    expect(heading('Already rooted ✓')).toBeVisible();
    expect(screen.getByText('Magisk 30.7 is running, and so is the boot service.')).toBeVisible();
    expect(button(/^Reinstall boot service/)).toBeEnabled();
    expect(button(/^Return to stock/)).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Import a backup' })).toBeNull();
    const start = vi.spyOn(app, 'startJob');
    await click(/^Reinstall boot service/);
    expect(start).toHaveBeenCalledWith('reinstallBootService', {});
  });

  test('Return to stock uses the stock backup', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'rooted' });
    await click(/^Root/);
    const start = vi.spyOn(app, 'startJob');
    await click(/^Return to stock/);
    expect(start).toHaveBeenCalledWith('unroot', { backupId: SCENARIOS.rooted.backups[0].id });
  });

  test('rooted without stock backup points to import', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'rooted-no-stock' });
    expect(button(/^Stock/)).toBeDisabled();
    expect(button(/^Stock/)).toHaveTextContent('no original copy');
    await click(/^Root/);
    expect(screen.getByText(/You don't have an original safety copy on file\./)).toBeVisible();
    expect(button(/^Return to stock/)).toBeDisabled();
    const imp = vi.spyOn(app, 'importBackup').mockResolvedValue();
    await click('Import a backup');
    expect(imp).toHaveBeenCalledOnce();
  });

  test('Back returns home', async () => {
    await renderWithApp(Shell, { scenario: 'rooted' });
    await click(/^Root/);
    await click('Back');
    expect(heading('Pick a job.')).toBeVisible();
  });
});

test('App boots the mock backend and shows starting… until it loads', async () => {
  render(App);
  expect(screen.getByText('starting…')).toBeVisible();
  await vi.advanceTimersByTimeAsync(10);
  expect(await screen.findByRole('heading', { name: 'Pick a job.' })).toBeVisible();
  expect(screen.queryByText('starting…')).toBeNull();
});

test('App says so plainly when it cannot connect', async () => {
  const connect = vi.spyOn(AppState.prototype, 'connect').mockRejectedValue(new Error('no backend'));
  const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
  render(App);
  await vi.advanceTimersByTimeAsync(10);
  expect(screen.getByRole('alert')).toHaveTextContent("Inkroot couldn't start. Try closing it and opening it again.");
  expect(screen.queryByText('starting…')).toBeNull();
  expect(logged).toHaveBeenCalled();
  connect.mockRestore();
  logged.mockRestore();
});
