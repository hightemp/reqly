// Auth schemes as data: defaults and the fields each form shows.
// Labels are keys under `auth.` (i18n-dynamic: auth.).

import type { Auth } from '$lib/api/generated/Auth';
import type { OAuth2Config } from '$lib/api/generated/OAuth2Config';

export type AuthKind = Auth['type'];

export type FieldKind = 'text' | 'secret' | 'multiline' | 'select' | 'flag' | 'number';

export interface FieldSpec {
  key: string;
  /** i18n key under `auth.` */
  label: string;
  kind: FieldKind;
  /** select: value → i18n key under `auth.` (or a literal when `literal`). */
  options?: { value: string; label: string; literal?: boolean }[];
  placeholder?: string;
  /** Shown only when this returns true. */
  when?: (auth: Record<string, unknown>) => boolean;
}

/** Order of the type picker; `inherit` is added when allowed. */
export const KINDS: { kind: AuthKind; label: string }[] = [
  { kind: 'none', label: 'none' },
  { kind: 'basic', label: 'basic' },
  { kind: 'bearer', label: 'bearer' },
  { kind: 'apiKey', label: 'apiKey' },
  { kind: 'digest', label: 'digest' },
  { kind: 'ntlm', label: 'ntlm' },
  { kind: 'oauth1', label: 'oauth1' },
  { kind: 'oauth2', label: 'oauth2' },
  { kind: 'awsSigV4', label: 'awsSigV4' },
  { kind: 'hawk', label: 'hawk' },
  { kind: 'asap', label: 'asap' },
  { kind: 'jwt', label: 'jwt' },
  { kind: 'edgeGrid', label: 'edgeGrid' },
  { kind: 'netrc', label: 'netrc' },
];

/** Kinds that read local files and so only work on desktop. */
export const DESKTOP_ONLY: AuthKind[] = ['netrc'];

export const OAUTH2_DEFAULTS: OAuth2Config = {
  grant: 'authorizationCode',
  authUrl: '',
  tokenUrl: '',
  deviceUrl: '',
  clientId: '',
  clientSecret: '',
  clientAuth: 'basic',
  scope: '',
  audience: '',
  resource: '',
  username: '',
  password: '',
  redirectUrl: '',
  pkce: true,
  state: '',
  tokenPrefix: '',
  useIdToken: false,
  autoRefresh: true,
};

/** A fresh value of `kind`, carrying over username/password where both have them. */
export function defaults(kind: AuthKind, from?: Auth): Auth {
  const prev = (from ?? {}) as Record<string, unknown>;
  const user = typeof prev.username === 'string' ? prev.username : '';
  const pass = typeof prev.password === 'string' ? prev.password : '';
  switch (kind) {
    case 'basic':
      return { type: 'basic', username: user, password: pass };
    case 'bearer':
      return { type: 'bearer', token: '', prefix: '' };
    case 'apiKey':
      return { type: 'apiKey', key: '', value: '', location: 'header' };
    case 'digest':
      return { type: 'digest', username: user, password: pass };
    case 'ntlm':
      return { type: 'ntlm', username: user, password: pass, domain: '' };
    case 'oauth1':
      return {
        type: 'oauth1',
        consumerKey: '',
        consumerSecret: '',
        token: '',
        tokenSecret: '',
        signatureMethod: 'HMAC-SHA1',
        privateKey: '',
        realm: '',
        callback: '',
        verifier: '',
        nonce: '',
        timestamp: '',
        location: 'header',
        includeBodyHash: false,
      };
    case 'oauth2':
      return { type: 'oauth2', ...OAUTH2_DEFAULTS };
    case 'awsSigV4':
      return {
        type: 'awsSigV4',
        accessKey: '',
        secretKey: '',
        sessionToken: '',
        region: '',
        service: '',
      };
    case 'hawk':
      return {
        type: 'hawk',
        id: '',
        key: '',
        algorithm: 'sha256',
        ext: '',
        app: '',
        dlg: '',
        includePayloadHash: false,
      };
    case 'asap':
      return {
        type: 'asap',
        issuer: '',
        subject: '',
        audience: '',
        keyId: '',
        privateKey: '',
        additionalClaims: '',
        expiresInSecs: 600,
      };
    case 'netrc':
      return { type: 'netrc', file: '' };
    case 'edgeGrid':
      return {
        type: 'edgeGrid',
        accessToken: '',
        clientToken: '',
        clientSecret: '',
        headersToSign: '',
        maxBody: 131072,
      };
    case 'jwt':
      return {
        type: 'jwt',
        algorithm: 'HS256',
        secret: '',
        payload: '{\n  "sub": ""\n}',
        header: '',
        prefix: '',
        location: 'header',
        queryParam: 'token',
      };
    case 'inherit':
    case 'none':
      return { type: kind };
  }
}

