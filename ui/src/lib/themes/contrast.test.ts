import { describe, expect, it } from 'vitest';
import { builtinThemes } from './builtin';
import { contrast } from './palette';

const hex = (c: string) => (c.startsWith('#') ? c : null);

describe.each(builtinThemes.map((t) => [t.id, t] as const))('theme %s contrast', (_, theme) => {
  const c = theme.colors;
  const pair = (fg: keyof typeof c, bg: keyof typeof c, min: number) => {
    const a = hex(c[fg]);
    const b = hex(c[bg]);
    if (!a || !b) return;
    expect(contrast(a, b), `${fg} on ${bg}`).toBeGreaterThanOrEqual(min);
  };

  it('body text meets WCAG AA', () => {
    pair('fg', 'bg', 4.5);
    pair('fg', 'bg-elevated', 4.5);
    pair('fg', 'bg-sunken', 4.5);
  });

  it('secondary text and accents stay readable', () => {
    pair('fg-muted', 'bg-elevated', 4.5);
    pair('fg-subtle', 'bg-elevated', 3);
    pair('fg-on-accent', 'accent', 3);
    pair('fg', 'bg-selected', 4.5);
  });

  it('method colors are readable on panels', () => {
    for (const m of [
      'method-get',
      'method-post',
      'method-put',
      'method-patch',
      'method-delete',
    ] as const)
      pair(m, 'bg-elevated', 3);
  });
});
