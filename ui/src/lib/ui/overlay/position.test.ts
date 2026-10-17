import { describe, expect, it } from 'vitest';
import { computePosition } from './position';

const viewport = { width: 1000, height: 800 };
const anchor = { x: 100, y: 100, width: 80, height: 20 };
const size = { width: 200, height: 150 };

describe('computePosition', () => {
  it('places below-start with offset', () => {
    expect(computePosition(anchor, size, viewport, 'bottom-start', 4)).toEqual({
      x: 100,
      y: 124,
      placement: 'bottom-start',
    });
  });

  it('aligns end and centers', () => {
    // End-aligned would start at -20, so it is shifted to the padding.
    expect(computePosition(anchor, size, viewport, 'bottom-end', 0).x).toBe(8);
    // No room above (y would be -50): flips below; centered horizontally.
    expect(computePosition(anchor, size, viewport, 'top', 0)).toEqual({
      x: 40,
      y: 120,
      placement: 'bottom',
    });
  });

  it('flips to the top when there is no room below', () => {
    const low = { x: 100, y: 700, width: 80, height: 20 };
    expect(computePosition(low, size, viewport, 'bottom-start', 4)).toEqual({
      x: 100,
      y: 546,
      placement: 'top-start',
    });
  });

  it('flips right to left', () => {
    const edge = { x: 900, y: 100, width: 50, height: 20 };
    const p = computePosition(edge, size, viewport, 'right-start', 4);
    expect(p.placement).toBe('left-start');
    expect(p.x).toBe(900 - 4 - 200);
  });

  it('shifts inside the viewport with padding', () => {
    const right = { x: 950, y: 100, width: 30, height: 20 };
    expect(computePosition(right, size, viewport, 'bottom-start', 4).x).toBe(1000 - 200 - 8);
  });

  it('clamps when the element is larger than the viewport', () => {
    const p = computePosition(anchor, { width: 2000, height: 2000 }, viewport, 'bottom-start', 4);
    expect(p.x).toBe(8);
    expect(p.y).toBe(8);
  });

  it('treats a point anchor (context menu)', () => {
    const point = { x: 500, y: 400, width: 0, height: 0 };
    expect(computePosition(point, size, viewport, 'bottom-start', 0)).toEqual({
      x: 500,
      y: 400,
      placement: 'bottom-start',
    });
  });
});
