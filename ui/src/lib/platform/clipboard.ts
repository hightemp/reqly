// Reads the clipboard: through the Tauri plugin in the app (the webview may
// refuse `navigator.clipboard.readText`), the Clipboard API on the web.

import { isTauri } from '$lib/api/transport';

/**
 * The clipboard text, or `text: null` when it cannot be read here. `error`
 * is the app's reason (the web gives none worth showing: it only refuses).
 */
export type ClipboardRead = { text: string } | { text: null; error: string | null };

export async function readClipboardText(): Promise<ClipboardRead> {
  if (isTauri()) {
    try {
      const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
      return { text: await readText() };
    } catch (e) {
      console.warn('[clipboard] read failed', e);
      return { text: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
  try {
    if (!navigator.clipboard?.readText) return { text: null, error: null };
    return { text: await navigator.clipboard.readText() };
  } catch {
    return { text: null, error: null };
  }
}
