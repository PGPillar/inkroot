import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, test, vi } from 'vitest';
import type { DeviceStatus, LogLine } from '../ipc';
import Button from './Button.svelte';
import DeviceChip from './DeviceChip.svelte';
import DetailsLog from './DetailsLog.svelte';
import JobCard from './JobCard.svelte';
import RefreshTransition from './RefreshTransition.svelte';
import SegmentedProgress from './SegmentedProgress.svelte';

const label = (t: string) => createRawSnippet(() => ({ render: () => `<span>${t}</span>` }));

describe('primitives', () => {
  test('danger button has heavy border class and warning icon', () => {
    const { container } = render(Button, { variant: 'danger', children: label('Erase') });
    const btn = screen.getByRole('button');
    expect(btn).toHaveClass('danger');
    expect(container.querySelector('[data-icon="warning"]')).not.toBeNull();
  });

  test('disabled button does not fire', async () => {
    const onclick = vi.fn();
    render(Button, { disabled: true, onclick, children: label('Go') });
    const btn = screen.getByRole('button');
    await fireEvent.click(btn);
    expect(onclick).not.toHaveBeenCalled();
    expect(btn).toHaveAttribute('aria-disabled', 'true');
    expect(btn).toBeDisabled();
  });

  test('segmented progress fills proportionally', () => {
    const { container } = render(SegmentedProgress, { done: 50, total: 100, segments: 24 });
    expect(container.querySelectorAll('[data-filled]')).toHaveLength(12);
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
  });

  test('segmented progress is indeterminate without total', () => {
    const { container } = render(SegmentedProgress, { meta: 'working' });
    expect(container.querySelector('.marching')).not.toBeNull();
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
    expect(screen.getByText('working')).toBeInTheDocument();
  });

  test('refresh transition remounts with flash on key change', async () => {
    const children = createRawSnippet(() => ({ render: () => `<p data-testid="c">hi</p>` }));
    const { rerender } = render(RefreshTransition, { key: 1, children });
    const old = screen.getByTestId('c').parentElement;
    await rerender({ key: 2, children });
    const next = screen.getByTestId('c').parentElement;
    expect(next).not.toBe(old);
    expect(next).toHaveClass('flash');
  });

  test('details log hides raw output until expanded and copies', async () => {
    const lines: LogLine[] = [
      { flowId: null, ts: '2026-10-25T10:11:12Z', stream: 'stderr', line: 'boom raw' },
    ];
    const onCopy = vi.fn().mockResolvedValue(true);
    render(DetailsLog, { lines, tech: 'adb reboot', onCopy });
    expect(screen.queryByText(/boom raw/)).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: /Details/ }));
    expect(screen.getByText(/10:11:12 ! boom raw/)).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    expect(onCopy).toHaveBeenCalled();
    expect(await screen.findByText('Copied ✓')).toBeInTheDocument();
  });

  test('device chip text', () => {
    const status = {
      kind: 'adb',
      gate: null,
      info: { model: 'NoteMax', displayName: 'Note Max', firmwareLabel: '4.2', slot: 'b', rooted: false },
    } as unknown as DeviceStatus;
    render(DeviceChip, { status });
    expect(screen.getByText('● NOTE MAX / FW 4.2 / SLOT_B')).toBeInTheDocument();
  });

  test('job card primary is inverted', () => {
    render(JobCard, { title: 'Root', desc: 'd', icon: 'root', primary: true, onclick: () => {} });
    expect(screen.getByRole('button')).toHaveClass('primary');
  });
});
