import { fireEvent, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { renderWithApp } from '../../test/render';
import Shell from '../Shell.svelte';
import { DISCLAIMER_URL } from '../links';
import { createMockBackend, createMockPlatform } from '../mock/engine';
import { ROOTED_BACKUP, STOCK_BACKUP } from '../mock/fixtures';
import { SCENARIOS } from '../mock/scenarios';
import { AppState } from '../state.svelte';
import type { Route } from '../screen';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const heading = (name: string) => screen.getByRole('heading', { name });
const button = (name: string | RegExp) => screen.getByRole('button', { name });
const click = (name: string | RegExp) => fireEvent.click(button(name));
async function go(app: AppState, route: Route) {
  app.navigate(route);
  await tick();
}
const ACCEPT = "I understand and accept these risks; I'm responsible for my device";

/** An app on the mock backend with spies on what the screens call. */
/** The no-device scenario with an empty library. */
async function renderEmpty() {
  const s = { ...SCENARIOS['no-device'], backups: [] };
  const app = new AppState(createMockBackend(s), createMockPlatform(s));
  await app.connect();
  await renderWithApp(Shell, { app });
  return app;
}

async function spied(scenario: string) {
  const s = SCENARIOS[scenario];
  const backend = createMockBackend(s);
  const platform = createMockPlatform(s);
  const startFlow = vi.spyOn(backend, 'startFlow');
  const revealPath = vi.spyOn(backend, 'revealPath').mockResolvedValue();
  const openExternal = vi.spyOn(platform, 'openExternal').mockResolvedValue();
  const app = new AppState(backend, platform);
  await app.connect();
  return { app, startFlow, revealPath, openExternal };
}

describe('risk notice', () => {
  test('Continue is disabled until ticked, then sets the version and starts', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'no-backup' });
    await click(/Root/);
    const cont = button('Continue');
    expect(cont).toBeDisabled();
    await click('Back');
    await click(/Root/);
    await fireEvent.click(screen.getByRole('checkbox', { name: ACCEPT }));
    expect(button('Continue')).toBeEnabled();
    await click('Continue');
    await vi.waitFor(() => expect(app.flow?.kind).toBe('root'));
    expect(app.settings.acceptedRiskVersion).toBe(1);
  });

  test('gates rescue and unroot but not backup or import', async () => {
    const { app } = await spied('no-device').then(async (x) => {
      await renderWithApp(Shell, { app: x.app });
      return x;
    });
    await click('Tablet stuck on the ONYX logo? Rescue it →');
    await fireEvent.click(button('Choose a file…'));
    await vi.waitFor(() => expect(button('Continue')).toBeEnabled());
    await click('Continue');
    expect(heading('Please read this first')).toBeVisible();
    expect(app.flow).toBeNull();
  });

  test('backup and import start without the notice', async () => {
    const x = await spied('no-backup');
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'backups' });
    await click('Back up now');
    expect(x.startFlow).toHaveBeenCalledWith('backup', {});
    await vi.waitFor(() => expect(x.app.flow?.kind).toBe('backup'));
  });

  test('links the disclaimer', async () => {
    const x = await spied('no-backup');
    await renderWithApp(Shell, { app: x.app });
    await click(/Root/);
    await click('Read the full disclaimer');
    expect(x.openExternal).toHaveBeenCalledWith(DISCLAIMER_URL);
  });
});

describe('links and folders that fail to open', () => {
  test('a refused disclaimer link shows a note and a Details line', async () => {
    const x = await spied('no-backup');
    x.openExternal.mockRejectedValue(new Error('url not allowed'));
    await renderWithApp(Shell, { app: x.app });
    await click(/Root/);
    await click('Read the full disclaimer');
    await vi.waitFor(() => expect(screen.getByText("Couldn't open your browser. The link is in Details.")).toBeVisible());
    expect(x.app.logs.at(-1)!.line).toContain(DISCLAIMER_URL);
    await click('Back');
    expect(screen.queryByText(/Couldn't open your browser/)).toBeNull();
  });

  test('About links show the note too', async () => {
    const x = await spied('ready');
    x.openExternal.mockRejectedValue(new Error('url not allowed'));
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'about' });
    await click('Source code');
    await vi.waitFor(() => expect(screen.getByText("Couldn't open your browser. The link is in Details.")).toBeVisible());
  });

  test('a folder that cannot be shown leaves a Details line and no crash', async () => {
    const x = await spied('ready');
    x.revealPath.mockRejectedValue(new Error('no such folder'));
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'backups' });
    await fireEvent.click(screen.getAllByRole('button', { name: /Show in Finder/ })[0]);
    await vi.waitFor(() => expect(x.app.logs.at(-1)?.line).toContain('no such folder'));
    expect(heading('Your safety copies')).toBeVisible();
  });
});

