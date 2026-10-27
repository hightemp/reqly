import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { beforeEach, describe, expect, it } from 'vitest';
import { i18n } from '$lib/i18n';
import { PREFERENCES_KEY } from '$lib/preferences.svelte';
import { createServices } from '$lib/services.svelte';
import { Viewport } from '$lib/stores/viewport.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import AppShell from './AppShell.svelte';

const media = { matches: false, addEventListener() {}, removeEventListener() {} };

function request(id: string, name: string) {
  return {
    id,
    version: 1,
    collectionId: 'c1',
    parentId: null,
    name,
    sortKey: 'V',
    createdAt: 0,
    updatedAt: 0,
    deletedAt: null,
    data: {
      kind: 'request',
      method: 'GET',
      url: 'http://a',
      params: [],
      pathParams: [],
      headers: [],
      body: { kind: 'none', text: '', fields: [], file: null },
      auth: { type: 'inherit' },
      settings: {},
      scripts: { preRequest: '', afterResponse: '' },
      variables: {},
      eventStream: false,
      description: '',
    },
  };
}

async function setup({ compact = false } = {}) {
  const transport = new FakeTransport();
  const saved = transport.withSettings();
  transport.withEmptyWorkspace();
  const nodes = [request('r1', 'First'), request('r2', 'Second')];
  transport.handle('node.get', ({ id }) => nodes.find((n) => n.id === id) as never);
  transport.handle('response.list', () => [] as never);
  const services = await createServices(transport, {
    platform: 'linux',
    media,
    root: document.createElement('div'),
    viewport: new Viewport({ compact: { ...media, matches: compact } }),
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

  it('opens settings from the sidebar toolbar', async () => {
    await setup();
    const sidebar = screen.getByRole('complementary', { name: 'Collections' });
    const button = [...sidebar.querySelectorAll('button')].find(
      (b) => b.getAttribute('aria-label') === 'Settings',
    );
    expect(button).toBeTruthy();
    await fireEvent.click(button!);
    expect(await screen.findByRole('dialog', { name: 'Settings' })).toBeInTheDocument();
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

  it('saves open tabs at once when the page unloads', async () => {
    const { services, saved } = await setup();
    services.tabs.open('r1', { preview: false });
    window.dispatchEvent(new Event('beforeunload'));
    await tick();
    expect(saved.get('ui.tabs.w1')).toMatchObject({ tabs: [{ id: 'r1' }], activeId: 'r1' });
  });

  describe('compact layout', () => {
    it('keeps the sidebar in a drawer', async () => {
      await setup({ compact: true });
      expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
      await fireEvent.click(screen.getByRole('button', { name: 'Menu' }));
      const drawer = screen.getByRole('dialog', { name: 'Collections' });
      expect(drawer).toContainElement(screen.getByRole('complementary'));
      await fireEvent.click(screen.getByRole('button', { name: 'Close menu' }));
      expect(screen.queryByRole('dialog', { name: 'Collections' })).not.toBeInTheDocument();
    });

    it('opens the drawer with a swipe from the left edge and closes it with a swipe left', async () => {
      await setup({ compact: true });
      const touch = { pointerType: 'touch', isPrimary: true };
      const main = screen.getByRole('main');
      await fireEvent.pointerDown(main, { ...touch, clientX: 200, clientY: 300 });
      await fireEvent.pointerUp(window, { ...touch, clientX: 330, clientY: 300 });
      expect(screen.queryByRole('dialog', { name: 'Collections' })).not.toBeInTheDocument();

      await fireEvent.pointerDown(main, { ...touch, clientX: 4, clientY: 300 });
      await fireEvent.pointerUp(window, { ...touch, clientX: 160, clientY: 310 });
      const drawer = screen.getByRole('dialog', { name: 'Collections' });

      await fireEvent.pointerDown(drawer, { ...touch, clientX: 250, clientY: 300 });
      await fireEvent.pointerUp(window, { ...touch, clientX: 60, clientY: 300 });
      expect(screen.queryByRole('dialog', { name: 'Collections' })).not.toBeInTheDocument();
    });

    it('closes the drawer when a request is opened', async () => {
      const { services } = await setup({ compact: true });
      await fireEvent.click(screen.getByRole('button', { name: 'Menu' }));
      services.tabs.open('r1');
      await tick();
      expect(screen.queryByRole('dialog', { name: 'Collections' })).not.toBeInTheDocument();
    });

    it('shows the request or the response', async () => {
      await setup({ compact: true });
      const request = screen.getByRole('button', { name: 'Request' });
      const response = screen.getByRole('button', { name: 'Response' });
      expect(request).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByText('Select or create a request')).toBeInTheDocument();
      expect(screen.queryByText('Send a request to see the response')).not.toBeInTheDocument();
      await fireEvent.click(response);
      expect(response).toHaveAttribute('aria-pressed', 'true');
      expect(screen.queryByText('Select or create a request')).not.toBeInTheDocument();
      expect(screen.getByText('Send a request to see the response')).toBeInTheDocument();
    });

    it('switches to the response when a request is sent', async () => {
      const { services } = await setup({ compact: true });
      services.tabs.open('r1');
      await tick();
      services.responses.run('r1').sending = true;
      await tick();
      expect(screen.getByRole('button', { name: 'Response' })).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });

    it('lists open requests instead of a tab strip', async () => {
      const { services } = await setup({ compact: true });
      services.tabs.open('r1', { preview: false });
      services.tabs.open('r2', { preview: false });
      await tick();
      expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
      await fireEvent.click(await screen.findByRole('button', { name: 'Open requests: 2' }));
      const list = screen.getByRole('dialog', { name: 'Open requests' });
      await fireEvent.click(within(list).getByRole('button', { name: /First/ }));
      expect(services.tabs.activeId).toBe('r1');
      expect(screen.queryByRole('dialog', { name: 'Open requests' })).not.toBeInTheDocument();

      await fireEvent.click(screen.getByRole('button', { name: 'Open requests: 2' }));
      const again = screen.getByRole('dialog', { name: 'Open requests' });
      await fireEvent.click(within(again).getAllByRole('button', { name: 'Close' })[1]!);
      expect(services.tabs.tabs.map((t) => t.id)).toEqual(['r1']);
    });
  });
});
