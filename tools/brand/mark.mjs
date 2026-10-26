// The Inkroot mark: a hard-bordered square holding, inside a one-cell gutter, a 1-bit ordered dither that
// fades from dense ink (top-left) to sparse ink (bottom-right). Pure <rect>s,
// two colours, no curves — it should look like it was drawn by an e-ink panel.

export const INK = '#111111';
export const PANEL = '#E8E9E4';

export const BAYER4 = [
  [0, 8, 2, 10],
  [12, 4, 14, 6],
  [3, 11, 1, 9],
  [15, 7, 13, 5],
];

// Shipped mark sizes. The app icon is downscaled by the OS to 16–256 px, so its
// dither uses few, large cells: a 5×5 field whose cells stay ≥2 px at 16 px.
// The favicon SVG keeps the same proportions.
export const ICON = Object.freeze({ size: 1024, border: 64, cell: 128 });
export const FAVICON = Object.freeze({ size: 64, border: 4, cell: 8 });

const rect = (x, y, w, h, fill) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`;

/**
 * Ordered-dither a box into ink rects. Each row's horizontal runs of ink cells
 * are merged into one rect. Cells at the right/bottom edge are clipped to the box.
 * density(cx, cy, cols, rows) returns the ink fraction (0..1) for a cell.
 */
export function ditherRects({ x, y, width, height, cell, density, fill = INK }) {
  const cols = Math.ceil(width / cell);
  const rows = Math.ceil(height / cell);
  const out = [];
  for (let cy = 0; cy < rows; cy++) {
    const top = y + cy * cell;
    const h = Math.min(cell, y + height - top);
    let run = -1;
    for (let cx = 0; cx <= cols; cx++) {
      const on = cx < cols && BAYER4[cy % 4][cx % 4] / 16 < density(cx, cy, cols, rows);
      if (on && run < 0) run = cx;
      if (!on && run >= 0) {
        const left = x + run * cell;
        const right = Math.min(x + cx * cell, x + width);
        out.push(rect(left, top, right - left, h, fill));
        run = -1;
      }
    }
  }
  return out.join('');
}

/** Diagonal ramp: ~75% ink at the top-left cell, ~25% at the bottom-right. */
export const diagonalRamp = (from = 0.75, to = 0.25) => (cx, cy, cols, rows) => {
  const span = Math.max(1, cols - 1 + rows - 1);
  return from + (to - from) * ((cx + cy) / span);
};

export function markSvg({ size, border, cell, ink = INK, panel = PANEL }) {
  const inner = size - 2 * border;
  // one blank cell of panel between the frame and the dither keeps the frame
  // reading as a hard edge even where the dither is densest
  const field = inner - 2 * cell;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}" shape-rendering="crispEdges">` +
    rect(0, 0, size, size, ink) +
    rect(border, border, inner, inner, panel) +
    ditherRects({ x: border + cell, y: border + cell, width: field, height: field, cell, density: diagonalRamp(), fill: ink }) +
    `</svg>`
  );
}
