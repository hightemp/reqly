// Scrolling the tab strip: wheel to horizontal and the overflow arrows.

interface Wheel {
  deltaX: number;
  deltaY: number;
  deltaMode: number;
}

const LINE = 16;

/** Horizontal pixels for a wheel event: a mouse wheel scrolls the strip sideways. */
export function wheelDelta(e: Wheel, page = 400): number {
  const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
  return e.deltaMode === 1 ? d * LINE : e.deltaMode === 2 ? d * page : d;
}

interface Scrollable {
  scrollLeft: number;
  clientWidth: number;
  scrollWidth: number;
}

/** Whether tabs are hidden before (`start`) or after (`end`) the visible part. */
export function overflow(el: Scrollable): { start: boolean; end: boolean } {
  return {
    start: el.scrollLeft > 1,
    end: el.scrollLeft + el.clientWidth < el.scrollWidth - 1,
  };
}
