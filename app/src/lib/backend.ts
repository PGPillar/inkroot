import { createTauriBackend, type Backend } from './ipc';
import { createMockBackend, createMockPlatform } from './mock/engine';
import { SCENARIOS } from './mock/scenarios';
import { createTauriPlatform, type Platform } from './platform';

/** Picks the scripted mock when `?mock=` is present or there is no Tauri runtime; otherwise the real backend. */
export function selectBackend(
  loc: Pick<Location, 'search'>,
  w: object,
): { backend: Backend; platform: Platform; mock: string | null } {
  const params = new URLSearchParams(loc.search);
  const requested = params.get('mock');
  if (requested === null && '__TAURI_INTERNALS__' in w) {
    return { backend: createTauriBackend(), platform: createTauriPlatform(), mock: null };
  }
  const name = requested ?? 'ready';
  const known = Object.hasOwn(SCENARIOS, name);
  if (!known) console.warn(`Unknown mock scenario "${name}", using "ready".`);
  const mock = known ? name : 'ready';
  const speed = Number(params.get('speed'));
  return {
    backend: createMockBackend(SCENARIOS[mock], { speed: speed > 0 ? speed : 1 }),
    platform: createMockPlatform(SCENARIOS[mock]),
    mock,
  };
}
