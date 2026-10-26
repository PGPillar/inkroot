import { test } from 'node:test';
import assert from 'node:assert/strict';
import { markSvg, ICON, FAVICON } from '../brand/mark.mjs';
import { wordmarkSvg } from '../brand/wordmark.mjs';

const INK = '#111111';
const PANEL = '#E8E9E4';

// fill of the first element carrying data-role="<role>"
const roleFill = (svg, role) => {
  const m = svg.match(new RegExp(`<[^>]*data-role="${role}"[^>]*>`));
  assert.ok(m, `no element with data-role="${role}"`);
  return m[0].match(/fill="(#[0-9A-Fa-f]{6})"/)[1].toUpperCase();
};

test('markSvg is 1-bit, square-cornered and deterministic', () => {
  const a = markSvg({ size: 1024, border: 48, cell: 16 });
  assert.equal(a, markSvg({ size: 1024, border: 48, cell: 16 }));
  assert.doesNotMatch(a, /\brx=|gradient|filter|opacity/);
  const fills = new Set([...a.matchAll(/fill="(#[0-9A-Fa-f]{6})"/g)].map(m => m[1].toUpperCase()));
  assert.deepEqual([...fills].sort(), ['#111111', '#E8E9E4']);
  assert.match(a, /viewBox="0 0 1024 1024"/);
});

test('markSvg uses only <rect> shapes', () => {
  const a = markSvg({ size: 64, border: 8, cell: 4 });
  const tags = new Set([...a.matchAll(/<([a-z]+)[\s>]/g)].map(m => m[1]));
  assert.deepEqual([...tags].sort(), ['rect', 'svg']);
});

test('markSvg dither ramps from ~75% ink top-left to ~25% bottom-right', () => {
  // 1px cells over a 64px dither field (inside a 1-cell gutter) so ink
  // coverage can be counted per pixel
  const size = 68, border = 1, cell = 1, start = border + cell, end = size - border - cell;
  const svg = markSvg({ size, border, cell });
  const ink = Array.from({ length: size }, () => new Uint8Array(size));
  for (const m of svg.matchAll(/<rect x="(\d+)" y="(\d+)" width="(\d+)" height="(\d+)" fill="#111111"\/>/g)) {
    const [x, y, w, h] = m.slice(1).map(Number);
    if (w === size && h === size) continue; // the border plate
    for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) ink[j][i] = 1;
  }
  const coverage = (x0, y0) => {
    let n = 0;
    for (let j = y0; j < y0 + 8; j++) for (let i = x0; i < x0 + 8; i++) n += ink[j][i];
    return n / 64;
  };
  const tl = coverage(start, start);
  const br = coverage(end - 8, end - 8);
  assert.equal(coverage(border, border) < tl, true, 'gutter keeps the frame clear');
  assert.ok(tl > 0.65 && tl <= 0.8, `top-left coverage ${tl}`);
  assert.ok(br >= 0.2 && br < 0.35, `bottom-right coverage ${br}`);
});

test('markSvg honours ink/panel overrides', () => {
  const a = markSvg({ size: 64, border: 8, cell: 4, ink: '#E8E9E4', panel: '#111111' });
  assert.match(a, /^<svg[^>]*><rect x="0" y="0" width="64" height="64" fill="#E8E9E4"\/>/);
});

test('wordmarkSvg outlines text (no <text>, no font dependency)', async () => {
  const s = await wordmarkSvg({ theme: 'light', height: 64 });
  assert.doesNotMatch(s, /<text|font-family/);
  assert.match(s, /<path /);
  assert.match(s, /height="64"/);
});

test('dark wordmark inverts colours', async () => {
  const light = await wordmarkSvg({ theme: 'light', height: 64 });
  const dark = await wordmarkSvg({ theme: 'dark', height: 64 });
  assert.equal(roleFill(light, 'glyphs'), INK);
  assert.equal(roleFill(light, 'plate'), PANEL);
  assert.equal(roleFill(dark, 'glyphs'), PANEL);
  assert.equal(roleFill(dark, 'plate'), INK);
  // the embedded mark flips too: its outer border plate takes the glyph colour
  assert.match(light, /<svg[^>]*data-role="mark"[^>]*><rect x="0" y="0" width="\d+" height="\d+" fill="#111111"\/>/);
  assert.match(dark, /<svg[^>]*data-role="mark"[^>]*><rect x="0" y="0" width="\d+" height="\d+" fill="#E8E9E4"\/>/);
  // same geometry, colours swapped
  const swap = s => s.replace(/#111111|#E8E9E4/g, c => (c === INK ? PANEL : INK));
  assert.equal(swap(light), dark);
});

test('icon and favicon dithers survive OS downscaling', () => {
  for (const [spec, smallest] of [[ICON, 16], [FAVICON, 16]]) {
    // every cell is at least 2 px once the OS shrinks the icon to its smallest size
    assert.ok((spec.cell * smallest) / spec.size >= 2, `cell ${spec.cell} too fine for ${smallest}px`);
    // the field is a whole number of cells, so no clipped sliver at the edge
    const field = spec.size - 2 * spec.border - 2 * spec.cell;
    assert.equal(field % spec.cell, 0);
    assert.ok(field / spec.cell >= 4 && field / spec.cell <= 8, `${field / spec.cell} cells across`);
  }
});
