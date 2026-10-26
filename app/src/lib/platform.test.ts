import { afterEach, describe, expect, test, vi } from 'vitest';
import { writeClipboard } from './platform';

/** Stands in for ClipboardItem in jsdom, keeping what it was given. */
class FakeItem {
  constructor(readonly items: Record<string, Promise<Blob>>) {}
}

function stubClipboard(clipboard: Partial<Clipboard>, withItem = true) {
  vi.stubGlobal('navigator', { clipboard });
  if (withItem) vi.stubGlobal('ClipboardItem', FakeItem);
  else vi.stubGlobal('ClipboardItem', undefined);
}

afterEach(() => vi.unstubAllGlobals());

describe('writeClipboard', () => {
  test('a plain string uses writeText', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard({ writeText, write: vi.fn() });
    await writeClipboard('hello');
    expect(writeText).toHaveBeenCalledWith('hello');
  });

  test('pending text starts a clipboard write right away, before the text is ready', async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn();
    stubClipboard({ write, writeText });
    let resolve!: (t: string) => void;
    const done = writeClipboard(new Promise<string>((r) => (resolve = r)));
    expect(write).toHaveBeenCalledOnce(); // synchronously, inside the gesture
    const item = write.mock.calls[0][0][0] as FakeItem;
    resolve('the log');
    const blob = await item.items['text/plain'];
    expect(blob.type).toBe('text/plain');
    expect(await blob.text()).toBe('the log');
    await done;
    expect(writeText).not.toHaveBeenCalled();
  });

  test('falls back to writeText when promise items are refused', async () => {
    const write = vi.fn().mockRejectedValue(new Error('NotAllowed'));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard({ write, writeText });
    await writeClipboard(Promise.resolve('the log'));
    expect(writeText).toHaveBeenCalledWith('the log');
  });

  test('falls back to writeText when ClipboardItem is missing', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard({ writeText }, false);
    await writeClipboard(Promise.resolve('the log'));
    expect(writeText).toHaveBeenCalledWith('the log');
  });

  test('a text that fails to load rejects, without an unhandled rejection', async () => {
    const write = vi.fn(async (items: FakeItem[]) => {
      await items[0].items['text/plain'];
    });
    const writeText = vi.fn();
    stubClipboard({ write: write as unknown as Clipboard['write'], writeText });
    await expect(writeClipboard(Promise.reject(new Error('gone')))).rejects.toThrow('gone');
    expect(writeText).not.toHaveBeenCalled();
  });
});
