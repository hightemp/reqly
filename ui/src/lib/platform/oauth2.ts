// Interactive OAuth 2.0 sign-in. Desktop opens a native window and catches
// the redirect before it loads; the browser opens a pop-up that lands on
// `/oauth2/callback` of this app, which hands the URL back to the opener.

import { isTauri } from '$lib/api/transport';

export const CALLBACK_PATH = '/oauth2/callback';
const MESSAGE = 'reqly-oauth2-callback';

export class SignInError extends Error {
  constructor(readonly code: 'popupBlocked' | 'cancelled') {
    super(code);
  }
}

/** Redirect URI used when the config has none (the web callback page). */
export function webRedirectUri(): string | undefined {
  return isTauri() ? undefined : `${window.location.origin}${CALLBACK_PATH}`;
}

/** Opens `authorizeUrl`; resolves with the URL the provider redirected to. */
export async function signIn(
  authorizeUrl: string,
  redirectUri: string,
  title: string,
): Promise<string> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string>('oauth2_sign_in', { url: authorizeUrl, redirectUri, title });
  }
  return popup(authorizeUrl);
}

function popup(url: string): Promise<string> {
  const win = window.open(url, 'reqly-oauth2', 'popup,width=520,height=720');
  if (!win) return Promise.reject(new SignInError('popupBlocked'));
  return new Promise((resolve, reject) => {
    const done = () => {
      window.removeEventListener('message', onMessage);
      clearInterval(timer);
    };
    const onMessage = (e: MessageEvent) => {
      if (e.origin !== window.location.origin || e.data?.type !== MESSAGE) return;
      done();
      resolve(String(e.data.url));
    };
    window.addEventListener('message', onMessage);
    const timer = setInterval(() => {
      if (win.closed) {
        done();
        reject(new SignInError('cancelled'));
      }
    }, 500);
  });
}

/**
 * On the callback page (inside the pop-up): passes the URL to the opener and
 * closes. Returns true when the page was the callback, so the app should not start.
 */
export function handleCallbackPage(): boolean {
  if (window.location.pathname !== CALLBACK_PATH || !window.opener) return false;
  (window.opener as Window).postMessage(
    { type: MESSAGE, url: window.location.href },
    window.location.origin,
  );
  window.close();
  return true;
}
