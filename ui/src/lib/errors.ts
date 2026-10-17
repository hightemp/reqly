import { ApiError } from './api/transport';
import { i18n, t, type Key } from './i18n';

// i18n-dynamic: errors.
/** Localized message for any thrown value; core error codes map to `errors.<code>`. */
export function errorMessage(error: unknown): string {
  const err = ApiError.from(error);
  const key = `errors.${err.code}` as Key;
  const message = t(key, err.params as Record<string, string | number>);
  return message === key ? `${t('errors.unknown')}: ${err.message}` : message;
}

export { i18n };
