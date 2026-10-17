import { I18n, detectLocale, type MessageKey } from './i18n.svelte';
import { en, type Messages } from './locales/en';
import { ru } from './locales/ru';

export const LOCALES = ['en', 'ru'] as const;
export type Locale = (typeof LOCALES)[number];
export type Key = MessageKey<Messages>;

export const i18n = new I18n<Locale, Messages>(
  { en, ru },
  {
    fallback: 'en',
    locale: detectLocale(
      typeof navigator === 'undefined' ? [] : navigator.languages,
      LOCALES,
      'en',
    ),
  },
);

/** Shorthand used in components: `t('sidebar.title')`. */
export const t = i18n.t;
