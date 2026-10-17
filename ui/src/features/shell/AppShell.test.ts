import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it } from 'vitest';
import { i18n } from '$lib/i18n';
import { PREFERENCES_KEY } from '$lib/preferences.svelte';
import { createServices } from '$lib/services.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import AppShell from './AppShell.svelte';

const media = { matches: false, addEventListener() {}, removeEventListener() {} };

async function setup() {
  const transport = new FakeTransport();
  const saved = transport.withSettings();
  const services = await createServices(transport, {
    platform: 'linux',
    media,
    root: document.createElement('div'),
  });
  render(AppShell, { services });
  return { services, saved };
}

describe('AppShell', () => {
  beforeEach(() => i18n.setLocale('en'));

  it('shows sidebar, request and response areas', async () => {
    await setup();
    expect(screen.getByRole('complementary', { name: 'Collections' })).toBeInTheDocument();
    expect(screen.getByText('Select or create a request')).toBeInTheDocument();
    expect(screen.getByText('Send a request to see the response')).toBeInTheDocument();
  });

  it('toggles the sidebar with Mod+B and persists it', async () => {
    const { saved } = await setup();
    await fireEvent.keyDown(window, { code: 'KeyB', key: 'b', ctrlKey: true });
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    expect(saved.get(PREFERENCES_KEY)).toMatchObject({ sidebarCollapsed: true });
    await fireEvent.click(screen.getByRole('button', { name: 'Show sidebar' }));
    expect(screen.getByRole('complementary')).toBeInTheDocument();
  });

  it('switches response layout', async () => {
    const { services } = await setup();
    await fireEvent.keyDown(window, { code: 'KeyL', key: 'l', ctrlKey: true, altKey: true });
    expect(services.prefs.value.responseLayout).toBe('bottom');
  });

  it('cycles the theme preference', async () => {
    const { services } = await setup();
    const button = screen.getByRole('button', { name: /Theme/ });
    await fireEvent.click(button);
    expect(services.prefs.value.theme).toBe('light');
    await fireEvent.click(button);
    expect(services.prefs.value.theme).toBe('dark');
    await fireEvent.click(button);
    expect(services.prefs.value.theme).toBe('system');
  });

  it('switches language', async () => {
    await setup();
    await fireEvent.click(screen.getByRole('button', { name: /Language/ }));
    expect(await screen.findByText('Выберите или создайте запрос')).toBeInTheDocument();
  });
});
