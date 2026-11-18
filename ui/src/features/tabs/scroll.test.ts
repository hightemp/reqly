import { describe, expect, it } from 'vitest';
import { overflow, wheelDelta } from './scroll';

describe('wheelDelta', () => {
  it('turns a vertical wheel into horizontal scrolling', () => {
    expect(wheelDelta({ deltaX: 0, deltaY: 100, deltaMode: 0 })).toBe(100);
    expect(wheelDelta({ deltaX: 0, deltaY: -40, deltaMode: 0 })).toBe(-40);
  });

  it('keeps horizontal (touchpad) scrolling', () => {
    expect(wheelDelta({ deltaX: 30, deltaY: 5, deltaMode: 0 })).toBe(30);
  });

  it('converts lines and pages to pixels', () => {
    expect(wheelDelta({ deltaX: 0, deltaY: 3, deltaMode: 1 })).toBe(48);
    expect(wheelDelta({ deltaX: 0, deltaY: 1, deltaMode: 2 }, 500)).toBe(500);
  });
});

describe('overflow', () => {
  const el = (scrollLeft: number, clientWidth: number, scrollWidth: number) => ({
    scrollLeft,
    clientWidth,
    scrollWidth,
  });

  it('nothing to scroll when the tabs fit', () => {
    expect(overflow(el(0, 500, 500))).toEqual({ start: false, end: false });
  });

  it('reports which sides have hidden tabs', () => {
    expect(overflow(el(0, 500, 1200))).toEqual({ start: false, end: true });
    expect(overflow(el(300, 500, 1200))).toEqual({ start: true, end: true });
    expect(overflow(el(700, 500, 1200))).toEqual({ start: true, end: false });
  });

  it('ignores sub-pixel rounding at the end', () => {
    expect(overflow(el(699.5, 500, 1200))).toEqual({ start: true, end: false });
  });
});
