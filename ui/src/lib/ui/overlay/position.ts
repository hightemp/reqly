// Floating element positioning with flip and shift (no dependencies).

export type Side = 'top' | 'bottom' | 'left' | 'right';
export type Align = 'start' | 'center' | 'end';
export type Placement = Side | `${Side}-start` | `${Side}-end`;

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Position {
  x: number;
  y: number;
  placement: Placement;
}

const PADDING = 8;
const OPPOSITE: Record<Side, Side> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };

function parse(placement: Placement): [Side, Align] {
  const [side, align] = placement.split('-') as [Side, Align | undefined];
  return [side, align ?? 'center'];
}

function join(side: Side, align: Align): Placement {
  return (align === 'center' ? side : `${side}-${align}`) as Placement;
}

function place(a: Rect, s: Size, side: Side, align: Align, offset: number) {
  const vertical = side === 'top' || side === 'bottom';
  let x: number;
  let y: number;
  if (vertical) {
    y = side === 'bottom' ? a.y + a.height + offset : a.y - offset - s.height;
    x =
      align === 'start'
        ? a.x
        : align === 'end'
          ? a.x + a.width - s.width
          : a.x + a.width / 2 - s.width / 2;
  } else {
    x = side === 'right' ? a.x + a.width + offset : a.x - offset - s.width;
    y =
      align === 'start'
        ? a.y
        : align === 'end'
          ? a.y + a.height - s.height
          : a.y + a.height / 2 - s.height / 2;
  }
  return { x, y };
}

function fits(p: { x: number; y: number }, s: Size, side: Side, viewport: Size): boolean {
  if (side === 'bottom') return p.y + s.height <= viewport.height - PADDING;
  if (side === 'top') return p.y >= PADDING;
  if (side === 'right') return p.x + s.width <= viewport.width - PADDING;
  return p.x >= PADDING;
}

const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, max));

/** Position for a floating element of `size` next to `anchor`, kept inside `viewport`. */
export function computePosition(
  anchor: Rect,
  size: Size,
  viewport: Size,
  placement: Placement = 'bottom-start',
  offset = 4,
): Position {
  const [initialSide, align] = parse(placement);
  let side = initialSide;
  let p = place(anchor, size, side, align, offset);
  if (!fits(p, size, side, viewport)) {
    const flipped = place(anchor, size, OPPOSITE[side], align, offset);
    if (fits(flipped, size, OPPOSITE[side], viewport)) {
      side = OPPOSITE[side];
      p = flipped;
    }
  }
  return {
    x: Math.round(clamp(p.x, PADDING, Math.max(PADDING, viewport.width - size.width - PADDING))),
    y: Math.round(clamp(p.y, PADDING, Math.max(PADDING, viewport.height - size.height - PADDING))),
    placement: join(side, align),
  };
}
