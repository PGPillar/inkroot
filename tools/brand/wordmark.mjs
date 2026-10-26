// INKROOT wordmark: the dithered mark + "INKROOT" in IBM Plex Mono SemiBold,
// outlined to <path> so it renders identically everywhere (GitHub strips fonts).
// Built on a fixed 64-unit-tall grid and scaled with width/height.
import { fileURLToPath } from 'node:url';
import opentype from 'opentype.js';
import { markSvg, INK, PANEL } from './mark.mjs';

// @ibm/plex-mono@1 ships WOFF/WOFF2 only (no TTF); opentype.js reads WOFF.
export const FONT_PATH = fileURLToPath(
  new URL('../node_modules/@ibm/plex-mono/fonts/complete/woff/IBMPlexMono-SemiBold.woff', import.meta.url),
);

const GRID = 64;          // design height
const PAD = 12;           // plate padding around the lockup
const MARK = GRID - 2 * PAD; // 40: mark square
const MARK_BORDER = 4;
const MARK_CELL = 4;      // 6×6 dither field: crisp at the 32–64 px heights it is used at
const GAP = 16;           // mark → text (0.4 × mark, as in the Panel mockup)
const CAP = MARK / 2;     // text cap height = half the mark
const TRACKING = 0.02;    // letter-spacing, em

let fontPromise;
const loadFont = () => (fontPromise ??= opentype.load(FONT_PATH));

const r = n => Math.round(n * 100) / 100;
const glyphAdvance = (font, ch, scale) => font.charToGlyph(ch).advanceWidth * scale;

export async function wordmarkSvg({ theme, height }) {
  if (theme !== 'light' && theme !== 'dark') throw new Error(`theme must be light|dark, got ${theme}`);
  const font = await loadFont();
  const fg = theme === 'light' ? INK : PANEL;
  const bg = theme === 'light' ? PANEL : INK;

  const fontSize = CAP / (font.tables.os2.sCapHeight / font.unitsPerEm);
  const scale = fontSize / font.unitsPerEm;
  const baseline = PAD + MARK / 2 + CAP / 2;
  const textX = PAD + MARK + GAP;

  let x = textX;
  let d = '';
  let last = 0;
  for (const glyph of font.stringToGlyphs('INKROOT')) {
    d += glyph.getPath(x, baseline, fontSize).toPathData(2);
    last = x + glyph.advanceWidth * scale;
    x = last + TRACKING * fontSize;
  }
  // trim the final glyph's right side-bearing so padding is optically even
  const bbox = font.getPath('T', 0, 0, fontSize).getBoundingBox();
  const trailing = glyphAdvance(font, 'T', scale) - bbox.x2;
  const width = Math.ceil(last - trailing + PAD);

  const mark = markSvg({ size: MARK, border: MARK_BORDER, cell: MARK_CELL, ink: fg, panel: bg })
    .replace('<svg ', `<svg data-role="mark" x="${PAD}" y="${PAD}" `)
    .replace(/ width="\d+" height="\d+"/, ` width="${MARK}" height="${MARK}"`);

  const outW = r((width * height) / GRID);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${GRID}" width="${outW}" height="${height}" role="img" aria-label="Inkroot">` +
    `<rect data-role="plate" x="0" y="0" width="${width}" height="${GRID}" fill="${bg}"/>` +
    mark +
    `<path data-role="glyphs" fill="${fg}" d="${d}"/>` +
    `</svg>`
  );
}
