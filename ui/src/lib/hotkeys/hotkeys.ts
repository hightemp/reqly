// Keyboard shortcuts: parsing, matching, display and an action registry.
//
// Combos are written as `Mod+Shift+K`. `Mod` is ⌘ on macOS and Ctrl elsewhere.
// Matching uses `KeyboardEvent.code` (physical key), so shortcuts keep working
// on non-latin layouts where `event.key` would be e.g. "ы" instead of "s".

export type Platform = 'mac' | 'windows' | 'linux';

export interface Combo {
  code: string;
  ctrl: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
}

const MODIFIERS = new Set(['mod', 'ctrl', 'control', 'cmd', 'meta', 'alt', 'option', 'shift']);

const NAMED_KEYS: Record<string, string> = {
  enter: 'Enter',
  tab: 'Tab',
  esc: 'Escape',
  escape: 'Escape',
  del: 'Delete',
  delete: 'Delete',
  backspace: 'Backspace',
  space: 'Space',
  up: 'ArrowUp',
  down: 'ArrowDown',
  left: 'ArrowLeft',
  right: 'ArrowRight',
  arrowup: 'ArrowUp',
  arrowdown: 'ArrowDown',
  arrowleft: 'ArrowLeft',
  arrowright: 'ArrowRight',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  insert: 'Insert',
};

const PUNCTUATION: Record<string, string> = {
  ',': 'Comma',
  '.': 'Period',
  '/': 'Slash',
  ';': 'Semicolon',
  "'": 'Quote',
  '[': 'BracketLeft',
  ']': 'BracketRight',
  '\\': 'Backslash',
  '-': 'Minus',
  '=': 'Equal',
  '`': 'Backquote',
};

const CODE_LABELS: Record<string, string> = Object.fromEntries([
  ...Object.entries(PUNCTUATION).map(([label, code]) => [code, label]),
  ['Escape', 'Esc'],
  ['Delete', 'Del'],
  ['ArrowUp', '↑'],
  ['ArrowDown', '↓'],
  ['ArrowLeft', '←'],
  ['ArrowRight', '→'],
]);

function keyToCode(key: string): string {
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(key)) return key.toUpperCase();
  const named = NAMED_KEYS[key.toLowerCase()];
  if (named) return named;
  const punct = PUNCTUATION[key];
  if (punct) return punct;
  throw new Error(`unknown key "${key}"`);
}

export function parseCombo(combo: string, platform: Platform): Combo {
  // Split on "+" but keep a literal trailing "+" key out of scope (use "Equal" instead).
  const parts = combo.split('+').map((p) => p.trim());
  if (parts.some((p) => p === '')) throw new Error(`malformed combo "${combo}"`);
  const result: Combo = { code: '', ctrl: false, meta: false, alt: false, shift: false };
  for (const part of parts) {
    const lower = part.toLowerCase();
    if (MODIFIERS.has(lower)) {
      if (lower === 'mod') result[platform === 'mac' ? 'meta' : 'ctrl'] = true;
      else if (lower === 'ctrl' || lower === 'control') result.ctrl = true;
      else if (lower === 'cmd' || lower === 'meta') result.meta = true;
      else if (lower === 'alt' || lower === 'option') result.alt = true;
      else result.shift = true;
    } else {
      if (result.code) throw new Error(`combo "${combo}" has more than one key`);
      result.code = keyToCode(part);
    }
  }
  if (!result.code) throw new Error(`combo "${combo}" has no key`);
  return result;
}

export function matches(event: KeyboardEvent, combo: Combo): boolean {
  return (
    event.code === combo.code &&
    event.ctrlKey === combo.ctrl &&
    event.metaKey === combo.meta &&
    event.altKey === combo.alt &&
    event.shiftKey === combo.shift
  );
}

function codeLabel(code: string): string {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  return CODE_LABELS[code] ?? code;
}

