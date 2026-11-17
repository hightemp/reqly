// Reads the clipboard: through the Tauri plugin in the app (the webview may
// refuse `navigator.clipboard.readText`), the Clipboard API on the web.

import { isTauri } from '$lib/api/transport';

/** The clipboard text, or null when it cannot be read here. */
export async function readClipboardText(): Promise<string | null> {
  try {
    if (isTauri()) {
      const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
      return await readText();
    }
    if (!navigator.clipboard?.readText) return null;
    return await navigator.clipboard.readText();
  } catch {
    return null;
  }
}
