import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  CALLBACK_PATH,
  handleCallbackPage,
  isRedirectOf,
  signIn,
  SignInError,
  webRedirectUri,
} from './oauth2';

describe('oauth2 sign-in (web)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    history.replaceState(null, '', '/');
  });

  it('uses the callback page of this origin', () => {
    expect(webRedirectUri()).toBe(`${location.origin}${CALLBACK_PATH}`);
  });

  it('resolves with the URL posted by the callback page', async () => {
    const fake = { closed: false } as Window;
    vi.spyOn(window, 'open').mockReturnValue(fake);
    const pending = signIn('https://idp.test/authorize', webRedirectUri()!, 'Sign in');
    window.dispatchEvent(
      new MessageEvent('message', {
        origin: location.origin,
        data: { type: 'reqly-oauth2-callback', url: 'http://x/oauth2/callback?code=1' },
      }),
    );
    await expect(pending).resolves.toBe('http://x/oauth2/callback?code=1');
  });

  it('reports a blocked pop-up', async () => {
    vi.spyOn(window, 'open').mockReturnValue(null);
    await expect(signIn('https://idp.test/', '', '')).rejects.toBeInstanceOf(SignInError);
  });

  it('the callback page posts to its opener', () => {
    history.replaceState(null, '', `${CALLBACK_PATH}?code=abc&state=s`);
    const postMessage = vi.fn();
    Object.defineProperty(window, 'opener', { value: { postMessage }, configurable: true });
    const close = vi.spyOn(window, 'close').mockImplementation(() => {});
    expect(handleCallbackPage()).toBe(true);
    expect(postMessage).toHaveBeenCalledWith(
      { type: 'reqly-oauth2-callback', url: `${location.origin}${CALLBACK_PATH}?code=abc&state=s` },
      location.origin,
    );
    expect(close).toHaveBeenCalled();
    Object.defineProperty(window, 'opener', { value: null, configurable: true });
  });
});

describe('oauth2 sign-in (Android)', () => {
  const redirect = 'http://localhost:8000/callback';

  it('opens the browser and takes the redirected URL the user pastes', async () => {
    const open = vi.fn(async () => {});
    const ask = vi.fn(async () => `  ${redirect}?code=42  `);
    const url = await signIn('https://idp.test/authorize', redirect, 'Sign in', {
      mobile: { open, ask },
    });
    expect(open).toHaveBeenCalledWith('https://idp.test/authorize');
    expect(ask).toHaveBeenCalledWith(redirect);
    expect(url).toBe(`${redirect}?code=42`);
  });

  it('is cancelled when nothing is pasted', async () => {
    const pending = signIn('https://idp.test/', redirect, '', {
      mobile: { open: async () => {}, ask: async () => null },
    });
    await expect(pending).rejects.toMatchObject({ code: 'cancelled' });
  });

  it('accepts only URLs of the redirect URI', () => {
    expect(isRedirectOf(`${redirect}?code=1`, redirect)).toBe(true);
    expect(isRedirectOf(`${redirect}#access_token=1`, redirect)).toBe(true);
    expect(isRedirectOf('http://evil.test/callback?code=1', redirect)).toBe(false);
    expect(isRedirectOf('not a url', redirect)).toBe(false);
  });
});
