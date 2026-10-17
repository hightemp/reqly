// Builds full themes from a compact palette so every theme derives hover,
// selection, syntax and status colors the same way.

import type { ColorToken } from './tokens';
import type { ResolvedTheme, ThemeKind } from './theme';

export interface Palette {
  id: string;
  name: string;
  kind: ThemeKind;
  bg: string;
  elevated: string;
  sunken: string;
  fg: string;
  muted: string;
  subtle: string;
  border: string;
  accent: string;
  onAccent: string;
  red: string;
  orange: string;
  yellow: string;
  green: string;
  cyan: string;
  blue: string;
  purple: string;
  pink: string;
  comment?: string;
}

function parseHex(hex: string): [number, number, number] {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? [...h].map((c) => c + c).join('') : h;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as [number, number, number];
}

const toHex = (rgb: number[]) =>
  `#${rgb
    .map((v) =>
      Math.round(Math.max(0, Math.min(255, v)))
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;

/** `a` mixed with `b` by `amount` (0 = a, 1 = b). */
export function mix(a: string, b: string, amount: number): string {
  const x = parseHex(a);
  const y = parseHex(b);
  return toHex(x.map((v, i) => v + (y[i]! - v) * amount));
}

export function alpha(color: string, a: number): string {
  const [r, g, b] = parseHex(color);
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

/** WCAG relative luminance and contrast ratio. */
export function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, b] = parseHex(hex).map((v) => {
      const c = v / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    }) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

export function themeFromPalette(p: Palette): ResolvedTheme {
  const dark = p.kind === 'dark';
  const hover = mix(p.elevated, p.fg, dark ? 0.07 : 0.05);
  const colors: Record<ColorToken, string> = {
    bg: p.bg,
    'bg-elevated': p.elevated,
    'bg-sunken': p.sunken,
    'bg-hover': hover,
    'bg-active': mix(p.elevated, p.fg, dark ? 0.12 : 0.09),
    'bg-selected': mix(p.elevated, p.accent, dark ? 0.25 : 0.15),
    overlay: dark ? 'rgba(0, 0, 0, 0.55)' : 'rgba(16, 16, 24, 0.35)',
    border: p.border,
    'border-strong': mix(p.border, p.fg, 0.15),
    fg: p.fg,
    'fg-muted': p.muted,
    'fg-subtle': p.subtle,
    'fg-on-accent': p.onAccent,
    accent: p.accent,
    'accent-hover': mix(p.accent, dark ? '#ffffff' : '#000000', 0.12),
    'accent-subtle': mix(p.elevated, p.accent, dark ? 0.18 : 0.12),
    'focus-ring': mix(p.accent, dark ? '#ffffff' : '#000000', 0.15),
    danger: p.red,
    'danger-subtle': mix(p.elevated, p.red, 0.18),
    warning: p.orange,
    'warning-subtle': mix(p.elevated, p.orange, 0.18),
    success: p.green,
    'success-subtle': mix(p.elevated, p.green, 0.18),
    info: p.blue,
    'info-subtle': mix(p.elevated, p.blue, 0.18),
    shadow: dark ? 'rgba(0, 0, 0, 0.45)' : 'rgba(16, 16, 24, 0.12)',
    'scrollbar-thumb': alpha(p.fg, dark ? 0.16 : 0.18),
    'scrollbar-thumb-hover': alpha(p.fg, dark ? 0.3 : 0.32),
    'method-get': p.green,
    'method-post': p.orange,
    'method-put': p.blue,
    'method-patch': p.purple,
    'method-delete': p.red,
    'method-head': p.cyan,
    'method-options': p.cyan,
    'method-other': p.muted,
    'status-1xx': p.blue,
    'status-2xx': p.green,
    'status-3xx': p.purple,
    'status-4xx': p.orange,
    'status-5xx': p.red,
    'status-error': p.red,
    'syntax-keyword': p.purple,
    'syntax-string': p.green,
    'syntax-number': p.orange,
    'syntax-boolean': p.blue,
    'syntax-null': p.subtle,
    'syntax-property': p.pink,
    'syntax-comment': p.comment ?? p.subtle,
    'syntax-punctuation': p.muted,
    'syntax-variable': p.accent,
    'syntax-variable-missing': p.red,
    'syntax-tag': p.blue,
    'syntax-attribute': p.yellow,
    'syntax-function': p.cyan,
    'syntax-error': p.red,
    'syntax-selection': alpha(p.accent, dark ? 0.28 : 0.18),
  };
  return { id: p.id, name: p.name, kind: p.kind, colors };
}
