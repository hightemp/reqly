import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { UpdateRelease } from '$lib/api/generated/UpdateRelease';
import { UpdatesStore, type Installer } from '$lib/stores/updates.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import UpdateDialog from './UpdateDialog.svelte';

const release: UpdateRelease = {
  version: '0.10.0',
  tag: 'v0.10.0',
  name: 'Reqly v0.10.0',
  notes: '### Added\n- Updates',
  url: 'https://github.com/hightemp/reqly/releases/tag/v0.10.0',
  publishedAt: '2026-11-04T10:00:00Z',
  prerelease: false,
  assets: [{ name: 'latest.json', url: 'https://dl.test/latest.json', size: 1 }],
};

async function setup(installable: boolean) {
  const transport = new FakeTransport();
  let settings: unknown = null;
  transport.handle('update.check', () => ({
    enabled: true,
    current: '0.9.0',
    checkedAt: 1,
    release,
  }));
  transport.handle('settings.get', () => ({ value: settings }));
  transport.handle('settings.set', (i) => {
    settings = i.value;
    return {};
  });
  let finish!: () => void;
  const installer: Installer = {
    installable: async () => installable,
    install: vi.fn(async (_url, onProgress) => {
      onProgress(25, 100);
      await new Promise<void>((r) => (finish = r));
    }),
    restart: vi.fn(async () => {}),
  };
  const updates = new UpdatesStore(transport, installer);
  await updates.start();
  updates.stop();
  updates.dialogOpen = true;
  const onopen = vi.fn();
  render(UpdateDialog, { updates, onopen });
  return { updates, installer, onopen, finish: () => finish(), settings: () => settings };
}

describe('UpdateDialog', () => {
  it('shows the release and installs it with progress', async () => {
    const fx = await setup(true);
    expect(screen.getByText('Reqly 0.10.0 is available. You have 0.9.0.')).toBeTruthy();
    expect(screen.getByText(/- Updates/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Update' }));
    expect(fx.installer.install).toHaveBeenCalled();
    expect(await screen.findByRole('progressbar', { name: 'Download progress' })).toBeTruthy();
    fx.finish();
    await fireEvent.click(await screen.findByRole('button', { name: 'Restart now' }));
    expect(fx.installer.restart).toHaveBeenCalled();
  });

  it('builds that cannot update themselves link to the download', async () => {
    const fx = await setup(false);
    expect(screen.getByText(/cannot update itself/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Update' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Download' }));
    expect(fx.onopen).toHaveBeenCalledWith(release.url);
  });

  it('opens the release page', async () => {
    const fx = await setup(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Release page' }));
    expect(fx.onopen).toHaveBeenCalledWith(release.url);
  });

  it('skips the version', async () => {
    const fx = await setup(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Skip this version' }));
    await waitFor(() => expect(fx.updates.dialogOpen).toBe(false));
    expect(fx.settings()).toMatchObject({ skippedVersion: '0.10.0' });
  });

  it('later closes the dialog', async () => {
    const fx = await setup(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(fx.updates.dialogOpen).toBe(false);
    expect(fx.updates.release?.version).toBe('0.10.0');
  });
});
