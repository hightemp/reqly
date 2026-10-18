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
