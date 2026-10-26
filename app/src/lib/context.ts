import { getContext, setContext } from 'svelte';
import type { AppState } from './state.svelte';

const KEY = 'inkroot';

export function setApp(app: AppState): void {
  setContext(KEY, app);
}

export function getApp(): AppState {
  const app = getContext<AppState | undefined>(KEY);
  if (!app) throw new Error('getApp() called outside an Inkroot app context');
  return app;
}
