import { describe, expect, it, vi } from 'vitest';
import { I18n, detectLocale } from './i18n.svelte';

const en = {
  app: { title: 'Reqly', greet: 'Hello, {name}!' },
  items: { one: '{count} item', other: '{count} items' },
} as const;

const ru = {
  app: { title: 'Reqly', greet: 'Привет, {name}!' },
  items: {
    one: '{count} элемент',
    few: '{count} элемента',
    many: '{count} элементов',
    other: '{count} элемента',
  },
};

function make(locale: 'en' | 'ru' = 'en') {
  return new I18n<'en' | 'ru', typeof en>({ en, ru }, { fallback: 'en', locale });
}

describe('I18n', () => {
  it('translates nested keys with interpolation', () => {
    const i = make();
    expect(i.t('app.title')).toBe('Reqly');
    expect(i.t('app.greet', { name: 'Ann' })).toBe('Hello, Ann!');
  });

  it('switches locale at runtime', () => {
    const i = make();
    i.setLocale('ru');
    expect(i.locale).toBe('ru');
    expect(i.t('app.greet', { name: 'Аня' })).toBe('Привет, Аня!');
  });

  it.each([
    [1, '1 элемент'],
    [2, '2 элемента'],
    [5, '5 элементов'],
    [11, '11 элементов'],
    [21, '21 элемент'],
    [22, '22 элемента'],
    [1.5, '1.5 элемента'],
  ])('russian plural for %s', (count, expected) => {
    expect(make('ru').t('items', { count })).toBe(expected);
  });

  it('english plural', () => {
    const i = make();
    expect(i.t('items', { count: 1 })).toBe('1 item');
    expect(i.t('items', { count: 0 })).toBe('0 items');
  });

  it('falls back to english and warns once for missing keys', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const i = new I18n<'en' | 'ru', typeof en>(
      { en, ru: { app: { title: 'Рекли' } } as never },
      { fallback: 'en', locale: 'ru' },
    );
    expect(i.t('app.greet', { name: 'X' })).toBe('Hello, X!');
    i.t('app.greet', { name: 'X' });
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });

  it('returns the key when missing everywhere', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(make().t('nope.key' as never)).toBe('nope.key');
    warn.mockRestore();
  });

  it('leaves unknown placeholders intact', () => {
    expect(make().t('app.greet')).toBe('Hello, {name}!');
  });

  it('formats numbers, bytes and durations per locale', () => {
    const i = make('ru');
    expect(i.formatNumber(1234.5)).toBe('1 234,5');
    expect(make().formatBytes(0)).toBe('0 B');
    expect(make().formatBytes(1536)).toBe('1.5 KB');
    expect(make().formatBytes(5 * 1024 * 1024)).toBe('5 MB');
    expect(make().formatDuration(850)).toBe('850 ms');
    expect(make().formatDuration(2345)).toBe('2.35 s');
  });
});

describe('detectLocale', () => {
  it('maps browser languages to supported locales', () => {
    expect(detectLocale(['ru-RU', 'en'], ['en', 'ru'], 'en')).toBe('ru');
    expect(detectLocale(['de-DE'], ['en', 'ru'], 'en')).toBe('en');
    expect(detectLocale([], ['en', 'ru'], 'en')).toBe('en');
  });
});
