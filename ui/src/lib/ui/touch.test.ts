import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installLongPress, LONG_PRESS_MS, swipeDirection } from './touch';

function pointer(type: string, x: number, y: number, pointerType = 'touch') {
  return new PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: x,
    clientY: y,
    pointerType,
    isPrimary: true,
  });
}

describe('installLongPress', () => {
  let el: HTMLElement;
  let off: () => void;
  let menus: MouseEvent[];

  beforeEach(() => {
    vi.useFakeTimers();
    el = document.createElement('div');
    document.body.append(el);
    menus = [];
    el.addEventListener('contextmenu', (e) => menus.push(e));
    off = installLongPress(window);
  });

  afterEach(() => {
    off();
    el.remove();
    vi.useRealTimers();
  });

  it('opens the context menu where a finger is held', () => {
    el.dispatchEvent(pointer('pointerdown', 40, 50));
    vi.advanceTimersByTime(LONG_PRESS_MS - 1);
    expect(menus).toHaveLength(0);
    vi.advanceTimersByTime(1);
    expect(menus).toHaveLength(1);
    expect(menus[0]).toMatchObject({ clientX: 40, clientY: 50 });
  });

  it('ignores the mouse, moving fingers and short taps', () => {
    el.dispatchEvent(pointer('pointerdown', 0, 0, 'mouse'));
    vi.advanceTimersByTime(LONG_PRESS_MS);

    el.dispatchEvent(pointer('pointerdown', 0, 0));
    window.dispatchEvent(pointer('pointermove', 0, 30));
    vi.advanceTimersByTime(LONG_PRESS_MS);

    el.dispatchEvent(pointer('pointerdown', 0, 0));
    window.dispatchEvent(pointer('pointerup', 0, 0));
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(menus).toHaveLength(0);
  });

  it('does not open the menu twice when the browser opens it too', () => {
    el.dispatchEvent(pointer('pointerdown', 0, 0));
    el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(menus).toHaveLength(1);
  });

  it('swallows the click that ends the long press', () => {
    const clicks = vi.fn();
    el.addEventListener('click', clicks);
    el.dispatchEvent(pointer('pointerdown', 0, 0));
    vi.advanceTimersByTime(LONG_PRESS_MS);
    window.dispatchEvent(pointer('pointerup', 0, 0));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(clicks).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(clicks).toHaveBeenCalledOnce();
  });
});

describe('swipeDirection', () => {
  it('detects quick horizontal swipes', () => {
    expect(swipeDirection({ x: 10, y: 100, t: 0 }, { x: 120, y: 110, t: 200 })).toBe('right');
    expect(swipeDirection({ x: 200, y: 100, t: 0 }, { x: 60, y: 90, t: 250 })).toBe('left');
  });

  it('ignores short, slow and vertical moves', () => {
    expect(swipeDirection({ x: 10, y: 100, t: 0 }, { x: 40, y: 100, t: 100 })).toBeNull();
    expect(swipeDirection({ x: 10, y: 100, t: 0 }, { x: 200, y: 100, t: 2000 })).toBeNull();
    expect(swipeDirection({ x: 10, y: 100, t: 0 }, { x: 90, y: 260, t: 200 })).toBeNull();
  });
});
