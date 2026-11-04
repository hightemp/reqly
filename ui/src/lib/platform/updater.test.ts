import { describe, expect, it, vi } from 'vitest';
import { tauriInstaller, type TauriCore } from './updater';

function fakeCore(onInvoke: (cmd: string, args?: Record<string, unknown>) => unknown) {
  class Channel<T> {
    onmessage: (m: T) => void = () => {};
  }
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => onInvoke(cmd, args));
  return { core: { invoke, Channel } as unknown as TauriCore, invoke };
}

describe('tauriInstaller', () => {
  it('asks the shell whether this build can update itself', async () => {
    const { core, invoke } = fakeCore(() => false);
    expect(await tauriInstaller(core).installable()).toBe(false);
    expect(invoke).toHaveBeenCalledWith('update_installable', undefined);
  });

  it('installs with progress from the channel', async () => {
    const { core, invoke } = fakeCore((cmd, args) => {
      if (cmd !== 'update_install') return undefined;
      const channel = args?.onProgress as { onmessage: (m: unknown) => void };
      channel.onmessage({ downloaded: 10, total: 20 });
      channel.onmessage({ downloaded: 20, total: null });
      return undefined;
    });
    const seen: [number, number | null][] = [];
    await tauriInstaller(core).install('https://dl.test/latest.json', (d, t) => seen.push([d, t]));
    expect(invoke).toHaveBeenCalledWith('update_install', {
      url: 'https://dl.test/latest.json',
      onProgress: expect.anything(),
    });
    expect(seen).toEqual([
      [10, 20],
      [20, null],
    ]);
  });

  it('restarts the app', async () => {
    const { core, invoke } = fakeCore(() => undefined);
    await tauriInstaller(core).restart();
    expect(invoke).toHaveBeenCalledWith('update_restart', undefined);
  });
});
