import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { HotkeyRegistry } from '$lib/hotkeys/hotkeys';
import HelpDialog from './HelpDialog.svelte';

function setup(page = 'getting-started') {
  const hotkeys = new HotkeyRegistry('linux');
  hotkeys.define({ id: 'palette.open', combos: ['Mod+K'], scope: 'global' });
  hotkeys.define({ id: 'tab.goto1', combos: ['Mod+1'], scope: 'global' });
  let closed = false;
  render(HelpDialog, { hotkeys, platform: 'linux', page, onclose: () => (closed = true) });
  return { closed: () => closed };
}

const article = () => screen.getByRole('article');

describe('HelpDialog', () => {
  it('shows the page and the table of contents', () => {
    setup();
    expect(screen.getByRole('heading', { level: 1, name: 'Getting started' })).toBeTruthy();
    const toc = screen.getByRole('navigation', { name: 'Contents' });
    expect(toc.textContent).toContain('Variables and environments');
    expect(screen.getByRole('link', { name: 'Getting started', current: 'page' })).toBeTruthy();
  });

  it('follows help links and goes back', async () => {
    setup();
    await fireEvent.click(screen.getAllByRole('link', { name: 'Variables and environments' })[1]!);
    expect(
      screen.getByRole('heading', { level: 1, name: 'Variables and environments' }),
    ).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('heading', { level: 1, name: 'Getting started' })).toBeTruthy();
  });

  it('opens a page from the contents', async () => {
    setup();
    const toc = screen.getByRole('navigation', { name: 'Contents' });
    await fireEvent.click(within(toc).getByRole('link', { name: 'Scripts' }));
    expect(article().textContent).toContain('rq.test');
  });

  it('searches the pages', async () => {
    setup();
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search help' }), {
      target: { value: 'jq' },
    });
    const results = screen.getByRole('navigation', { name: 'Search results' });
    expect(results.textContent).toContain('Tests and the runner');
    await fireEvent.click(screen.getByRole('link', { name: /Tests and the runner/ }));
    expect(screen.getByRole('heading', { level: 1, name: 'Tests and the runner' })).toBeTruthy();
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search help' }), {
      target: { value: 'zzzz-nothing' },
    });
    expect(screen.getByText('Nothing found')).toBeTruthy();
  });

  it('lists the current shortcuts', () => {
    setup('shortcuts');
    const table = screen.getByRole('table', { name: 'Keyboard shortcuts' });
    expect(table.textContent).toContain('Command palette');
    expect(table.textContent).toContain('Ctrl+K');
    expect(table.textContent).toContain('Go to tab 1');
  });

  it('opens a page at an anchor', async () => {
    setup('variables#secrets');
    await waitFor(() =>
      expect(
        screen.getByRole('heading', { level: 1, name: 'Variables and environments' }),
      ).toBeTruthy(),
    );
    expect(article().querySelector('#secrets')).toBeTruthy();
  });
});