describe('about', () => {
  test('states privacy and non-affiliation', async () => {
    const { app } = await renderWithApp(Shell, { scenario: 'ready' });
    await go(app, { screen: 'about' });
    expect(screen.getByText('Inkroot collects no data. Nothing leaves your computer.')).toBeVisible();
    expect(screen.getByText(/Not affiliated with/)).toBeVisible();
    expect(screen.getByText('Version 0.1.0-mock')).toBeVisible();
  });
});

describe('backups', () => {
  test('import uses the file dialog and starts importBackup', async () => {
    const x = await spied('ready');
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'backups' });
    await click('Import a backup…');
    await vi.waitFor(() => expect(x.app.flow?.kind).toBe('importBackup'));
    expect(x.startFlow).toHaveBeenCalledWith('importBackup', { imagePath: '/Users/you/Downloads/boot_b.stock.img' });
  });

  test('reveal button calls reveal_path', async () => {
    const x = await spied('ready');
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'backups' });
    await click('Show in Finder');
    expect(x.revealPath).toHaveBeenCalledWith(STOCK_BACKUP.path);
  });

  test('empty library says so and Back up now needs a tablet', async () => {
    const app = await renderEmpty();
    await go(app, { screen: 'backups' });
    expect(screen.getByText(/No safety copies yet/)).toBeVisible();
    expect(button('Back up now')).toBeDisabled();
  });
});

describe('rescue picker', () => {
  test('preselects from the route and passes the slot', async () => {
    const x = await spied('ready');
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'rescuePick', preselect: { backupId: STOCK_BACKUP.id, slot: 'b' } });
    await click('Continue');
    expect(x.startFlow).toHaveBeenCalledWith('rescue', { backupId: STOCK_BACKUP.id, slot: 'b' });
  });

  test('a different backup or a file drops the preselected slot', async () => {
    const x = await spied('ready');
    x.app.backups = [STOCK_BACKUP, ROOTED_BACKUP];
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'rescuePick', preselect: { backupId: STOCK_BACKUP.id, slot: 'b' } });
    const radios = screen.getAllByRole('radio');
    const other = radios.find((r) => !(r as HTMLInputElement).checked)!;
    await fireEvent.click(other);
    await click('Continue');
    expect(x.startFlow).toHaveBeenLastCalledWith('rescue', { backupId: ROOTED_BACKUP.id });
    await vi.waitFor(() => expect(x.app.flow?.kind).toBe('rescue'));
  });

  test('picking a file drops the preselected slot', async () => {
    const x = await spied('ready');
    await renderWithApp(Shell, { app: x.app });
    await go(x.app, { screen: 'rescuePick', preselect: { backupId: STOCK_BACKUP.id, slot: 'b' } });
    await click('Choose a file…');
    await vi.waitFor(() => expect(screen.getByText('boot_b.stock.img')).toBeVisible());
    await click('Continue');
    expect(x.startFlow).toHaveBeenCalledWith('rescue', { imagePath: '/Users/you/Downloads/boot_b.stock.img' });
  });

  test('with no backups it needs a file', async () => {
    const app = await renderEmpty();
    await go(app, { screen: 'rescuePick' });
    expect(screen.getByText(/There are no safety copies in Inkroot yet/)).toBeVisible();
    expect(button('Continue')).toBeDisabled();
    await click('Choose a file…');
    await vi.waitFor(() => expect(button('Continue')).toBeEnabled());
    expect(screen.getByText('boot_b.stock.img')).toBeVisible();
  });
});
