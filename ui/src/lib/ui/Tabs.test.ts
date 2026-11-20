import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Tabs from './Tabs.svelte';

const tabs = ['params', 'body', 'auth', 'headers', 'scripts'].map((id) => ({ id, label: id }));

describe('Tabs in a narrow panel', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows scroll arrows and scrolls with the wheel when the tabs do not fit', async () => {
    vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute('role') === 'tablist' ? 400 : 0;
    });
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(150);
    render(Tabs, { tabs, value: 'params', label: 'Sections' });

    const left = await screen.findByRole('button', { name: 'Scroll tabs left' });
    const right = screen.getByRole('button', { name: 'Scroll tabs right' });
    expect((left as HTMLButtonElement).disabled).toBe(true);
    expect((right as HTMLButtonElement).disabled).toBe(false);

    const list = screen.getByRole('tablist');
    await fireEvent.wheel(list, { deltaY: 100 });
    expect(list.scrollLeft).toBe(100);
    await fireEvent.scroll(list);
    expect((left as HTMLButtonElement).disabled).toBe(false);
  });

  it('has no arrows when the tabs fit', () => {
    render(Tabs, { tabs, value: 'params', label: 'Sections' });
    expect(screen.queryByRole('button', { name: 'Scroll tabs left' })).toBeNull();
  });
});
