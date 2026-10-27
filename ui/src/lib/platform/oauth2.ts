// Interactive OAuth 2.0 sign-in. Desktop opens a native window and catches
// the redirect before it loads; the browser opens a pop-up that lands on
// `/oauth2/callback` of this app, which hands the URL back to the opener.
// Android has a single webview: the sign-in page opens in the system
// browser and the user pastes the URL it redirected to.

import { isMobile, isTauri } from '$lib/api/transport';
import { t } from '$lib/i18n';
import { dialogs } from '$lib/ui/overlay/dialogs.svelte';

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

export interface MobileSignIn {
  /** Opens the URL in the system browser. */
  open(url: string): Promise<void>;
  /** Asks for the URL the browser ended up on; null when cancelled. */
  ask(redirectUri: string): Promise<string | null>;
}

/** Whether `url` is where the provider sends the user back (`redirectUri`). */
export function isRedirectOf(url: string, redirectUri: string): boolean {
  const base = (u: string) => {
    const parsed = new URL(u);
    return `${parsed.origin}${parsed.pathname}`;
  };
  try {
    return base(url) === base(redirectUri);
  } catch {
    return false;
  }
}

function mobileSignIn(): MobileSignIn {
  return {
    open: async (url) => {
      const { openUrl } = await import('@tauri-apps/plugin-opener');
      await openUrl(url);
    },
    ask: (redirectUri) =>
      dialogs.prompt({
        title: t('auth.token2.pasteTitle'),
        label: t('auth.token2.pasteLabel'),
        placeholder: `${redirectUri}?code=…`,
        confirmLabel: t('auth.token2.pasteConfirm'),
        validate: (v) =>
          isRedirectOf(v.trim(), redirectUri) ? null : t('auth.token2.pasteInvalid'),
      }),
  };
}

/** Opens `authorizeUrl`; resolves with the URL the provider redirected to. */
export async function signIn(
  authorizeUrl: string,
  redirectUri: string,
  title: string,
  deps: { mobile?: MobileSignIn } = {},
): Promise<string> {
  const mobile = deps.mobile ?? (isTauri() && isMobile() ? mobileSignIn() : undefined);
  if (mobile) {
    await mobile.open(authorizeUrl);
    const url = (await mobile.ask(redirectUri))?.trim();
    if (!url) throw new SignInError('cancelled');
    return url;
  }
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
