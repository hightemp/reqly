import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { WindowController } from '$lib/platform/window';
import Titlebar from './Titlebar.svelte';

function controller(maximized = false) {
  let onResized: (() => void) | undefined;
  const c = {
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {
      maximized = !maximized;
      onResized?.();
    }),
    close: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => maximized),
    onResized: vi.fn(async (l: () => void) => {
      onResized = l;
      return () => {};
    }),
  } satisfies WindowController;
  return c;
}

describe('Titlebar', () => {
  it('drives the window', async () => {
    const c = controller();
    render(Titlebar, { controller: c, platform: 'linux' });
    await fireEvent.click(screen.getByRole('button', { name: 'Minimize' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(c.minimize).toHaveBeenCalledOnce();
    expect(c.close).toHaveBeenCalledOnce();
  });

  it('toggles between maximize and restore', async () => {
    const c = controller();
    render(Titlebar, { controller: c, platform: 'linux' });
    await fireEvent.click(await screen.findByRole('button', { name: 'Maximize' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Restore' })).toBeInTheDocument(),
    );
  });

  it('maximizes on double click of the drag region', async () => {
    const c = controller();
    render(Titlebar, { controller: c, platform: 'linux' });
    await fireEvent.dblClick(screen.getByTestId('titlebar-drag'));
    expect(c.toggleMaximize).toHaveBeenCalledOnce();
  });

  it('leaves window buttons to macOS', () => {
    render(Titlebar, { controller: controller(), platform: 'mac' });
    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
  });
});
