import { afterEach, describe, expect, it, vi } from 'vitest';
import { CALLBACK_PATH, handleCallbackPage, signIn, SignInError, webRedirectUri } from './oauth2';

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
