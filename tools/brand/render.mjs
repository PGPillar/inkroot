#!/usr/bin/env node
// Render Inkroot's brand assets into ../../assets.
//   node brand/render.mjs <mark|wordmark|icon|hero|social|placeholders|all> [--force]
// SVGs are generated directly; PNGs are rendered by headless Chromium at exact
// pixel sizes. `placeholders` keeps existing screenshots unless --force is given.
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir, access } from 'node:fs/promises';
import { dirname, extname, join, normalize, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import opentype from 'opentype.js';
import { markSvg, ICON, FAVICON } from './mark.mjs';
import { wordmarkSvg } from './wordmark.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const out = p => join(ROOT, 'assets', p);
const TARGETS = ['mark', 'wordmark', 'icon', 'hero', 'social', 'placeholders'];
const MOCKUP = 'docs/superpowers/specs/assets/2026-10-25-look-directions.html';

const MIME = {
  '.html': 'text/html', '.css': 'text/css', '.mjs': 'text/javascript', '.js': 'text/javascript',
  '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2', '.woff': 'font/woff',
};

async function write(path, data) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, data);
  console.log(`wrote ${relative(ROOT, path)}`);
}
const exists = p => access(p).then(() => true, () => false);

// ---- SVG targets ---------------------------------------------------------

const renderMark = () => write(out('brand/inkroot-mark.svg'), markSvg(FAVICON) + '\n');
const renderIcon = () => write(out('icon/inkroot-icon-1024.svg'), markSvg(ICON) + '\n');
async function renderWordmark() {
  await write(out('brand/inkroot-wordmark.svg'), (await wordmarkSvg({ theme: 'light', height: 64 })) + '\n');
  await write(out('brand/inkroot-wordmark-dark.svg'), (await wordmarkSvg({ theme: 'dark', height: 64 })) + '\n');
}

// ---- browser plumbing ----------------------------------------------------

// Serve the repo root so templates can import ../mark.mjs and load fonts from
// tools/node_modules (file:// blocks module imports).
function serve() {
  const server = createServer(async (req, res) => {
    const path = normalize(join(ROOT, decodeURIComponent(new URL(req.url, 'http://x').pathname)));
    if (!path.startsWith(ROOT)) return res.writeHead(403).end();
    try {
      const body = await readFile(path);
      res.writeHead(200, { 'content-type': MIME[extname(path)] ?? 'application/octet-stream' }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve(server)));
}

// The mockup pulls Plex from Google Fonts; answer with the local copies so
// renders are offline and byte-stable.
const localPlexCss = base => ['Sans', 'Mono'].flatMap(fam => [[400, 'Regular'], [500, 'Medium'], [600, 'SemiBold'], [700, 'Bold']].map(([w, n]) =>
  `@font-face{font-family:"IBM Plex ${fam}";font-weight:${w};src:url(${base}/tools/node_modules/@ibm/plex-${fam.toLowerCase()}/fonts/complete/woff2/IBMPlex${fam}-${n}.woff2) format("woff2")}`)).join('\n');

async function withBrowser(fn) {
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch();
  try {
    return await fn(async (w, h) => {
      const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 1 });
      await page.route(url => !url.href.startsWith(base), route =>
        route.request().url().includes('fonts.googleapis.com')
          ? route.fulfill({ contentType: 'text/css', body: localPlexCss(base) })
          : route.abort());
      return page;
    }, base);
  } finally {
    await browser.close();
    server.close();
  }
}

// Fail loudly if any text fell back to a system font.
async function assertPlexLoaded(page) {
  await page.evaluate(() => document.fonts.ready);
  const bad = await page.evaluate(() => {
    const used = new Set();
    for (const el of document.querySelectorAll('body *')) {
      if (![...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim())) continue;
      const cs = getComputedStyle(el);
      used.add(`${cs.fontWeight} 16px ${cs.fontFamily.split(',')[0]}`);
    }
    return [...used].filter(f => f.includes('Plex') && ![...document.fonts].some(ff =>
      ff.status === 'loaded' && f.includes(ff.family) && String(ff.weight) === f.split(' ')[0]));
  });
  if (bad.length) throw new Error(`fonts not loaded: ${bad.join(', ')}`);
}

// Fail if any rendered character is missing from the Plex face that draws it
// (the browser would silently substitute a system font for that glyph).
const fontFile = (family, weight) => {
  const fam = family.includes('Mono') ? 'Mono' : 'Sans';
  const name = { 400: 'Regular', 500: 'Medium', 600: 'SemiBold' }[weight];
  return join(ROOT, `tools/node_modules/@ibm/plex-${fam.toLowerCase()}/fonts/complete/woff/IBMPlex${fam}-${name}.woff`);
};
async function assertGlyphCoverage(page) {
  const runs = await page.evaluate(() => [...document.querySelectorAll('body *')].flatMap(el => {
    const text = [...el.childNodes].filter(n => n.nodeType === 3).map(n => n.textContent).join('').replace(/\s/g, '');
    if (!text) return [];
    const cs = getComputedStyle(el);
    return [{ text: cs.textTransform === 'uppercase' ? text.toUpperCase() : text, family: cs.fontFamily, weight: cs.fontWeight }];
  }));
  const missing = new Set();
  for (const { text, family, weight } of runs) {
    if (!family.includes('Plex')) { missing.add(`${family}: ${text}`); continue; }
    const font = opentype.loadSync(fontFile(family, weight));
    for (const ch of text) if (font.charToGlyphIndex(ch) === 0) missing.add(`${ch} (U+${ch.codePointAt(0).toString(16).toUpperCase()}) in ${family} ${weight}`);
  }
  if (missing.size) throw new Error(`glyphs would fall back to a system font: ${[...missing].join(', ')}`);
}

