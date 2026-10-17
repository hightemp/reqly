import { describe, expect, it, vi } from 'vitest';
import { ApiError } from './api/transport';
import { errorMessage, i18n } from './errors';

describe('errorMessage', () => {
  it('translates known codes with params', () => {
    i18n.setLocale('en');
    const err = new ApiError({ code: 'command.unknown', message: 'x', params: { name: 'a.b' } });
    expect(errorMessage(err)).toBe('Unknown command "a.b"');
  });

  it('falls back to a generic message', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    i18n.setLocale('en');
    expect(errorMessage(new Error('boom'))).toBe('Something went wrong: boom');
    warn.mockRestore();
  });
});
