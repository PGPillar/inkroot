import { render } from '@testing-library/svelte';
import type { Component } from 'svelte';
import { describe, expect, test } from 'vitest';
import * as art from './index';
import { DeepFlashCable, DitherDefs, Tablet } from './index';

// Every illustration exported from index.ts; DitherDefs is a <pattern> helper, not art.
const ALL_ART = Object.entries(art).filter(([name]) => name !== 'DitherDefs') as [
  string,
  Component<{ title: string }>,
][];

const patternIdOf = (svg: Element) => svg.querySelector('pattern')!.getAttribute('id')!;

describe('illustrations', () => {
  test('index exports all five illustrations plus DitherDefs', () => {
    expect(ALL_ART.map(([n]) => n).sort()).toEqual(
      ['DeepFlashCable', 'MenuSketch', 'PressIndicator', 'Tablet', 'UsbAPort'],
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

  test('DeepFlashCable draws the website cable: dithered grips and box, solid button, HOLD cue', () => {
    const svg = render(DeepFlashCable, { title: 'cable' }).container.querySelector('svg')!;
    const ids = [...svg.querySelectorAll('pattern')].map((p) => p.getAttribute('id'));
    expect(new Set(ids).size).toBe(2);
    for (const part of ['usbAGrip', 'box', 'usbCGrip']) {
      expect(ids.map((id) => `url(#${id})`)).toContain(svg.querySelector(`[data-part=${part}]`)!.getAttribute('fill'));
    }
    expect(svg.querySelector('[data-part=button]')).toHaveAttribute('fill', 'currentColor');
    expect(svg.querySelector('[data-part=holdArrow]')).not.toBeNull();
    expect(svg.textContent).toMatch(/HOLD.*USB-A.*COMPUTER.*BUTTON.*USB-C.*TABLET/s);
  });

  test('DeepFlashCable without cue or labels draws only the cable', () => {
    const svg = render(DeepFlashCable, { title: 'cable', cue: 'none', labels: false }).container.querySelector('svg')!;
    expect(svg.querySelector('[data-part=holdArrow]')).toBeNull();
    expect(svg.querySelector('text')).toBeNull();
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
