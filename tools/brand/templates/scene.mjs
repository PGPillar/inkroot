// Shared composition for the README hero and the social preview: copy on the
// left, a generic line-art tablet on the right. Everything is 1-bit: the only
// "greys" are ordered dithers drawn with the same Bayer matrix as the mark.
import { ditherRects } from '../mark.mjs';

export const ditherSvg = (w, h, cell, density) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" shape-rendering="crispEdges">` +
  ditherRects({ x: 0, y: 0, width: w, height: h, cell, density, fill: 'currentColor' }) +
  `</svg>`;

const flat = d => () => d;

// Plex has no U+25CF, so the status dot is drawn: a 7×7 pixel disc.
const DISC = ['..xxx..', '.xxxxx.', 'xxxxxxx', 'xxxxxxx', 'xxxxxxx', '.xxxxx.', '..xxx..'];
const pixels = (cls, rows) =>
  `<svg class="${cls}" viewBox="0 0 ${rows[0].length} ${rows.length}" shape-rendering="crispEdges" aria-hidden="true">` +
  rows.map((row, y) => [...row].map((c, x) => (c === 'x' ? `<rect x="${x}" y="${y}" width="1" height="1"/>` : '')).join('')).join('') +
  `</svg>`;
// a drawn tick, so the screen never leans on a font's check-mark glyph
const TICK = ['......x', '.....x.', 'x...x..', '.x.x...', '..x....'];
export const tickSvg = () => pixels('tick', TICK);
export const dotSvg = () => pixels('dot', DISC);

// 24×24 icon silhouettes from the Panel mockup, used as dither masks.
const ICONS = {
  root: 'M12 2l3 6 6 1-4.5 4 1 6.5L12 16l-5.5 3.5 1-6.5L3 9l6-1z',
  rescue: 'M12 2a10 10 0 100 20 10 10 0 000-20zm1 5v6h-2V7zm0 8v2h-2v-2z',
  stock: 'M12 4V1L7 6l5 5V7a5 5 0 11-5 5H5a7 7 0 107-8z',
};
const mask = d => `url(data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="${d}"/></svg>`)})`;

function card(icon, title, desc) {
  return `<div class="card box">
    <span class="ico dither" style="-webkit-mask-image:${mask(ICONS[icon])};mask-image:${mask(ICONS[icon])}">${ditherSvg(48, 48, 2, flat(0.5))}</span>
    <span class="ct">${title}</span><span class="cd">${desc}</span>
  </div>`;
}

export function tabletHtml() {
  return `<div class="tablet">
    <div class="screen box">
      <div class="srow">
        <span class="slogo"><span class="smark dither">${ditherSvg(16, 16, 2, (x, y) => 0.75 - 0.5 * ((x + y) / 14))}</span>INKROOT</span>
        <span class="schip">${dotSvg()}NOTE MAX / FW 4.2</span>
      </div>
      <div class="sh">Pick a job.</div>
      <div class="ssub">backup ${tickSvg()} boot_b.img · 96.0 MB</div>
      <div class="cards">
        ${card('root', 'Root', 'magisk 30.7')}
        ${card('rescue', 'Rescue', 'stuck / bootloop')}
        ${card('stock', 'Stock', 'undo root')}
      </div>
    </div>
  </div>`;
}

export function applyTheme() {
  const theme = new URLSearchParams(location.search).get('theme') === 'dark' ? 'dark' : 'light';
  document.documentElement.dataset.theme = theme;
  const wm = document.querySelector('[data-wordmark]');
  if (wm) wm.src = `../../../assets/brand/inkroot-wordmark${theme === 'dark' ? '-dark' : ''}.svg`;
  return theme;
}

export function mount() {
  applyTheme();
  for (const el of document.querySelectorAll('[data-dot]')) el.insertAdjacentHTML('afterbegin', dotSvg());
  document.querySelector('[data-tablet]').innerHTML = tabletHtml();
  // the field fades in from data-field (fraction of the width) to ~70% ink at the right edge
  const field = document.querySelector('[data-field]');
  const start = Number(field.dataset.field || 0.5);
  field.innerHTML = ditherSvg(field.clientWidth, field.clientHeight, 4,
    (x, y, cols) => Math.max(0, ((x / (cols - 1) - start) / (1 - start)) * 0.7));
}
