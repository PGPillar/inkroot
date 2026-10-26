import { render, type RenderResult } from '@testing-library/svelte';
import type { Component } from 'svelte';
import { createMockBackend, createMockPlatform } from '../lib/mock/engine';
import { SCENARIOS } from '../lib/mock/scenarios';
import { AppState } from '../lib/state.svelte';

/**
 * Renders any component with an AppState in the 'inkroot' context. Without `app`, builds one on the
 * mock backend for `scenario` (default "ready") and connects it before rendering.
 */
export async function renderWithApp<C>(
  component: C,
  opts: { scenario?: string; props?: object; app?: AppState } = {},
): Promise<RenderResult<Component> & { app: AppState }> {
  let app = opts.app;
  if (!app) {
    const scenario = SCENARIOS[opts.scenario ?? 'ready'];
    if (!scenario) throw new Error(`Unknown mock scenario "${opts.scenario}"`);
    app = new AppState(createMockBackend(scenario), createMockPlatform(scenario));
    await app.connect();
  }
  const result = render(component as Component, {
    props: (opts.props ?? {}) as never,
    context: new Map([['inkroot', app]]),
  });
  return Object.assign(result, { app });
}
