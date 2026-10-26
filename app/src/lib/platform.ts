import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { Unlisten } from './ipc';

export interface Platform {
  pickBootImage(): Promise<string | null>;
  openExternal(url: string): Promise<void>;
  onCloseRequested(allow: () => boolean): Promise<Unlisten>;
  closeWindow(): Promise<void>;
  /**
   * Puts text on the clipboard. Call it straight from the click handler: a promise is accepted so the
   * write can start inside the user gesture (WebKit refuses clipboard writes after it) while the text
   * is still being fetched.
   */
  copyText(text: string | Promise<string>): Promise<void>;
}

/** Clipboard write that works with text that isn't ready yet. */
export async function writeClipboard(text: string | Promise<string>): Promise<void> {
  const clipboard = navigator.clipboard;
  if (typeof text === 'string') return clipboard.writeText(text);
  if (typeof ClipboardItem !== 'undefined' && typeof clipboard.write === 'function') {
    const blob = text.then((t) => new Blob([t], { type: 'text/plain' }));
    blob.catch(() => {}); // Handled below via `await text`; keeps a failed fetch from going unhandled.
    try {
      // Synchronous up to here, so the write begins within the gesture.
      await clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
      return;
    } catch {
      // Promise-valued clipboard items aren't supported here (or the fetch failed); fall through.
    }
  }
  await clipboard.writeText(await text);
}

export function createTauriPlatform(): Platform {
  return {
    async pickBootImage() {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Boot image', extensions: ['img'] }],
      });
      return typeof picked === 'string' ? picked : null;
    },
    openExternal: (url) => openUrl(url),
    onCloseRequested: (allow) =>
      getCurrentWindow().onCloseRequested((event) => {
        if (!allow()) event.preventDefault();
      }),
    closeWindow: () => getCurrentWindow().destroy(),
    copyText: (text) => writeClipboard(text),
  };
}
