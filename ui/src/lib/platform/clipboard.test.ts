import { afterEach, describe, expect, it, vi } from 'vitest';
import { readClipboardText } from './clipboard';

const invoke = vi.fn();
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ readText: () => invoke() }));

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  invoke.mockReset();
});

describe('readClipboardText', () => {
  it('reads through the Clipboard API on the web', async () => {
    Object.assign(navigator, { clipboard: { readText: vi.fn(async () => 'curl x') } });
    expect(await readClipboardText()).toEqual({ text: 'curl x' });
  });

  it('reports that the web page may not read the clipboard', async () => {
    Object.assign(navigator, {
      clipboard: {
        readText: vi.fn(async () => {
          throw new DOMException('denied', 'NotAllowedError');
        }),
      },
    });
    expect(await readClipboardText()).toEqual({ text: null, error: null });
  });

  it('reads through the Tauri plugin in the app and keeps its error', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    invoke.mockResolvedValueOnce('GET https://x.test');
    expect(await readClipboardText()).toEqual({ text: 'GET https://x.test' });
    invoke.mockRejectedValueOnce('The clipboard contents were not available');
    expect(await readClipboardText()).toEqual({
      text: null,
      error: 'The clipboard contents were not available',
    });
  });
});
