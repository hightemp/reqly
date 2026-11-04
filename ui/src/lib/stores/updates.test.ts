import { afterEach, describe, expect, it, vi } from 'vitest';
import type { UpdateCheckOutput } from '$lib/api/generated/UpdateCheckOutput';
import type { UpdateRelease } from '$lib/api/generated/UpdateRelease';
import { FakeTransport } from '$lib/testing/fakeTransport';
import { UpdatesStore, type Installer } from './updates.svelte';

const release = (version: string, assets = ['latest.json']): UpdateRelease => ({
  version,
  tag: `v${version}`,
  name: `Reqly v${version}`,
  notes: `Notes of ${version}`,
  url: `https://github.com/hightemp/reqly/releases/tag/v${version}`,
  publishedAt: '2026-11-04T10:00:00Z',
  prerelease: false,
  assets: assets.map((name) => ({ name, url: `https://dl.test/v${version}/${name}`, size: 1 })),
});

const out = (r: UpdateRelease | null): UpdateCheckOutput => ({
  enabled: true,
  current: '0.9.0',
  checkedAt: 1000,
  release: r,
});

function setup(answer: () => UpdateCheckOutput | Promise<UpdateCheckOutput>) {
  const transport = new FakeTransport();
  let settings: unknown = null;
  transport.handle('update.check', answer);
  transport.handle('settings.get', () => ({ value: settings }));
  transport.handle('settings.set', (input) => {
    settings = input.value;
    return {};
  });
  const progress: ((downloaded: number, total: number | null) => void)[] = [];
  const installer: Installer & { install: ReturnType<typeof vi.fn> } = {
    installable: vi.fn(async () => true),
    install: vi.fn(async (_url: string, onProgress) => {
      progress.push(onProgress);
      onProgress(50, 100);
      onProgress(100, 100);
    }),
    restart: vi.fn(async () => {}),
  };
  const found: UpdateRelease[] = [];
  const store = new UpdatesStore(transport, installer, { onFound: (r) => found.push(r) });
  return {
    transport,
    store,
    installer,
    found,
    settings: () => settings,
    checks: () => transport.calls.filter((c) => c.name === 'update.check'),
  };
}

describe('UpdatesStore', () => {
  afterEach(() => vi.useRealTimers());

  it('automatic checks at start and every hour notify once per version', async () => {
    vi.useFakeTimers();
    const fx = setup(() => out(release('0.10.0')));
    await fx.store.start();
    expect(fx.checks()).toEqual([{ name: 'update.check', input: { manual: false } }]);
    expect(fx.store.status).toBe('available');
    expect(fx.store.release?.version).toBe('0.10.0');
    expect(fx.found.map((r) => r.version)).toEqual(['0.10.0']);
    expect(fx.store.installable).toBe(true);

    await vi.advanceTimersByTimeAsync(60 * 60 * 1000);
    expect(fx.checks()).toHaveLength(2);
    expect(fx.found).toHaveLength(1);
    fx.store.stop();
    await vi.advanceTimersByTimeAsync(2 * 60 * 60 * 1000);
    expect(fx.checks()).toHaveLength(2);
  });

  it('a manual check reports up to date and errors', async () => {
    let answer: () => UpdateCheckOutput = () => out(null);
    const fx = setup(() => answer());
    await fx.store.check(true);
    expect(fx.checks()[0]?.input).toEqual({ manual: true });
    expect(fx.store.status).toBe('upToDate');
    expect(fx.found).toEqual([]);

    answer = () => {
      throw { code: 'update.network', message: 'down', params: {} };
    };
    await fx.store.check(true);
    expect(fx.store.status).toBe('error');
    expect(fx.store.error).toBeTruthy();
  });

  it('automatic check errors stay quiet', async () => {
    const fx = setup(() => {
      throw { code: 'update.network', message: 'down', params: {} };
    });
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    await fx.store.check(false);
    expect(fx.store.status).toBe('idle');
    expect(fx.store.error).toBeNull();
  });

  it('turned off for this build', async () => {
    const fx = setup(() => ({ ...out(null), enabled: false }));
    await fx.store.check(false);
    expect(fx.store.enabled).toBe(false);
    expect(fx.store.status).toBe('idle');
  });

  it('downloads with progress, then restarts', async () => {
    const fx = setup(() => out(release('0.10.0')));
    await fx.store.check(true);
    expect(fx.store.canInstall).toBe(false); // installable not loaded yet
    await fx.store.start();
    expect(fx.store.canInstall).toBe(true);
    await fx.store.install();
    expect(fx.installer.install).toHaveBeenCalledWith(
      'https://dl.test/v0.10.0/latest.json',
      expect.any(Function),
    );
    expect(fx.store.progress).toEqual({ downloaded: 100, total: 100 });
    expect(fx.store.status).toBe('ready');
    await fx.store.restart();
    expect(fx.installer.restart).toHaveBeenCalled();
  });

  it('a failed install is an error', async () => {
    const fx = setup(() => out(release('0.10.0')));
    await fx.store.start();
    fx.installer.install.mockRejectedValueOnce(new Error('bad signature'));
    await fx.store.install();
    expect(fx.store.status).toBe('error');
    expect(fx.store.error).toContain('bad signature');
  });

  it('installs on its own when auto-install is on', async () => {
    const fx = setup(() => out(release('0.10.0')));
    await fx.store.saveSettings({ autoInstall: true });
    await fx.store.start();
    expect(fx.installer.install).toHaveBeenCalled();
    expect(fx.store.status).toBe('ready');
  });

  it('releases without updater bundles are downloaded by hand', async () => {
    const fx = setup(() => out(release('0.10.0', ['Reqly_0.10.0_android.apk'])));
    await fx.store.start();
    expect(fx.store.canInstall).toBe(false);
    expect(fx.store.downloadUrl).toBe('https://github.com/hightemp/reqly/releases/tag/v0.10.0');
  });

  it('skipping a version saves it and hides the update', async () => {
    const fx = setup(() => out(release('0.10.0')));
    await fx.store.start();
    fx.store.dialogOpen = true;
    await fx.store.skip();
    expect(fx.settings()).toMatchObject({ skippedVersion: '0.10.0' });
    expect(fx.store.release).toBeNull();
    expect(fx.store.status).toBe('idle');
    expect(fx.store.dialogOpen).toBe(false);
  });

  it('settings merge with the defaults', async () => {
    const fx = setup(() => out(null));
    await fx.store.loadSettings();
    expect(fx.store.settings).toMatchObject({
      autoCheck: true,
      autoInstall: false,
      channel: 'stable',
      intervalHours: 24,
    });
    await fx.store.saveSettings({ channel: 'prerelease' });
    expect(fx.settings()).toMatchObject({ channel: 'prerelease', autoCheck: true });
  });
});

describe('UpdatesStore on Android', () => {
  it('links to the APK of the release', async () => {
    const transport = new FakeTransport();
    transport.handle('update.check', () =>
      out(release('0.10.0', ['Reqly_0.10.0_android-unsigned.apk', 'Reqly_0.10.0_android.aab'])),
    );
    transport.handle('settings.get', () => ({ value: null }));
    const store = new UpdatesStore(transport, null, { mobile: true });
    await store.start();
    expect(store.canInstall).toBe(false);
    expect(store.downloadUrl).toBe('https://dl.test/v0.10.0/Reqly_0.10.0_android-unsigned.apk');
  });
});
