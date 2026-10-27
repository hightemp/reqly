import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import { LONG_PRESS_MS } from '../touch';
import Harness from './TreeHarness.test.svelte';

const row = (name: string) => screen.getByRole('treeitem', { name: new RegExp(`^${name}`) });

function setup(extra: Record<string, unknown> = {}) {
  const spies = {
    onopen: vi.fn(),
    onactivate: vi.fn(),
    onmove: vi.fn(),
    onrename: vi.fn(),
    oncontextmenu: vi.fn(),
    onkey: vi.fn(),
  };
  const result = render(Harness, { ...spies, ...extra });
  return { ...spies, ...result };
}

describe('Tree', () => {
  it('renders roots and expands folders', async () => {
    setup();
    expect(screen.getAllByRole('treeitem').map((r) => r.textContent?.trim())).toEqual([
      'api',
      'readme',
    ]);
    await fireEvent.click(row('api').querySelector('.chevron')!);
    expect(screen.getAllByRole('treeitem').map((r) => r.textContent?.trim())).toEqual([
      'api',
      'users',
      'health',
      'readme',
    ]);
    expect(row('api')).toHaveAttribute('aria-expanded', 'true');
  });

  it('navigates with arrows, expands with right and goes to parent with left', async () => {
    const { onopen } = setup();
    await fireEvent.click(row('api'));
    const tree = screen.getByRole('tree');
    await fireEvent.keyDown(tree, { key: 'ArrowRight' });
    await tick();
    expect(row('users')).toBeInTheDocument();
    // Second ArrowRight moves into the expanded folder (users), then down to health.
    await fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(row('users')).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(row('health')).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(tree, { key: 'ArrowLeft' });
    expect(row('api')).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(tree, { key: 'End' });
    await fireEvent.keyDown(tree, { key: 'Enter' });
    expect(onopen).toHaveBeenCalledWith('readme');
  });

  it('supports ctrl and shift selection', async () => {
    setup({ expanded: ['api'] });
    await fireEvent.click(row('api'));
    await fireEvent.click(row('health'), { shiftKey: true });
    expect(
      screen.getAllByRole('treeitem', { selected: true }).map((r) => r.textContent?.trim()),
    ).toEqual(['api', 'users', 'health']);
    await fireEvent.click(row('users'), { ctrlKey: true });
    expect(
      screen.getAllByRole('treeitem', { selected: true }).map((r) => r.textContent?.trim()),
    ).toEqual(['api', 'health']);
    await fireEvent.keyDown(screen.getByRole('tree'), { key: 'a', ctrlKey: true });
    expect(screen.getAllByRole('treeitem', { selected: true })).toHaveLength(4);
  });

  it('typeahead focuses matching rows', async () => {
    setup();
    await fireEvent.click(row('api'));
    await fireEvent.keyDown(screen.getByRole('tree'), { key: 'r' });
    expect(row('readme')).toHaveAttribute('aria-selected', 'true');
  });

  it('context menu selects the row and reports the selection', async () => {
    const { oncontextmenu } = setup();
    await fireEvent.contextMenu(row('readme'));
    expect(oncontextmenu).toHaveBeenCalledWith(expect.any(MouseEvent), ['readme']);
    expect(row('readme')).toHaveAttribute('aria-selected', 'true');
  });

  it('renames inline: Enter commits, Escape cancels', async () => {
    const { onrename } = setup({ renaming: 'readme' });
    const input = await screen.findByRole('textbox');
    await waitFor(() => expect(document.activeElement).toBe(input));
    await fireEvent.input(input, { target: { value: 'README.md' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onrename).toHaveBeenCalledWith('readme', 'README.md');
  });

  it('Escape clears the selection, then is forwarded', async () => {
    const { onkey } = setup();
    await fireEvent.click(row('readme'));
    await fireEvent.keyDown(screen.getByRole('tree'), { key: 'Escape' });
    expect(screen.queryAllByRole('treeitem', { selected: true })).toHaveLength(0);
    expect(onkey).not.toHaveBeenCalled();
    await fireEvent.keyDown(screen.getByRole('tree'), { key: 'Escape' });
    expect(onkey).toHaveBeenCalledWith(expect.objectContaining({ key: 'Escape' }));
  });

  it('forwards unhandled keys', async () => {
    const { onkey } = setup();
    await fireEvent.click(row('readme'));
    await fireEvent.keyDown(screen.getByRole('tree'), { key: 'F2' });
    expect(onkey).toHaveBeenCalledWith(expect.objectContaining({ key: 'F2' }));
  });

  it('drags a row into a folder', async () => {
    const { onmove } = setup({ expanded: ['api'] });
    const tree = screen.getByRole('tree');
    const viewport = tree.querySelector<HTMLElement>('.viewport')!;
    viewport.getBoundingClientRect = () => ({
      top: 0,
      bottom: 400,
      left: 0,
      right: 300,
      width: 300,
      height: 400,
      x: 0,
      y: 0,
      toJSON() {},
    });
    document.elementFromPoint = () => row('users');

    // Rows: api(0) users(1) health(2) readme(3); 26px each. Drag readme into users (middle of row 1).
    await fireEvent.pointerDown(row('readme'), { button: 0, clientX: 10, clientY: 90 });
    await fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    await fireEvent.pointerMove(window, { clientX: 10, clientY: 39 });
    await tick();
    expect(row('users')).toHaveClass('drop-inside');
    await fireEvent.pointerUp(window);
    expect(onmove).toHaveBeenCalledWith(
      ['readme'],
      expect.objectContaining({ id: 'users', position: 'inside' }),
    );
  });

  it('refuses dropping a folder into its own child', async () => {
    const { onmove } = setup({ expanded: ['api'] });
    const viewport = screen.getByRole('tree').querySelector<HTMLElement>('.viewport')!;
    viewport.getBoundingClientRect = () => ({
      top: 0,
      bottom: 400,
      left: 0,
      right: 300,
      width: 300,
      height: 400,
      x: 0,
      y: 0,
      toJSON() {},
    });
    document.elementFromPoint = () => row('users');
    await fireEvent.pointerDown(row('api'), { button: 0, clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(window, { clientX: 10, clientY: 39 });
    await fireEvent.pointerUp(window);
    expect(onmove).not.toHaveBeenCalled();
  });

  describe('on touch screens', () => {
    function prepare() {
      const spies = setup({ expanded: ['api'] });
      const viewport = screen.getByRole('tree').querySelector<HTMLElement>('.viewport')!;
      viewport.getBoundingClientRect = () => ({
        top: 0,
        bottom: 400,
        left: 0,
        right: 300,
        width: 300,
        height: 400,
        x: 0,
        y: 0,
        toJSON() {},
      });
      document.elementFromPoint = () => row('users');
      return spies;
    }
    const touch = { pointerType: 'touch', isPrimary: true };

    it('scrolls instead of dragging when the finger moves at once', async () => {
      const { onmove } = prepare();
      await fireEvent.pointerDown(row('readme'), { ...touch, button: 0, clientX: 10, clientY: 90 });
      await fireEvent.pointerMove(window, { ...touch, clientX: 10, clientY: 39 });
      await fireEvent.pointerUp(window, touch);
      expect(onmove).not.toHaveBeenCalled();
    });

    it('drags after holding the row', async () => {
      vi.useFakeTimers();
      try {
        const { onmove } = prepare();
        await fireEvent.pointerDown(row('readme'), {
          ...touch,
          button: 0,
          clientX: 10,
          clientY: 90,
        });
        vi.advanceTimersByTime(LONG_PRESS_MS);
        const move = new Event('touchmove', { cancelable: true, bubbles: true });
        window.dispatchEvent(move);
        expect(move.defaultPrevented).toBe(true);
        await fireEvent.pointerMove(window, { ...touch, clientX: 10, clientY: 39 });
        await fireEvent.pointerUp(window, touch);
        expect(onmove).toHaveBeenCalledWith(
          ['readme'],
          expect.objectContaining({ id: 'users', position: 'inside' }),
        );
      } finally {
        vi.useRealTimers();
      }
    });
  });
});
