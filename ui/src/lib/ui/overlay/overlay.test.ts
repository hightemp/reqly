import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '$lib/i18n';
import Harness from './OverlayHarness.test.svelte';
import { dialogs } from './dialogs.svelte';
import { menus, type MenuEntry } from './menus.svelte';
import { toasts } from './toasts.svelte';

afterEach(() => {
  menus.close();
  toasts.clear();
  dialogs.closeAll();
  i18n.setLocale('en');
});

function items(log: string[]): MenuEntry[] {
  return [
    { id: 'new', label: 'New request', shortcut: 'Ctrl+N', onSelect: () => log.push('new') },
    { id: 'off', label: 'Disabled', disabled: true, onSelect: () => log.push('off') },
    { separator: true },
    {
      id: 'more',
      label: 'More',
      children: [{ id: 'dup', label: 'Duplicate', onSelect: () => log.push('dup') }],
    },
    { id: 'del', label: 'Delete', danger: true, onSelect: () => log.push('del') },
  ];
}

describe('context menu', () => {
  it('opens at a point and selects with the mouse', async () => {
    const log: string[] = [];
    render(Harness, {});
    menus.openAt({ x: 10, y: 10 }, items(log));
    await tick();
    expect(screen.getByRole('menu')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('menuitem', { name: /New request/ }));
    expect(log).toEqual(['new']);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('navigates with the keyboard skipping disabled items', async () => {
    const log: string[] = [];
    render(Harness, {});
    menus.openAt({ x: 10, y: 10 }, items(log));
    await tick();
    const menu = screen.getByRole('menu');
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: /New request/ })),
    );
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: /More/ }));
    await fireEvent.keyDown(menu, { key: 'End' });
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: /Delete/ }));
    await fireEvent.keyDown(menu, { key: 'Enter' });
    expect(log).toEqual(['del']);
  });

  it('opens submenus with ArrowRight and closes with Escape', async () => {
    const log: string[] = [];
    render(Harness, {});
    menus.openAt({ x: 10, y: 10 }, items(log));
    await tick();
    const more = screen.getByRole('menuitem', { name: /More/ });
    more.focus();
    await fireEvent.keyDown(more, { key: 'ArrowRight' });
    await tick();
    expect(screen.getAllByRole('menu')).toHaveLength(2);
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: 'Duplicate' })),
    );
    await fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' });
    await tick();
    expect(screen.getAllByRole('menu')).toHaveLength(1);
    await fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('typeahead focuses the matching item', async () => {
    render(Harness, {});
    menus.openAt({ x: 10, y: 10 }, items([]));
    await tick();
    await fireEvent.keyDown(screen.getByRole('menu'), { key: 'd' });
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: /Delete/ }));
  });

  it('closes on outside pointer down', async () => {
    render(Harness, {});
    menus.openAt({ x: 10, y: 10 }, items([]));
    await tick();
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});

describe('dialogs', () => {
  it('confirm resolves true on confirm and false on cancel/Escape', async () => {
    render(Harness, {});
    const yes = dialogs.confirm({
      title: 'Delete?',
      message: 'Gone forever',
      confirmLabel: 'Delete',
      danger: true,
    });
    await tick();
    expect(screen.getByRole('alertdialog', { name: 'Delete?' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await expect(yes).resolves.toBe(true);

    const no = dialogs.confirm({ title: 'Sure?' });
    await tick();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await expect(no).resolves.toBe(false);

    const esc = dialogs.confirm({ title: 'Esc?' });
    await tick();
    await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    await expect(esc).resolves.toBe(false);
  });

  it('prompt returns the entered value and validates', async () => {
    render(Harness, {});
    const p = dialogs.prompt({
      title: 'Rename',
      label: 'Name',
      value: 'old',
      validate: (v) => (v.trim() ? null : 'Required'),
    });
    await tick();
    const input = screen.getByRole('textbox', { name: 'Name' });
    await waitFor(() => expect(document.activeElement).toBe(input));
    await fireEvent.input(input, { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(screen.getByText('Required')).toBeInTheDocument();
    await fireEvent.input(input, { target: { value: 'new' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await expect(p).resolves.toBe('new');
  });

  it('traps focus inside the dialog', async () => {
    render(Harness, {});
    void dialogs.confirm({ title: 'Trap' });
    await tick();
    const [cancel, ok] = screen
      .getAllByRole('button')
      .filter((b) => b.closest('[role=alertdialog]'));
    ok!.focus();
    await fireEvent.keyDown(ok!, { key: 'Tab' });
    expect(document.activeElement).toBe(cancel);
    await fireEvent.keyDown(cancel!, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(ok);
  });
});

describe('toasts', () => {
  it('shows, auto-hides and runs actions', async () => {
    vi.useFakeTimers();
    render(Harness, {});
    const undo = vi.fn();
    toasts.show({
      message: 'Deleted 2 items',
      action: { label: 'Undo', run: undo },
      timeoutMs: 3000,
    });
    await tick();
    expect(screen.getByRole('status')).toHaveTextContent('Deleted 2 items');
    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(undo).toHaveBeenCalledOnce();
    await tick();
    expect(screen.queryByText('Deleted 2 items')).not.toBeInTheDocument();

    toasts.show({ message: 'Saved', timeoutMs: 1000 });
    await tick();
    vi.advanceTimersByTime(1100);
    await tick();
    expect(screen.queryByText('Saved')).not.toBeInTheDocument();
    vi.useRealTimers();
  });
});

describe('tooltip', () => {
  it('appears after a delay on hover and focus', async () => {
    vi.useFakeTimers();
    render(Harness, {});
    const target = screen.getByTestId('tip-target');
    await fireEvent.mouseEnter(target);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    vi.advanceTimersByTime(600);
    await tick();
    expect(screen.getByRole('tooltip')).toHaveTextContent('Helpful hint');
    await fireEvent.mouseLeave(target);
    await tick();
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('stays hidden after a tap until the keyboard or a mouse is used', async () => {
    vi.useFakeTimers();
    render(Harness, {});
    const target = screen.getByTestId('tip-target');
    await fireEvent.pointerDown(target, { pointerType: 'touch' });
    await fireEvent.mouseEnter(target);
    await fireEvent.focusIn(target);
    vi.advanceTimersByTime(600);
    await tick();
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();

    await fireEvent.keyDown(window, { key: 'Tab' });
    await fireEvent.focusIn(target);
    vi.advanceTimersByTime(600);
    await tick();
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    vi.useRealTimers();
  });
});

describe('Select', () => {
  it('opens, navigates and selects', async () => {
    const onchange = vi.fn();
    render(Harness, { onchange });
    const trigger = screen.getByRole('combobox', { name: 'Theme' });
    expect(trigger).toHaveTextContent('Light');
    await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    await tick();
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    await fireEvent.keyDown(screen.getByRole('listbox'), { key: 'ArrowDown' });
    await fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Enter' });
    expect(onchange).toHaveBeenCalledWith('dark');
    expect(trigger).toHaveTextContent('Dark');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
});

describe('Combobox', () => {
  it('filters options and accepts custom values', async () => {
    render(Harness, {});
    const input = screen.getByRole('combobox', { name: 'Method' });
    await fireEvent.focus(input);
    await fireEvent.input(input, { target: { value: 'pa' } });
    await tick();
    expect(screen.getAllByRole('option').map((o) => o.textContent?.trim())).toEqual(['PATCH']);
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByTestId('combo-value')).toHaveTextContent('PATCH');

    await fireEvent.input(input, { target: { value: 'PURGE' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByTestId('combo-value')).toHaveTextContent('PURGE');
  });
});
