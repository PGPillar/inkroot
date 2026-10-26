import { render } from '@testing-library/svelte';
import type { Component } from 'svelte';
import { describe, expect, test } from 'vitest';
import * as art from './index';
import { DeepFlashCable, DitherDefs, Hand, Tablet } from './index';

// Every illustration exported from index.ts; DitherDefs is a <pattern> helper, not art.
const ALL_ART = Object.entries(art).filter(([name]) => name !== 'DitherDefs') as [
  string,
  Component<{ title: string }>,
][];

const patternIdOf = (svg: Element) => svg.querySelector('pattern')!.getAttribute('id')!;

describe('illustrations', () => {
  test('index exports all six illustrations plus DitherDefs', () => {
    expect(ALL_ART.map(([n]) => n).sort()).toEqual(
      ['DeepFlashCable', 'Hand', 'MenuSketch', 'PressIndicator', 'Tablet', 'UsbAPort'],
    );
    expect(DitherDefs).toBeDefined();
  });

  test.each(ALL_ART)('%s is inline vector, labelled, no raster', (name, C) => {
    const { container } = render(C, { title: name });
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('role')).toBe('img');
    expect(svg).toHaveAttribute('aria-label', name);
    expect(svg).toHaveAttribute('fill', 'none');
    expect(svg).toHaveAttribute('stroke', 'currentColor');
    expect(svg).toHaveAttribute('stroke-width', '2');
    expect(container.querySelector('image, img, foreignObject')).toBeNull();
    expect(container.innerHTML).not.toMatch(/BOOX|ONYX/i);
  });

  test.each(ALL_ART)('%s marks every drawn element with a part name', (_name, C) => {
    const { container } = render(C, { title: 'x' });
    const svg = container.querySelector('svg')!;
    const drawn = svg.querySelectorAll(':scope > :not(defs)');
    expect(drawn.length).toBeGreaterThan(0);
    for (const el of drawn) expect(el).toHaveAttribute('data-part');
  });

  test('two instances get distinct pattern ids', () => {
    const a = render(Tablet, { title: 'a', highlight: 'screen' }).container.querySelector('svg')!;
    const b = render(Tablet, { title: 'b', highlight: 'screen' }).container.querySelector('svg')!;
    const idA = patternIdOf(a);
    const idB = patternIdOf(b);
    expect(idA).not.toBe(idB);
    expect(a.querySelector('[data-part=screen]')).toHaveAttribute('fill', `url(#${idA})`);
    expect(b.querySelector('[data-part=screen]')).toHaveAttribute('fill', `url(#${idB})`);
  });

  test('highlight applies dither fill', () => {
    const svg = render(DeepFlashCable, { title: 'cable', highlight: 'button' }).container.querySelector('svg')!;
    const fill = svg.querySelector('[data-part=button]')!.getAttribute('fill');
    expect(fill).toMatch(/^url\(#.+\)$/);
    expect(fill).toBe(`url(#${patternIdOf(svg)})`);
    expect(svg.querySelector('[data-part=usbA]')).toHaveAttribute('fill', 'none');
  });

  test.each(['press', 'hold'] as const)('Hand %s has an opaque panel knockout so it can overlay art', (pose) => {
    const svg = render(Hand, { title: 'hand', pose }).container.querySelector('svg')!;
    const knockout = svg.querySelector('[data-part=silhouette]')!;
    expect(knockout.getAttribute('style')).toMatch(/fill:\s*var\(--panel\)/);
    expect(svg.firstElementChild!.nextElementSibling).toBe(knockout); // painted first, under the outline
  });

  test('DitherDefs draws a two-square checkerboard of the given cell', () => {
    const { container } = render(DitherDefs, { id: 'p1', cell: 3 });
    const pattern = container.querySelector('pattern#p1')!;
    expect(pattern).toHaveAttribute('width', '6');
    expect(pattern).toHaveAttribute('height', '6');
    expect(pattern).toHaveAttribute('patternUnits', 'userSpaceOnUse');
    expect(pattern.querySelectorAll('rect')).toHaveLength(2);
  });
});
