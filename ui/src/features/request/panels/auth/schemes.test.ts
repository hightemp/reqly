import { describe, expect, it } from 'vitest';
import { en } from '$lib/i18n/locales/en';
import { defaults, FIELDS, KINDS } from './schemes';

describe('auth schemes', () => {
  it('every field exists on the default value and has a label', () => {
    for (const { kind, label } of KINDS) {
      const value = defaults(kind) as Record<string, unknown>;
      expect(value.type).toBe(kind);
      expect(en.auth).toHaveProperty(label);
      for (const f of FIELDS[kind] ?? []) {
        expect(value, `${kind}.${f.key}`).toHaveProperty(f.key);
        expect(en.auth, `${kind}.${f.label}`).toHaveProperty(f.label);
        for (const o of f.options ?? []) {
          if (!o.literal) expect(en.auth).toHaveProperty(o.label);
        }
      }
    }
  });

  it('carries credentials over between username/password schemes', () => {
    const basic = defaults('basic');
    const digest = defaults('digest', { ...basic, username: 'u', password: 'p' } as never);
    expect(digest).toEqual({ type: 'digest', username: 'u', password: 'p' });
  });
});
