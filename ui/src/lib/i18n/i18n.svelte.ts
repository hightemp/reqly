// Small typed i18n: nested message dictionaries, `{param}` interpolation and
// plural objects selected with Intl.PluralRules.

export type Plural = {
  zero?: string;
  one?: string;
  two?: string;
  few?: string;
  many?: string;
  other: string;
};
export type MessageValue = string | Plural;
export interface MessageTree {
  [key: string]: MessageValue | MessageTree;
}

/** Same shape as `T`, with every string leaf widened (for translations). */
export type Translation<T> = {
  [K in keyof T]: T[K] extends string
    ? string
    : T[K] extends { other: string }
      ? Plural
      : Translation<T[K]>;
};

/** Dotted paths to every leaf (string or plural) of `T`. */
export type MessageKey<T, P extends string = ''> = {
  [K in keyof T & string]: T[K] extends string | { other: string }
    ? `${P}${K}`
    : MessageKey<T[K], `${P}${K}.`>;
}[keyof T & string];

export type Params = Record<string, string | number>;

const isPlural = (v: unknown): v is Plural =>
  typeof v === 'object' && v !== null && typeof (v as Plural).other === 'string';

// Keys may themselves contain dots (e.g. `errors['command.unknown']`), so the
// longest matching segment wins at each level.
function lookup(
  node: MessageValue | MessageTree | undefined,
  key: string,
): MessageValue | undefined {
  if (key === '') return typeof node === 'string' || isPlural(node) ? node : undefined;
  if (!node || typeof node !== 'object' || isPlural(node)) return undefined;
  const parts = key.split('.');
  for (let i = parts.length; i > 0; i--) {
    const head = parts.slice(0, i).join('.');
    if (head in node) {
      const found = lookup((node as MessageTree)[head], parts.slice(i).join('.'));
      if (found !== undefined) return found;
    }
  }
  return undefined;
}

function interpolate(template: string, params: Params | undefined): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

export function detectLocale<L extends string>(
  languages: readonly string[],
  supported: readonly L[],
  fallback: L,
): L {
  for (const lang of languages) {
    const base = lang.toLowerCase().split('-')[0];
    const hit = supported.find((l) => l === base);
    if (hit) return hit;
  }
  return fallback;
}

export class I18n<L extends string, T extends MessageTree> {
  locale = $state<L>() as L;
  // Caches, not reactive state.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  private readonly warned = new Set<string>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  private pluralRules = new Map<string, Intl.PluralRules>();

  constructor(
    private readonly dictionaries: Record<L, Translation<T>>,
    private readonly options: { fallback: L; locale: L },
  ) {
    this.locale = options.locale;
  }

  get locales(): L[] {
    return Object.keys(this.dictionaries) as L[];
  }

  setLocale(locale: L) {
    this.locale = locale;
  }

  t = (key: MessageKey<T>, params?: Params): string => {
    const k = key as string;
    let value = lookup(this.dictionaries[this.locale] as MessageTree, k);
    let locale: L = this.locale;
    if (value === undefined) {
      this.warnOnce(`${this.locale}:${k}`, `[i18n] missing "${k}" in "${this.locale}"`);
      value = lookup(this.dictionaries[this.options.fallback] as MessageTree, k);
      locale = this.options.fallback;
    }
    if (value === undefined) return k;
    if (isPlural(value)) {
      const count = Number(params?.count ?? 0);
      const category = this.rules(locale).select(count) as keyof Plural;
      value = value[category] ?? value.other;
    }
    return interpolate(value, params);
  };

  formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
    return new Intl.NumberFormat(this.locale, options).format(value);
  }

  formatBytes(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${this.formatNumber(value, { maximumFractionDigits: unit === 0 ? 0 : 2 })} ${units[unit]}`;
  }

  formatDuration(ms: number): string {
    if (ms < 1000) return `${this.formatNumber(Math.round(ms))} ms`;
    return `${this.formatNumber(ms / 1000, { maximumFractionDigits: 2 })} s`;
  }

  formatDateTime(value: number | Date, options?: Intl.DateTimeFormatOptions): string {
    return new Intl.DateTimeFormat(
      this.locale,
      options ?? { dateStyle: 'medium', timeStyle: 'medium' },
    ).format(value);
  }

  private rules(locale: string): Intl.PluralRules {
    let rules = this.pluralRules.get(locale);
    if (!rules) {
      rules = new Intl.PluralRules(locale);
      this.pluralRules.set(locale, rules);
    }
    return rules;
  }

  private warnOnce(id: string, message: string) {
    if (this.warned.has(id)) return;
    this.warned.add(id);
    console.warn(message);
  }
}
