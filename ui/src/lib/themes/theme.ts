import { COLOR_TOKENS, cssVar, type ColorToken } from './tokens';

export type ThemeKind = 'light' | 'dark';

export interface Theme {
  id: string;
  name: string;
  kind: ThemeKind;
  /** Id of a theme to inherit missing colors from (custom themes). */
  extends?: string;
  colors: Partial<Record<ColorToken, string>>;
}

export interface ResolvedTheme extends Theme {
  colors: Record<ColorToken, string>;
}

export type ParseResult = { ok: true; theme: Theme } | { ok: false; errors: string[] };

const TOKEN_SET = new Set<string>(COLOR_TOKENS);

// Deliberately strict: theme files are user-supplied and end up inside a style
// attribute, so only plain color syntaxes are allowed.
const COLOR_RE =
  /^(#[0-9a-f]{3,4}|#[0-9a-f]{6}|#[0-9a-f]{8}|(rgb|rgba|hsl|hsla|oklch|oklab|lab|lch)\([0-9a-z.%,/\s+-]+\)|transparent)$/i;

export function isColor(value: unknown): value is string {
  return typeof value === 'string' && COLOR_RE.test(value.trim());
}

/** Validates untrusted theme JSON (imported files, user edits). */
export function parseTheme(input: unknown): ParseResult {
  const errors: string[] = [];
  if (!input || typeof input !== 'object')
    return { ok: false, errors: ['theme must be an object'] };
  const t = input as Record<string, unknown>;

  for (const key of ['id', 'name'] as const) {
    if (typeof t[key] !== 'string' || !(t[key] as string).trim())
      errors.push(`${key} must be a non-empty string`);
  }
  if (t.kind !== 'light' && t.kind !== 'dark') errors.push('kind must be "light" or "dark"');
  if (t.extends !== undefined && typeof t.extends !== 'string')
    errors.push('extends must be a string');

  const colors = t.colors;
  if (!colors || typeof colors !== 'object') {
    errors.push('colors must be an object');
  } else {
    for (const [token, value] of Object.entries(colors)) {
      if (!TOKEN_SET.has(token)) errors.push(`unknown token "${token}"`);
      else if (!isColor(value)) errors.push(`invalid color for "${token}"`);
    }
    if (t.extends === undefined) {
      for (const token of COLOR_TOKENS) {
        if (!(token in colors)) errors.push(`missing token "${token}"`);
      }
    }
  }

  return errors.length ? { ok: false, errors } : { ok: true, theme: input as Theme };
}

/** Fills inherited colors; `available` must contain the base themes. */
export function resolveTheme(
  theme: Theme,
  available: readonly Theme[],
  seen = new Set<string>(),
): ResolvedTheme {
  if (!theme.extends) return theme as ResolvedTheme;
  if (seen.has(theme.id)) throw new Error(`theme inheritance cycle at "${theme.id}"`);
  seen.add(theme.id);
  const base = available.find((t) => t.id === theme.extends);
  if (!base) throw new Error(`base theme "${theme.extends}" not found`);
  const resolvedBase = resolveTheme(base, available, seen);
  return { ...theme, colors: { ...resolvedBase.colors, ...theme.colors } };
}

export function applyTheme(theme: ResolvedTheme, el: HTMLElement = document.documentElement) {
  for (const token of COLOR_TOKENS) {
    el.style.setProperty(cssVar(token), theme.colors[token]);
  }
  el.dataset.themeKind = theme.kind;
  el.dataset.theme = theme.id;
  el.style.colorScheme = theme.kind;
}

/** JSON Schema for theme files, published at docs/format/theme.schema.json. */
export function themeJsonSchema() {
  const colorProps = Object.fromEntries(
    COLOR_TOKENS.map((t) => [t, { type: 'string', description: `CSS color for --c-${t}` }]),
  );
  return {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    $id: 'https://github.com/hightemp/reqly/docs/format/theme.schema.json',
    title: 'Reqly theme',
    type: 'object',
    required: ['id', 'name', 'kind', 'colors'],
    additionalProperties: false,
    properties: {
      id: { type: 'string', minLength: 1 },
      name: { type: 'string', minLength: 1 },
      kind: { enum: ['light', 'dark'] },
      extends: { type: 'string', description: 'Id of the theme to inherit missing colors from' },
      colors: {
        type: 'object',
        additionalProperties: false,
        properties: colorProps,
        description: 'Without "extends" every token is required.',
      },
    },
  };
}
