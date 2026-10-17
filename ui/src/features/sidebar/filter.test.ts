import { describe, expect, it } from 'vitest';
import { highlight, makeFilter, parseQuery } from './filter';

const item = (label: string, method: string | null, url: string | null) => ({
  id: label,
  parentId: null,
  isFolder: method === null,
  label,
  method,
  url,
  createdAt: 0,
  updatedAt: 0,
});

describe('parseQuery', () => {
  it('splits prefixes and words', () => {
    expect(parseQuery(' method:post  url:/users list ')).toEqual({
      method: 'POST',
      url: ['/users'],
      words: ['list'],
    });
    expect(parseQuery('')).toEqual({ method: null, url: [], words: [] });
  });
});

describe('makeFilter', () => {
  it('returns undefined for empty queries', () => {
    expect(makeFilter('  ')).toBeUndefined();
  });

  it('matches name or url words, method and url filters', () => {
    const f = makeFilter('method:get user')!;
    expect(f(item('List users', 'GET', 'http://a/u'))).toBe(true);
    expect(f(item('Create user', 'POST', 'http://a/u'))).toBe(false);
    expect(makeFilter('healthz')!(item('Ping', 'GET', 'http://a/healthz'))).toBe(true);
    expect(makeFilter('url:orders')!(item('Orders', 'GET', 'http://a/users'))).toBe(false);
  });

  it('folders match by name only', () => {
    expect(makeFilter('api')!(item('api', null, null))).toBe(true);
    expect(makeFilter('method:get')!(item('api', null, null))).toBe(false);
  });
});

describe('highlight', () => {
  it('splits text around case-insensitive matches', () => {
    expect(highlight('List Users', ['user'])).toEqual([
      { text: 'List ', match: false },
      { text: 'User', match: true },
      { text: 's', match: false },
    ]);
    expect(highlight('abc', [])).toEqual([{ text: 'abc', match: false }]);
  });
});
