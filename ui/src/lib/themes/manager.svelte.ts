import { builtinThemes } from './builtin';
import { applyTheme, resolveTheme, type ResolvedTheme, type Theme } from './theme';

export type ThemePreference = 'system' | (string & {});

export interface MediaQuery {
  readonly matches: boolean;
  addEventListener(type: 'change', listener: () => void): void;
  removeEventListener(type: 'change', listener: () => void): void;
}

export interface ThemeManagerOptions {
  media?: MediaQuery;
  root?: HTMLElement;
  /** Themes used for "system" mode. */
  systemLight?: string;
  systemDark?: string;
}

/** Holds the theme preference, resolves it and applies it to the DOM. */
export class ThemeManager {
  preference = $state<ThemePreference>('system');
  custom = $state<Theme[]>([]);
  private osDark = $state(false);

  readonly themes = $derived<ResolvedTheme[]>([
    ...builtinThemes,
    ...this.custom.map((t) => resolveTheme(t, [...builtinThemes, ...this.custom])),
  ]);

  readonly active = $derived.by<ResolvedTheme>(() => {
    const systemId = this.osDark ? this.opts.systemDark : this.opts.systemLight;
    const wanted = this.preference === 'system' ? systemId : this.preference;
    return (
      this.themes.find((t) => t.id === wanted) ??
      this.themes.find((t) => t.id === systemId) ??
      builtinThemes[0]!
    );
  });

  private readonly opts: Required<Omit<ThemeManagerOptions, 'media'>> & { media?: MediaQuery };
  private readonly onMediaChange = () => {
    this.osDark = this.opts.media?.matches ?? false;
    this.apply();
  };

  constructor(options: ThemeManagerOptions = {}) {
    this.opts = {
      media: options.media ?? globalThis.matchMedia?.('(prefers-color-scheme: dark)'),
      root: options.root ?? document.documentElement,
      systemLight: options.systemLight ?? 'light',
      systemDark: options.systemDark ?? 'dark',
    };
    this.osDark = this.opts.media?.matches ?? false;
    this.opts.media?.addEventListener('change', this.onMediaChange);
    this.apply();
  }

  setPreference(preference: ThemePreference) {
    if (preference !== 'system' && !this.themes.some((t) => t.id === preference)) {
      console.warn(`[themes] unknown theme "${preference}", using system`);
    }
    this.preference = preference;
    this.apply();
  }

  addCustom(theme: Theme) {
    if (builtinThemes.some((t) => t.id === theme.id)) {
      throw new Error(`theme id "${theme.id}" is reserved`);
    }
    resolveTheme(theme, [...builtinThemes, ...this.custom]);
    this.custom = [...this.custom.filter((t) => t.id !== theme.id), theme];
    this.apply();
  }

  removeCustom(id: string) {
    this.custom = this.custom.filter((t) => t.id !== id);
    this.apply();
  }

  /** Writes the active theme to the DOM. Called on every change. */
  apply() {
    applyTheme(this.active, this.opts.root);
  }

  destroy() {
    this.opts.media?.removeEventListener('change', this.onMediaChange);
  }
}
