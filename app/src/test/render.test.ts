import { screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import AppProbe from './AppProbe.svelte';
import { renderWithApp } from './render';

test('renderWithApp puts a connected AppState in context for any component', async () => {
  const { app } = await renderWithApp(AppProbe, { scenario: 'no-device', props: { label: 'now' } });
  expect(app.device.kind).toBe('none');
  expect(screen.getByText('now: gate-none')).toBeInTheDocument();
});
