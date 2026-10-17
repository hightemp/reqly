import { describe, expect, it } from 'vitest';
import { applyTheme, parseTheme, resolveTheme, themeJsonSchema, type Theme } from './theme';
import { COLOR_TOKENS } from './tokens';
import { builtinThemes } from './builtin';

const dark = builtinThemes.find((t) => t.id === 'dark')!;

describe('builtin themes', () => {
  it('include light and dark', () => {
    expect(builtinThemes.map((t) => t.id)).toEqual(expect.arrayContaining(['light', 'dark']));
  });

  it.each(builtinThemes.map((t) => [t.id, t] as const))('%s defines every token', (_, theme) => {
    expect(Object.keys(theme.colors).sort()).toEqual([...COLOR_TOKENS].sort());
    expect(parseTheme(theme)).toEqual({ ok: true, theme });
  });
});

describe('parseTheme', () => {
  it('rejects missing tokens without extends', () => {
    const { colors: _, ...rest } = dark;
    const res = parseTheme({ ...rest, colors: { bg: '#000' } });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.errors.some((e) => e.includes('fg'))).toBe(true);
  });

  it('accepts partial colors when extending a theme', () => {
    const res = parseTheme({
      id: 'mine',
      name: 'Mine',
      kind: 'dark',
      extends: 'dark',
      colors: { accent: '#ff0080' },
    });
    expect(res.ok).toBe(true);
  });

  it('rejects invalid color values and unknown tokens', () => {
    const res = parseTheme({
      id: 'x',
      name: 'X',
      kind: 'dark',
      extends: 'dark',
      colors: { accent: 'blue;}', nope: '#fff' },
    });
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.errors).toEqual(
        expect.arrayContaining([
          expect.stringContaining('accent'),
          expect.stringContaining('nope'),
        ]),
      );
    }
  });

  it('accepts common css color syntaxes', () => {
    for (const color of [
      '#abc',
      '#aabbcc',
      '#aabbccdd',
      'rgb(1 2 3)',
      'rgba(1,2,3,0.5)',
      'hsl(10 20% 30%)',
      'oklch(70% 0.1 200)',
      'transparent',
    ]) {
      const res = parseTheme({
        id: 'c',
        name: 'C',
        kind: 'light',
        extends: 'light',
        colors: { accent: color },
      });
      expect(res.ok, color).toBe(true);
    }
  });

  it('rejects non-objects and bad kind', () => {
    expect(parseTheme(null).ok).toBe(false);
    expect(parseTheme({ ...dark, kind: 'sepia' }).ok).toBe(false);
  });
});

describe('resolveTheme', () => {
  it('fills missing colors from the base theme', () => {
    const custom: Theme = {
      id: 'mine',
      name: 'Mine',
      kind: 'dark',
      extends: 'dark',
      colors: { accent: '#ff0080' },
    };
    const resolved = resolveTheme(custom, builtinThemes);
    expect(resolved.colors.accent).toBe('#ff0080');
    expect(resolved.colors.bg).toBe(dark.colors.bg);
  });

  it('throws on unknown base', () => {
    expect(() =>
      resolveTheme(
        { id: 'm', name: 'M', kind: 'dark', extends: 'nope', colors: {} },
        builtinThemes,
      ),
    ).toThrow(/nope/);
  });
});

describe('applyTheme', () => {
  it('writes css variables and color-scheme', () => {
    const el = document.createElement('div');
    applyTheme(dark, el);
    expect(el.style.getPropertyValue('--c-bg')).toBe(dark.colors.bg);
    expect(el.style.getPropertyValue('--c-method-get')).toBe(dark.colors['method-get']);
    expect(el.dataset.themeKind).toBe('dark');
    expect(el.style.colorScheme).toBe('dark');
  });
});

describe('themeJsonSchema', () => {
  it('lists every token and matches the published schema', async () => {
    const schema = themeJsonSchema();
    expect(Object.keys(schema.properties.colors.properties).sort()).toEqual(
      [...COLOR_TOKENS].sort(),
    );
    await expect(JSON.stringify(schema, null, 2) + '\n').toMatchFileSnapshot(
      '../../../../docs/format/theme.schema.json',
    );
  });
});
