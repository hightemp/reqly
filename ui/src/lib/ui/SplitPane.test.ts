import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import SplitPaneHarness from './SplitPaneHarness.test.svelte';

describe('SplitPane', () => {
  it('renders both panes and an accessible separator', () => {
    render(SplitPaneHarness, { size: 200 });
    const sep = screen.getByRole('separator');
    expect(sep).toHaveAttribute('aria-orientation', 'vertical');
    expect(sep).toHaveAttribute('aria-valuenow', '200');
    expect(screen.getByTestId('first')).toBeInTheDocument();
    expect(screen.getByTestId('second')).toBeInTheDocument();
  });

  it('resizes with arrow keys within bounds', async () => {
    const onResize = vi.fn();
    render(SplitPaneHarness, { size: 200, min: 150, max: 230, onResize });
    const sep = screen.getByRole('separator');
    await fireEvent.keyDown(sep, { key: 'ArrowRight' });
    expect(sep).toHaveAttribute('aria-valuenow', '210');
    await fireEvent.keyDown(sep, { key: 'ArrowRight', shiftKey: true });
    expect(sep).toHaveAttribute('aria-valuenow', '230');
    await fireEvent.keyDown(sep, { key: 'Home' });
    expect(sep).toHaveAttribute('aria-valuenow', '150');
    expect(onResize).toHaveBeenLastCalledWith(150);
  });

  it('uses up/down keys when vertical', async () => {
    render(SplitPaneHarness, { size: 200, direction: 'vertical' });
    const sep = screen.getByRole('separator');
    expect(sep).toHaveAttribute('aria-orientation', 'horizontal');
    await fireEvent.keyDown(sep, { key: 'ArrowDown' });
    expect(sep).toHaveAttribute('aria-valuenow', '210');
  });

  it('resets to the default size on double click', async () => {
    render(SplitPaneHarness, { size: 300, defaultSize: 240 });
    const sep = screen.getByRole('separator');
    await fireEvent.dblClick(sep);
    expect(sep).toHaveAttribute('aria-valuenow', '240');
  });

  it('drags with the pointer', async () => {
    render(SplitPaneHarness, { size: 200 });
    const sep = screen.getByRole('separator');
    await fireEvent.pointerDown(sep, { clientX: 100, pointerId: 1, button: 0 });
    await fireEvent.pointerMove(sep, { clientX: 140, pointerId: 1 });
    await fireEvent.pointerUp(sep, { clientX: 140, pointerId: 1 });
    expect(sep).toHaveAttribute('aria-valuenow', '240');
  });

  it('hides the first pane when collapsed', () => {
    render(SplitPaneHarness, { size: 200, collapsed: true });
    expect(screen.queryByTestId('first')).not.toBeInTheDocument();
    expect(screen.queryByRole('separator')).not.toBeInTheDocument();
  });
});
