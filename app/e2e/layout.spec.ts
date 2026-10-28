import { test, expect, type Page } from '@playwright/test';

type Box = { x: number; y: number; width: number; height: number };
const overlap = (a: Box, b: Box) =>
  a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

/** Every block of the step frame, plus the drawing inside the art slot. */
async function blocks(page: Page) {
  return page.evaluate(() => {
    const frame = document.querySelector('section.frame')!;
    const r = (el: Element) => {
      const b = el.getBoundingClientRect();
      return { x: b.x, y: b.y, width: b.width, height: b.height };
    };
    const kids = [...frame.children].map((el) => ({ name: el.tagName.toLowerCase() + '.' + el.className.split(' ')[0], box: r(el) }));
    const svg = frame.querySelector('.art svg');
    return { kids, art: svg ? r(svg) : null };
  });
}

const cases = [
  ['shot-root-reset', 'Restarting your tablet'],
  ['shot-root-boot-timeout', "Your tablet hasn't restarted yet"],
  ['shot-root-still-in-edl', 'Your tablet is still in rescue mode'],
  ['shot-rescue-waiting', 'Written and checked ✓'],
  ['shot-rescue-back', 'Your tablet is back ✓'],
  ['shot-rescue-not-seen', 'Written and checked ✓'],
  ['shot-rescue-back-in-edl', 'Your tablet went back to rescue mode'],
] as const;

for (const [scenario, heading] of cases) {
  test(`Details open never overlaps the step (${scenario})`, async ({ page }) => {
    await page.setViewportSize({ width: 880, height: 640 });
    await page.goto(`/?mock=${scenario}&speed=1000`);
    await expect(page.getByRole('heading', { name: heading })).toBeVisible();
    await page.getByRole('button', { name: /^Details/ }).click();
    await expect(page.locator('footer pre')).toBeVisible();

    const { kids, art } = await blocks(page);
    expect(art, 'illustration present').not.toBeNull();
    for (let i = 0; i < kids.length; i++) {
      for (let j = i + 1; j < kids.length; j++) {
        expect(overlap(kids[i].box, kids[j].box), `${kids[i].name} overlaps ${kids[j].name}`).toBe(false);
      }
      if (!kids[i].name.startsWith('div.art')) {
        expect(overlap(kids[i].box, art!), `illustration overlaps ${kids[i].name}`).toBe(false);
      }
    }
    const bar = page.getByRole('progressbar');
    if (await bar.count()) {
      const b = (await bar.boundingBox())!;
      expect(overlap(b, art!), 'progress bar overlaps the illustration').toBe(false);
    }
    // The drawing keeps its full size inside its slot.
    const slot = kids.find((k) => k.name.startsWith('div.art'))!.box;
    expect(art!.y).toBeGreaterThanOrEqual(slot.y - 0.5);
    expect(art!.y + art!.height).toBeLessThanOrEqual(slot.y + slot.height + 0.5);
    // Nothing of the step hides under the Details panel: what doesn't fit scrolls.
    const footer = (await page.locator('footer').boundingBox())!;
    const main = (await page.locator('main').boundingBox())!;
    expect(main.y + main.height).toBeLessThanOrEqual(footer.y + 0.5);
    await page.locator('main').evaluate((m) => (m.scrollTop = m.scrollHeight));
    const actions = (await page.locator('section.frame .actions').boundingBox())!;
    expect(actions.y + actions.height).toBeLessThanOrEqual(footer.y + 0.5);
    await expect(page.locator('section.frame .actions button').first()).toBeInViewport();
  });
}