const literal = (values: string[]) => values.map((v) => ({ value: v, label: v, literal: true }));
const isRsa = (a: Record<string, unknown>) => String(a.signatureMethod).startsWith('RSA');
const grantIs =
  (...grants: string[]) =>
  (a: Record<string, unknown>) =>
    grants.includes(String(a.grant));
const JWT_ALGORITHMS = [
  'HS256',
  'HS384',
  'HS512',
  'RS256',
  'RS384',
  'RS512',
  'PS256',
  'PS384',
  'PS512',
  'ES256',
  'ES384',
  'EdDSA',
];

export const FIELDS: Partial<Record<AuthKind, FieldSpec[]>> = {
  basic: [
    { key: 'username', label: 'username', kind: 'text' },
    { key: 'password', label: 'password', kind: 'secret' },
  ],
  bearer: [
    { key: 'token', label: 'token', kind: 'secret' },
    { key: 'prefix', label: 'prefix', kind: 'text', placeholder: 'Bearer' },
  ],
  apiKey: [
    { key: 'key', label: 'key', kind: 'text', placeholder: 'X-API-Key' },
    { key: 'value', label: 'value', kind: 'secret' },
    {
      key: 'location',
      label: 'addTo',
      kind: 'select',
      options: [
        { value: 'header', label: 'inHeader' },
        { value: 'query', label: 'inQuery' },
      ],
    },
  ],
  digest: [
    { key: 'username', label: 'username', kind: 'text' },
    { key: 'password', label: 'password', kind: 'secret' },
  ],
  ntlm: [
    { key: 'username', label: 'username', kind: 'text' },
    { key: 'password', label: 'password', kind: 'secret' },
    { key: 'domain', label: 'domain', kind: 'text' },
  ],
  oauth1: [
    {
      key: 'signatureMethod',
      label: 'signatureMethod',
      kind: 'select',
      options: literal([
        'HMAC-SHA1',
        'HMAC-SHA256',
        'HMAC-SHA512',
        'RSA-SHA1',
        'RSA-SHA256',
        'PLAINTEXT',
      ]),
    },
    { key: 'consumerKey', label: 'consumerKey', kind: 'text' },
    { key: 'consumerSecret', label: 'consumerSecret', kind: 'secret', when: (a) => !isRsa(a) },
    { key: 'privateKey', label: 'privateKey', kind: 'multiline', when: isRsa },
    { key: 'token', label: 'accessToken', kind: 'text' },
    { key: 'tokenSecret', label: 'tokenSecret', kind: 'secret', when: (a) => !isRsa(a) },
    {
      key: 'location',
      label: 'addTo',
      kind: 'select',
      options: [
        { value: 'header', label: 'inHeader' },
        { value: 'query', label: 'inQuery' },
        { value: 'body', label: 'inBody' },
      ],
    },
    { key: 'realm', label: 'realm', kind: 'text' },
    { key: 'callback', label: 'callbackUrl', kind: 'text' },
    { key: 'verifier', label: 'verifier', kind: 'text' },
    { key: 'nonce', label: 'nonce', kind: 'text', placeholder: 'auto' },
    { key: 'timestamp', label: 'timestamp', kind: 'text', placeholder: 'auto' },
    { key: 'includeBodyHash', label: 'includeBodyHash', kind: 'flag' },
  ],
  oauth2: [
    {
      key: 'grant',
      label: 'grant',
      kind: 'select',
      options: [
        { value: 'authorizationCode', label: 'grantAuthorizationCode' },
        { value: 'clientCredentials', label: 'grantClientCredentials' },
        { value: 'password', label: 'grantPassword' },
        { value: 'deviceCode', label: 'grantDeviceCode' },
        { value: 'implicit', label: 'grantImplicit' },
      ],
    },
    {
      key: 'authUrl',
      label: 'authUrl',
      kind: 'text',
      when: grantIs('authorizationCode', 'implicit'),
    },
    { key: 'deviceUrl', label: 'deviceUrl', kind: 'text', when: grantIs('deviceCode') },
    { key: 'tokenUrl', label: 'tokenUrl', kind: 'text', when: (a) => a.grant !== 'implicit' },
    { key: 'clientId', label: 'clientId', kind: 'text' },
    {
      key: 'clientSecret',
      label: 'clientSecret',
      kind: 'secret',
      when: (a) => a.grant !== 'implicit',
    },
    { key: 'username', label: 'username', kind: 'text', when: grantIs('password') },
    { key: 'password', label: 'password', kind: 'secret', when: grantIs('password') },
    {
      key: 'redirectUrl',
      label: 'redirectUrl',
      kind: 'text',
      placeholder: 'auto',
      when: grantIs('authorizationCode', 'implicit'),
    },
    { key: 'scope', label: 'scope', kind: 'text' },
    { key: 'pkce', label: 'pkce', kind: 'flag', when: grantIs('authorizationCode') },
    { key: 'audience', label: 'audience', kind: 'text' },
    { key: 'resource', label: 'resource', kind: 'text' },
    {
      key: 'state',
      label: 'state',
      kind: 'text',
      placeholder: 'auto',
      when: grantIs('authorizationCode', 'implicit'),
    },
    {
      key: 'clientAuth',
      label: 'clientAuth',
      kind: 'select',
      options: [
        { value: 'basic', label: 'clientAuthBasic' },
        { value: 'body', label: 'clientAuthBody' },
      ],
      when: (a) => a.grant !== 'implicit',
    },
    { key: 'tokenPrefix', label: 'prefix', kind: 'text', placeholder: 'Bearer' },
    { key: 'useIdToken', label: 'useIdToken', kind: 'flag' },
    { key: 'autoRefresh', label: 'autoRefresh', kind: 'flag' },
  ],
  awsSigV4: [
    { key: 'accessKey', label: 'accessKeyId', kind: 'text' },
    { key: 'secretKey', label: 'secretAccessKey', kind: 'secret' },
    { key: 'sessionToken', label: 'sessionToken', kind: 'secret' },
    { key: 'region', label: 'region', kind: 'text', placeholder: 'us-east-1' },
    { key: 'service', label: 'service', kind: 'text', placeholder: 'execute-api' },
  ],
  hawk: [
    { key: 'id', label: 'hawkId', kind: 'text' },
    { key: 'key', label: 'key', kind: 'secret' },
    {
      key: 'algorithm',
      label: 'algorithm',
      kind: 'select',
      options: [
        { value: 'sha256', label: 'SHA-256', literal: true },
        { value: 'sha1', label: 'SHA-1', literal: true },
      ],
    },
    { key: 'ext', label: 'ext', kind: 'text' },
    { key: 'app', label: 'app', kind: 'text' },
    { key: 'dlg', label: 'dlg', kind: 'text' },
    { key: 'includePayloadHash', label: 'includePayloadHash', kind: 'flag' },
  ],
  asap: [
    { key: 'issuer', label: 'issuer', kind: 'text' },
    { key: 'subject', label: 'subject', kind: 'text' },
    { key: 'audience', label: 'audience', kind: 'text' },
    { key: 'keyId', label: 'keyId', kind: 'text' },
    { key: 'privateKey', label: 'privateKey', kind: 'multiline' },
    { key: 'additionalClaims', label: 'additionalClaims', kind: 'multiline', placeholder: '{}' },
    { key: 'expiresInSecs', label: 'expiresIn', kind: 'number' },
  ],
  jwt: [
    { key: 'algorithm', label: 'algorithm', kind: 'select', options: literal(JWT_ALGORITHMS) },
    { key: 'secret', label: 'secretOrKey', kind: 'multiline' },
    { key: 'payload', label: 'payload', kind: 'multiline' },
    { key: 'header', label: 'jwtHeader', kind: 'multiline', placeholder: '{}' },
    {
      key: 'location',
      label: 'addTo',
      kind: 'select',
      options: [
        { value: 'header', label: 'inHeader' },
        { value: 'query', label: 'inQuery' },
      ],
    },
    {
      key: 'prefix',
      label: 'prefix',
      kind: 'text',
      placeholder: 'Bearer',
      when: (a) => a.location === 'header',
    },
    { key: 'queryParam', label: 'queryParam', kind: 'text', when: (a) => a.location === 'query' },
  ],
  edgeGrid: [
    { key: 'accessToken', label: 'accessToken', kind: 'text' },
    { key: 'clientToken', label: 'clientToken', kind: 'text' },
    { key: 'clientSecret', label: 'clientSecret', kind: 'secret' },
    { key: 'headersToSign', label: 'headersToSign', kind: 'text' },
    { key: 'maxBody', label: 'maxBody', kind: 'number' },
  ],
  netrc: [{ key: 'file', label: 'netrcFile', kind: 'text', placeholder: '~/.netrc' }],
};
