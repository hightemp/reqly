// The desktop shell installs updates (apps/desktop/src/updater.rs); the
// browser and Android have no installer.

import { isMobile, isTauri } from '$lib/api/transport';
import type { Installer } from '$lib/stores/updates.svelte';

interface Progress {
  downloaded: number;
  total: number | null;
}

/** The parts of `@tauri-apps/api/core` used here (tests pass a fake). */
export interface TauriCore {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  Channel: new <T>() => { onmessage: (message: T) => void };
}

export function tauriInstaller(core: TauriCore): Installer {
  return {
    installable: () => core.invoke<boolean>('update_installable', undefined),
    async install(url, onProgress) {
      const channel = new core.Channel<Progress>();
      channel.onmessage = (p) => onProgress(p.downloaded, p.total);
      await core.invoke('update_install', { url, onProgress: channel });
    },
    restart: () => core.invoke('update_restart', undefined),
  };
}

/** The installer of the desktop app, `null` elsewhere. */
export async function platformInstaller(): Promise<Installer | null> {
  if (!isTauri() || isMobile()) return null;
  const core = await import('@tauri-apps/api/core');
  return tauriInstaller(core as unknown as TauriCore);
}
