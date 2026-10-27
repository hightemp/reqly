// Touch gestures: long press as a right click, horizontal swipes.

export const LONG_PRESS_MS = 500;
/** Tree and list rows on touch screens (finger-sized). */
export const TOUCH_ROW_HEIGHT = 40;
/** A finger moving further than this is scrolling, not holding. */
export const MOVE_TOLERANCE = 10;

/**
 * Turns holding a finger still into a `contextmenu` event at that point, so
 * every context menu works on touch screens. Returns a function that removes
 * the listeners.
 */
export function installLongPress(target: Window | HTMLElement, delay = LONG_PRESS_MS) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let start: { x: number; y: number; el: EventTarget } | null = null;
  // The browser opened a menu itself during this touch.
  let opened = false;
  let dispatching = false;
  let swallowClick = false;

  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
    start = null;
  };

  const down = (e: Event) => {
    const p = e as PointerEvent;
    if (p.pointerType !== 'touch' || !p.isPrimary || !p.target) return;
    cancel();
    opened = false;
    start = { x: p.clientX, y: p.clientY, el: p.target };
    timer = setTimeout(() => {
      const s = start;
      cancel();
      if (!s || opened) return;
      dispatching = true;
      s.el.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: s.x,
          clientY: s.y,
        }),
      );
      dispatching = false;
      opened = true;
      swallowClick = true;
    }, delay);
  };

  const move = (e: Event) => {
    const p = e as PointerEvent;
    if (start && Math.hypot(p.clientX - start.x, p.clientY - start.y) > MOVE_TOLERANCE) cancel();
  };

  const up = () => {
    cancel();
    // The click of this touch comes right after pointerup.
    if (swallowClick) setTimeout(() => (swallowClick = false), 400);
  };

  const menu = (e: Event) => {
    if (dispatching) return;
    if (start) {
      // The browser has its own long press: let it win.
      opened = true;
      cancel();
    } else if (opened && swallowClick) {
      // Second menu for the same touch.
      e.preventDefault();
      e.stopPropagation();
    }
  };

  const click = (e: Event) => {
    if (!swallowClick) return;
    swallowClick = false;
    e.preventDefault();
    e.stopPropagation();
  };

  const listeners: [string, (e: Event) => void][] = [
    ['pointerdown', down],
    ['pointermove', move],
    ['pointerup', up],
    ['pointercancel', cancel],
    ['contextmenu', menu],
    ['click', click],
  ];
  listeners.forEach(([type, fn]) => target.addEventListener(type, fn, true));
  return () => {
    cancel();
    listeners.forEach(([type, fn]) => target.removeEventListener(type, fn, true));
  };
}

export interface TouchPoint {
  x: number;
  y: number;
  /** Milliseconds. */
  t: number;
}

const SWIPE_MIN = 60;
const SWIPE_MAX_MS = 600;

/** Direction of a quick, mostly horizontal swipe from `a` to `b`. */
export function swipeDirection(a: TouchPoint, b: TouchPoint): 'left' | 'right' | null {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  if (b.t - a.t > SWIPE_MAX_MS || Math.abs(dx) < SWIPE_MIN || Math.abs(dy) > Math.abs(dx) / 2) {
    return null;
  }
  return dx > 0 ? 'right' : 'left';
}
