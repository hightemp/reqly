// Opens a web page outside the app: the system browser on desktop and
// Android, a new tab on the web.

import { isTauri } from '$lib/api/transport';

export async function openExternal(url: string) {
  if (isTauri()) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } else {
    window.open(url, '_blank', 'noopener');
  }
}
