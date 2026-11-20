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

export type Hidden = { start: boolean; end: boolean };

/**
 * Makes a horizontal strip of tabs scrollable with a mouse wheel and reports
 * whether tabs are hidden at either end (for the scroll arrows) whenever the
 * strip scrolls, resizes or gets other tabs. Returns the cleanup.
 */
export function scrollStrip(el: HTMLElement, onchange: (hidden: Hidden) => void): () => void {
  let last: Hidden | null = null;
  const measure = () => {
    const next = overflow(el);
    if (next.start === last?.start && next.end === last?.end) return;
    last = next;
    onchange(next);
  };
  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey) return;
    const { start, end } = overflow(el);
    const delta = wheelDelta(e, el.clientWidth);
    if ((delta < 0 && !start) || (delta > 0 && !end) || delta === 0) return;
    e.preventDefault();
    el.scrollLeft += delta;
  };
  el.addEventListener('wheel', onWheel, { passive: false });
  el.addEventListener('scroll', measure, { passive: true });
  const resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
  const observeAll = () => {
    if (!resize) return;
    resize.disconnect();
    resize.observe(el);
    for (const child of el.children) resize.observe(child);
  };
  const mutations = new MutationObserver(() => {
    observeAll();
    measure();
  });
  mutations.observe(el, { childList: true });
  observeAll();
  measure();
  return () => {
    el.removeEventListener('wheel', onWheel);
    el.removeEventListener('scroll', measure);
    resize?.disconnect();
    mutations.disconnect();
  };
}

/** Scrolls a strip by most of its width. */
export function scrollPage(el: HTMLElement, direction: -1 | 1) {
  el.scrollBy?.({ left: direction * el.clientWidth * 0.8, behavior: 'smooth' });
}