async function shoot(page, url, path) {
  await page.goto(url);
  await page.waitForSelector('body[data-ready="1"]');
  await assertPlexLoaded(page);
  await assertGlyphCoverage(page);
  await mkdir(dirname(path), { recursive: true });
  await page.screenshot({ path, type: 'png' });
  console.log(`wrote ${relative(ROOT, path)}`);
}

// ---- PNG targets ---------------------------------------------------------

const renderIconPng = () => withBrowser(async (newPage, base) => {
  const page = await newPage(1024, 1024);
  await page.goto(`${base}/assets/icon/inkroot-icon-1024.svg`);
  await page.screenshot({ path: out('icon/inkroot-icon-1024.png'), type: 'png', omitBackground: true });
  console.log('wrote assets/icon/inkroot-icon-1024.png');
});

const renderHero = () => withBrowser(async (newPage, base) => {
  const page = await newPage(1600, 560);
  for (const theme of ['light', 'dark'])
    await shoot(page, `${base}/tools/brand/templates/hero.html?theme=${theme}`, out(`readme/hero-${theme}.png`));
});

const renderSocial = () => withBrowser(async (newPage, base) => {
  const page = await newPage(1280, 640);
  await shoot(page, `${base}/tools/brand/templates/social.html`, out('social/social-preview.png'));
});

// Screenshot the Panel (direction B) screens of the approved mockup onto an
// 880×640 panel-coloured stage. Mockup chrome that Panel forbids (rounded
// corners, drop shadows, round window dots) is squared off on the way.
const PLACEHOLDERS = [
  { file: 'home.png', select: '.dir.B .win' },
  { file: 'rescue-cable.png', flow: 0 },
  { file: 'root-step.png', flow: 2 },
  { file: 'done.png', flow: 3 },
  { file: 'home-dark.png', select: '.dir.B .win', invert: true },
];

const renderPlaceholders = force => withBrowser(async (newPage, base) => {
  const page = await newPage(880, 640);
  for (const p of PLACEHOLDERS) {
    const path = out(`screenshots/${p.file}`);
    if (!force && await exists(path)) { console.log(`kept assets/screenshots/${p.file} (use --force to replace)`); continue; }
    await page.goto(`${base}/${MOCKUP}`);
    await page.evaluate(({ select, flow, invert }) => {
      document.getElementById('flowwrap').dataset.t = 'B';
      const el = select ? document.querySelector(select) : document.querySelectorAll('#flow .fs')[flow];
      const rect = el.getBoundingClientRect();
      // the mockup still shows the old Magisk pin
      const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      while (walk.nextNode()) walk.currentNode.textContent = walk.currentNode.textContent.replace('30.1', '30.7');
      const scope = el.closest('.flowwrap') ? el.closest('.flowwrap').cloneNode(false) : el.closest('.dir').cloneNode(false);
      scope.removeAttribute('onclick');
      scope.style.cssText = 'all:unset;display:block;';
      el.style.width = `${rect.width}px`;
      el.style.borderRadius = '0';
      el.style.boxShadow = 'none';
      el.style.animation = 'none';
      if (select) el.style.border = '2px solid #111';
      el.querySelectorAll('.bar i').forEach(i => (i.style.borderRadius = '0'));
      scope.appendChild(el);
      const stage = document.createElement('div');
      const zoom = Math.min(760 / rect.width, 560 / rect.height);
      stage.style.cssText = `position:fixed;inset:0;z-index:9;background:#e8e9e4;display:flex;align-items:center;justify-content:center;${invert ? 'filter:invert(1);' : ''}`;
      scope.style.zoom = zoom;
      stage.appendChild(scope);
      document.body.appendChild(stage);
      document.body.style.overflow = 'hidden';
      document.body.dataset.ready = '1';
    }, p);
    await assertPlexLoaded(page);
    await mkdir(dirname(path), { recursive: true });
    await page.screenshot({ path, type: 'png' });
    console.log(`wrote assets/screenshots/${p.file}`);
  }
});

// ---- CLI -----------------------------------------------------------------

const RUN = {
  mark: renderMark,
  wordmark: renderWordmark,
  icon: async () => { await renderIcon(); await renderIconPng(); },
  hero: renderHero,
  social: renderSocial,
  placeholders: () => renderPlaceholders(process.argv.includes('--force')),
};

const target = process.argv[2];
if (target !== 'all' && !TARGETS.includes(target)) {
  console.error(`usage: render.mjs <${[...TARGETS, 'all'].join('|')}> [--force]`);
  process.exit(2);
}
for (const t of target === 'all' ? TARGETS : [target]) await RUN[t]();
