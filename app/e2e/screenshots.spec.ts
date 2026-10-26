import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const OUT = fileURLToPath(new URL('../../assets/screenshots', import.meta.url));

const shots = [
  ['home.png', 'ready', 'light', 'Pick a job.'],
  ['root-step.png', 'shot-root-writing', 'light', 'Writing to your tablet'],
  ['rescue-cable.png', 'shot-rescue-cable', 'light', 'You need a deep-flash cable'],
  ['done.png', 'shot-root-done', 'light', 'Your Note Max is rooted.'],
  ['home-dark.png', 'ready', 'dark', 'Pick a job.'],
] as const;

for (const [file, scenario, scheme, heading] of shots) {
  test(file, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: 'reduce' });
    await page.goto(`/?mock=${scenario}&speed=1000`);
    await expect(page.getByRole('heading', { name: heading })).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    expect(await page.evaluate(() => document.fonts.check('600 16px "IBM Plex Mono"'))).toBe(true);
    await page.screenshot({ path: path.join(OUT, file) });
  });
}

test('makes no network requests outside localhost', async ({ page }) => {
  const urls: string[] = [];
  page.on('request', (r) => urls.push(r.url()));
  await page.goto('/?mock=ready&speed=1000');
  await expect(page.getByRole('heading', { name: 'Pick a job.' })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  expect(urls.length).toBeGreaterThan(0);
  for (const u of urls) expect(u.startsWith('http://localhost:5179')).toBe(true);
});