/** Human-readable label: `Ctrl+Shift+K` or `⇧⌘K` on macOS. */
export function formatCombo(combo: string, platform: Platform): string {
  const c = parseCombo(combo, platform);
  const key = codeLabel(c.code);
  if (platform === 'mac') {
    return `${c.ctrl ? '⌃' : ''}${c.alt ? '⌥' : ''}${c.shift ? '⇧' : ''}${c.meta ? '⌘' : ''}${key}`;
  }
  return [c.ctrl && 'Ctrl', c.meta && 'Win', c.alt && 'Alt', c.shift && 'Shift', key]
    .filter(Boolean)
    .join('+');
}

/** Canonical combo for a key press (used to record custom bindings). */
export function comboFromEvent(event: KeyboardEvent, platform: Platform): string | null {
  if (/^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/.test(event.code)) return null;
  let key: string;
  if (event.code.startsWith('Key')) key = event.code.slice(3);
  else if (event.code.startsWith('Digit')) key = event.code.slice(5);
  else key = Object.entries(PUNCTUATION).find(([, code]) => code === event.code)?.[0] ?? event.code;

  const modKey = platform === 'mac' ? event.metaKey : event.ctrlKey;
  const parts: string[] = [];
  if (modKey) parts.push('Mod');
  if (platform === 'mac' ? event.ctrlKey : event.metaKey)
    parts.push(platform === 'mac' ? 'Ctrl' : 'Meta');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  parts.push(key);
  return parts.join('+');
}

export function detectPlatform(): Platform {
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  if (/Mac|iPhone|iPad/.test(ua)) return 'mac';
  if (/Win/.test(ua)) return 'windows';
  return 'linux';
}

// ---------------------------------------------------------------------------

export interface ActionDefinition {
  id: string;
  combos: string[];
  /** `global` is always active; other scopes are active while focused. */
  scope: string;
}

export interface Conflict {
  combo: string;
  scope: string;
  actions: string[];
}

export class HotkeyRegistry {
  private readonly actions = new Map<string, ActionDefinition>();
  private readonly overrides = new Map<string, string[]>();
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(readonly platform: Platform) {}

  define(action: ActionDefinition) {
    action.combos.forEach((c) => parseCombo(c, this.platform));
    this.actions.set(action.id, action);
  }

  definitions(): ActionDefinition[] {
    return [...this.actions.values()];
  }

  combosFor(id: string): string[] {
    return this.overrides.get(id) ?? this.actions.get(id)?.combos ?? [];
  }

  setOverride(id: string, combos: string[]) {
    combos.forEach((c) => parseCombo(c, this.platform));
    this.overrides.set(id, combos);
  }

  resetOverride(id: string) {
    this.overrides.delete(id);
  }

  getOverrides(): Record<string, string[]> {
    return Object.fromEntries(this.overrides);
  }

  /** Returns the action for `event`; later scopes in `activeScopes` win. */
  resolve(event: KeyboardEvent, activeScopes: readonly string[]): string | null {
    for (const scope of [...activeScopes].reverse()) {
      for (const action of this.actions.values()) {
        if (action.scope !== scope) continue;
        if (this.combosFor(action.id).some((c) => matches(event, parseCombo(c, this.platform)))) {
          return action.id;
        }
      }
    }
    return null;
  }

  conflicts(): Conflict[] {
    const byKey = new Map<string, Conflict>();
    for (const action of this.actions.values()) {
      for (const combo of this.combosFor(action.id)) {
        const canonical = JSON.stringify(parseCombo(combo, this.platform));
        const id = `${action.scope}|${canonical}`;
        const entry = byKey.get(id) ?? { combo, scope: action.scope, actions: [] };
        entry.actions.push(action.id);
        byKey.set(id, entry);
      }
    }
    return [...byKey.values()].filter((c) => c.actions.length > 1);
  }

  on(id: string, handler: () => void): () => void {
    const set = this.handlers.get(id) ?? new Set();
    set.add(handler);
    this.handlers.set(id, set);
    return () => set.delete(handler);
  }

  /** Runs the handler for `event`, if any. Returns whether it was handled. */
  handle(event: KeyboardEvent, activeScopes: readonly string[]): boolean {
    const id = this.resolve(event, activeScopes);
    const handlers = id ? this.handlers.get(id) : undefined;
    if (!handlers?.size) return false;
    event.preventDefault();
    handlers.forEach((h) => h());
    return true;
  }
}
