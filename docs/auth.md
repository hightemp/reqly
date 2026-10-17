# Authentication

Every request and folder has an **Auth** tab. Requests inherit the auth of the
nearest folder that defines one unless they set their own (or "No auth").
All text fields accept templates (`{{ token }}`, `{% prompt %}`…).

| Scheme | How it is applied |
| --- | --- |
| Basic, Bearer, API key | Header or query parameter. |
| Digest | Answers the server's `401` challenge on the same connection: MD5, SHA-256 and their `-sess` variants, `qop=auth` and `auth-int`. |
| NTLM | NTLMv2 handshake on one connection. The domain may also be written as `DOMAIN\user`. |
| OAuth 1.0 | HMAC-SHA1/256/512, RSA-SHA1/256 (PEM private key) or PLAINTEXT; parameters in the header, the query or a form body; optional `oauth_body_hash`. Nonce and timestamp are generated unless given. |
| OAuth 2.0 | See below. |
| AWS Signature v4 | Signs method, path, query, headers and payload (`X-Amz-Content-Sha256` for S3). Session tokens are supported. |
| Hawk | `Authorization: Hawk …` with optional payload hash, `ext`, `app` and `dlg`. |
| Atlassian ASAP | An RS256 JWT signed per request (`iss`, `sub`, `aud`, `kid`, extra claims, lifetime). |
| JWT | A JWT signed per request: HS256/384/512, RS*, PS*, ES256/384 or EdDSA; header or query parameter. |
| Akamai EdgeGrid | `EG1-HMAC-SHA256` with the selected headers and up to `max body` bytes of the payload. |
| Netrc | Credentials for the host from a netrc file (`~/.netrc` by default). Desktop only. |

Signing happens right before sending, after templates are rendered, so
"Copy as cURL" and code generation show the unsigned request.

## OAuth 2.0

Grants: **authorization code** (with PKCE, S256), **client credentials**,
**password**, **device code** and **implicit** (legacy).

- Tokens belong to the request or folder that defines the auth and are stored
  encrypted next to the other secrets. Requests that inherit the auth share
  the folder's token.
- Client credentials and password grants fetch a token automatically on
  send. Authorization code, implicit and device code need a sign-in: press
  **Sign in** in the Auth tab.
- Expired tokens are refreshed automatically when the provider issued a
  refresh token ("Refresh automatically"). Changing the grant, URLs, client,
  scope, audience, resource or username marks the token as stale and it is
  fetched again.
- **Clear token** forgets it.

### Redirect URL

- **Desktop:** the sign-in page opens in a Reqly window. The redirect is
  caught before it loads, so any URL registered at the provider works; the
  default is `http://127.0.0.1/oauth2/callback`.
- **Web (server):** the sign-in page opens in a pop-up and the provider
  redirects to `<Reqly URL>/oauth2/callback`, which hands the result back to
  the app. Register that URL at the provider (the Auth tab shows it), or set
  your own redirect URL that ends on that page.

The token endpoint is called by Reqly itself (desktop) or by the server, and
on the server it is subject to the same network policy as other requests.
