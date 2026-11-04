// Update checks (`update.check`) and installing on desktop (docs/updates.md).

import type { UpdateRelease } from '$lib/api/generated/UpdateRelease';
import type { UpdateSettings } from '$lib/api/generated/UpdateSettings';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';

export const UPDATE_SETTINGS_KEY = 'updates';
/** Automatic checks ask the core this often; it applies the real interval. */
const TICK_MS = 60 * 60 * 1000;

export const DEFAULT_UPDATE_SETTINGS: UpdateSettings = {
  autoCheck: true,
  autoInstall: false,
  channel: 'stable',
  intervalHours: 24,
  skippedVersion: null,
};

/** Installs updates in place (the desktop Tauri updater). */
export interface Installer {
  /** False for builds the updater cannot replace (deb, rpm). */
  installable(): Promise<boolean>;
  install(
    manifestUrl: string,
    onProgress: (downloaded: number, total: number | null) => void,
  ): Promise<void>;
  restart(): Promise<void>;
}

export type UpdateStatus =
  'idle' | 'checking' | 'upToDate' | 'available' | 'downloading' | 'ready' | 'error';

export interface UpdatesOptions {
  /** Android: offer the APK of the release. */
  mobile?: boolean;
  /** An automatic check found a version not announced before. */
  onFound?: (release: UpdateRelease) => void;
}

export class UpdatesStore {
  status = $state<UpdateStatus>('idle');
  enabled = $state(true);
  current = $state<string | null>(null);
  checkedAt = $state<number | null>(null);
  release = $state<UpdateRelease | null>(null);
  error = $state<string | null>(null);
  progress = $state<{ downloaded: number; total: number | null } | null>(null);
  settings = $state<UpdateSettings>({ ...DEFAULT_UPDATE_SETTINGS });
  installable = $state(false);
  dialogOpen = $state(false);
  /** An automatic check found a version not announced before. */
  onFound: ((release: UpdateRelease) => void) | undefined;
  private timer: ReturnType<typeof setInterval> | null = null;
  private announced: string | null = null;

  constructor(
    private readonly transport: Transport,
    private readonly installer: Installer | null,
    private readonly options: UpdatesOptions = {},
  ) {
    this.onFound = options.onFound;
  }

  /** The updater manifest of the release, when it has one. */
  get manifestUrl(): string | null {
    return this.release?.assets.find((a) => a.name === 'latest.json')?.url ?? null;
  }

  get canInstall(): boolean {
    return this.installable && this.manifestUrl !== null;
  }

  /** Where to get the release by hand: the APK on Android, else its page. */
  get downloadUrl(): string | null {
    if (!this.release) return null;
    if (this.options.mobile) {
      const apk = this.release.assets.find((a) => /_android[^/]*\.apk$/.test(a.name));
      if (apk) return apk.url;
    }
    return this.release.url;
  }

  /** Loads settings, checks now and then every hour. */
  async start() {
    await this.loadSettings().catch((e: unknown) => console.warn('[updates] settings', e));
    this.installable = (await this.installer?.installable().catch(() => false)) ?? false;
    await this.check(false);
    this.stop();
    this.timer = setInterval(() => void this.check(false), TICK_MS);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  async loadSettings() {
    const { value } = await this.transport.call('settings.get', { key: UPDATE_SETTINGS_KEY });
    this.settings = { ...DEFAULT_UPDATE_SETTINGS, ...((value as Partial<UpdateSettings>) ?? {}) };
  }

  async saveSettings(patch: Partial<UpdateSettings>) {
    this.settings = { ...this.settings, ...patch };
    await this.transport.call('settings.set', {
      key: UPDATE_SETTINGS_KEY,
      value: this.settings,
    });
  }

  /** `manual`: "Check for updates" (errors and skipped versions show). */
  async check(manual: boolean) {
    if (this.status === 'downloading' || this.status === 'ready') return;
    if (manual) {
      this.status = 'checking';
      this.error = null;
    }
    try {
      const out = await this.transport.call('update.check', { manual });
      this.enabled = out.enabled;
      this.current = out.current;
      this.checkedAt = out.checkedAt;
      this.release = out.release;
      this.error = null;
      if (!out.enabled) {
        this.status = 'idle';
      } else if (out.release) {
        this.status = 'available';
        if (!manual && this.announced !== out.release.version) {
          this.announced = out.release.version;
          if (this.settings.autoInstall && this.canInstall) await this.install();
          else this.onFound?.(out.release);
        }
      } else {
        this.status = manual ? 'upToDate' : 'idle';
      }
    } catch (e) {
      if (manual) {
        this.status = 'error';
        this.error = errorMessage(e);
      } else {
        console.warn('[updates] check failed', e);
        if (this.status === 'checking') this.status = 'idle';
      }
    }
  }

  async install() {
    const url = this.manifestUrl;
    if (!this.installer || !url) return;
    this.status = 'downloading';
    this.error = null;
    this.progress = { downloaded: 0, total: null };
    try {
      await this.installer.install(url, (downloaded, total) => {
        this.progress = { downloaded, total };
      });
      this.status = 'ready';
    } catch (e) {
      this.status = 'error';
      this.error = errorMessage(e);
    }
  }

  async restart() {
    await this.installer?.restart();
  }

  /** Don't offer this version again (newer ones still show). */
  async skip() {
    if (!this.release) return;
    await this.saveSettings({ skippedVersion: this.release.version });
    this.release = null;
    this.status = 'idle';
    this.dialogOpen = false;
  }
}
