# Authentication

Every request and folder has an **Auth** tab. A request uses the auth of the
nearest folder that defines one unless it sets its own (or **No auth**).
All fields accept [variables](help:variables).

| Scheme                                     | Notes                                                              |
| ------------------------------------------ | ------------------------------------------------------------------ |
| Basic, Bearer, API key                     | a header or a query parameter                                      |
| Digest                                     | answers the server's challenge (MD5, SHA-256, `-sess`, `auth-int`) |
| NTLM                                       | NTLMv2; the user may be written as `DOMAIN\user`                   |
| OAuth 1.0                                  | HMAC, RSA or PLAINTEXT signatures                                  |
| OAuth 2.0                                  | see below                                                          |
| AWS Signature v4                           | with session tokens and S3 payload hashes                          |
| Hawk, Atlassian ASAP, JWT, Akamai EdgeGrid | signed per request                                                 |
| Netrc                                      | credentials from `~/.netrc` (desktop only)                         |

Signatures are added right before sending, so **Copy as cURL** and generated
code show the unsigned request.

## OAuth 2.0

Grants: authorization code (with PKCE), client credentials, password, device
code and implicit.

- Client credentials and password tokens are fetched automatically when
  sending.
- Authorization code, implicit and device code need a sign-in: press
  **Sign in** in the Auth tab. On desktop the provider's page opens in a
  Reqly window; in the browser, in a pop-up.
- Tokens are stored encrypted and refreshed automatically when the provider
  gives a refresh token. Requests that inherit the auth share the folder's
  token. **Clear token** forgets it.
