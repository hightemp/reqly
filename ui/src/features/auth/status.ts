// Server sign-in state from `GET /api/v1/auth/status`.

export interface AuthStatus {
  mode: 'none' | 'token' | 'multi';
  authenticated: boolean;
  /** Who may create an account (`multi` mode only). */
  registration?: 'open' | 'invite' | 'admin';
}

/** `null` when the server cannot be asked (desktop, offline). */
export function authStatus(): Promise<AuthStatus | null> {
  return fetch('/api/v1/auth/status', { credentials: 'same-origin' })
    .then((r) => (r.ok ? (r.json() as Promise<AuthStatus>) : null))
    .catch(() => null);
}

const INVITE_PARAM = 'invite';

/** The invitation token of the current link (`?invite=…`). */
export function inviteToken(): string | null {
  return new URLSearchParams(location.search).get(INVITE_PARAM);
}

/** Removes the invitation from the address once it is used. */
export function forgetInvite() {
  const url = new URL(location.href);
  url.searchParams.delete(INVITE_PARAM);
  history.replaceState(history.state, '', url);
}

/** The link that opens Reqly with an invitation. */
export function inviteLink(token: string): string {
  const url = new URL('/', location.origin);
  url.searchParams.set(INVITE_PARAM, token);
  return url.toString();
}

export interface InvitePreview {
  workspaceName: string | null;
  role: 'owner' | 'editor' | 'viewer' | null;
  invitedBy: string;
}

/** `null` for an unknown, used or expired invitation. */
export function invitePreview(token: string): Promise<InvitePreview | null> {
  return fetch(`/api/v1/auth/invite/${encodeURIComponent(token)}`, {
    credentials: 'same-origin',
  })
    .then((r) => (r.ok ? (r.json() as Promise<InvitePreview>) : null))
    .catch(() => null);
}
