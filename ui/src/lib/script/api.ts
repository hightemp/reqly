// The script API as a tree, for completion (mirrors the prelude in
// crates/reqly-script/src/prelude.js). Snippet labels: i18n-dynamic: scripts.snippet.

import type { FakerInfo } from '$lib/api/generated/FakerInfo';

export interface ApiNode {
  /** `fn` entries complete with `()`. */
  kind: 'object' | 'fn' | 'prop';
  detail?: string;
  /** Short description shown in the completion list. */
  info?: string;
  children?: Record<string, ApiNode>;
}

const fn = (detail: string, info?: string): ApiNode => ({ kind: 'fn', detail, info });
const prop = (detail: string, info?: string): ApiNode => ({ kind: 'prop', detail, info });
const obj = (children: Record<string, ApiNode>, info?: string): ApiNode => ({
  kind: 'object',
  children,
  info,
});

const list = (item: string): ApiNode =>
  obj({
    get: fn('(key) → string', `Value of a ${item}`),
    has: fn('(key, value?) → boolean'),
    add: fn('({ key, value }) | "key: value"', `Add a ${item}`),
    upsert: fn('({ key, value })', `Add or replace a ${item}`),
    remove: fn('(key | predicate)', `Remove ${item}s`),
    all: fn('() → array'),
    each: fn('(fn)'),
    toObject: fn('() → object'),
    count: fn('() → number'),
  });

const scope = (what: string): ApiNode =>
  obj(
    {
      get: fn('(key) → value', `Read a ${what} variable`),
      set: fn('(key, value)', `Write a ${what} variable`),
      unset: fn('(key)', `Remove a ${what} variable`),
      has: fn('(key) → boolean'),
      clear: fn('()'),
      toObject: fn('() → object'),
      replaceIn: fn('(text) → string', 'Replace {{variables}} in text'),
    },
    `${what} variables`,
  );

const request = obj(
  {
    url: obj({
      toString: fn('() → string'),
      update: fn('(url)', 'Replace the URL (and query)'),
      query: list('query parameter'),
      getHost: fn('() → string'),
      getPath: fn('() → string'),
      getQueryString: fn('() → string'),
      addQueryParams: fn('(params)'),
      removeQueryParams: fn('(keys)'),
    }),
    method: prop('string'),
    headers: list('header'),
    body: obj({
      mode: prop("'raw' | 'urlencoded' | 'formdata' | 'none'"),
      raw: prop('string'),
      urlencoded: list('field'),
      formdata: list('field'),
      update: fn('(raw | { mode, raw })'),
      toString: fn('() → string'),
    }),
    addHeader: fn('({ key, value })'),
    upsertHeader: fn('({ key, value })'),
    removeHeader: fn('(key)'),
    auth: prop('object', 'Auth settings (read-only)'),
    name: prop('string'),
  },
  'The request being sent (pre-request scripts can change it)',
);

const response = obj(
  {
    code: prop('number', 'Status code'),
    status: prop('string', 'Status text'),
    headers: list('header'),
    responseTime: prop('number', 'Milliseconds'),
    responseSize: prop('number', 'Bytes'),
    json: fn('() → any', 'Body parsed as JSON'),
    text: fn('() → string', 'Body as text'),
    to: obj({
      have: obj({
        status: fn('(code | text)', 'Assert the status'),
        header: fn('(name, value?)', 'Assert a header'),
        body: fn('(text?)'),
        jsonBody: fn('(path?, value?)'),
      }),
      be: obj({ ok: prop('assertion'), json: prop('assertion'), notFound: prop('assertion') }),
      not: obj({ have: obj({ status: fn('(code)'), header: fn('(name)') }) }),
    }),
  },
  'The response (after-response scripts)',
);

const common: Record<string, ApiNode> = {
  request,
  response,
  variables: scope('any (local first)'),
  environment: scope('environment'),
  collectionVariables: scope('collection'),
  globals: scope('global'),
  iterationData: obj({
    get: fn('(key) → value'),
    has: fn('(key) → boolean'),
    toObject: fn('() → object'),
  }),
  test: fn('(name, fn)', 'Define a test'),
  expect: fn('(value) → chai assertion', 'chai expect'),
  sendRequest: fn('(request, callback?) → Promise<response>', 'Send another request'),
  info: obj({
    eventName: prop("'prerequest' | 'test'"),
    requestName: prop('string'),
    requestId: prop('string'),
    iteration: prop('number'),
    iterationCount: prop('number'),
  }),
  execution: obj({
    skipRequest: fn('()', 'Do not send this request'),
    setNextRequest: fn('(name | null)', 'Runner: what runs next'),
  }),
};

/** faker-js modules and methods from the template catalog. */
export function fakerApi(functions: FakerInfo[]): ApiNode {
  const modules: Record<string, ApiNode> = {
    seed: fn('(seed?) → number', 'Make the generated values repeatable'),
  };
  for (const f of functions) {
    const [module, name] = f.name.split('.');
    if (!module || !name) continue;
    const node = (modules[module] ??= obj({}, `faker.${module}`));
    node.children![name] = fn(`(${f.args})`);
  }
  return {
    kind: 'fn',
    detail: '(path, ...args)',
    info: 'faker-js in the data locale from the settings',
    children: modules,
  };
}

export const SCRIPT_API: Record<string, ApiNode> = {
  rq: obj({ ...common, faker: fakerApi([]) }, 'Reqly script API'),
  pm: obj(common, 'Postman-compatible API'),
  insomnia: obj(
    { ...common, baseEnvironment: scope('base environment'), faker: fakerApi([]) },
    'Insomnia-compatible API',
  ),
  faker: { ...fakerApi([]), kind: 'object' },
  console: obj({
    log: fn('(...values)'),
    info: fn('(...values)'),
    warn: fn('(...values)'),
    error: fn('(...values)'),
    debug: fn('(...values)'),
  }),
  require: fn(
    "('crypto-js' | 'lodash' | 'chai' | 'moment' | 'uuid' | '@faker-js/faker')",
    'Bundled modules',
  ),
  CryptoJS: obj({
    HmacSHA256: fn('(message, key)'),
    HmacSHA1: fn('(message, key)'),
    SHA256: fn('(message)'),
    MD5: fn('(message)'),
    enc: obj({ Base64: prop('encoder'), Hex: prop('encoder'), Utf8: prop('encoder') }),
  }),
  _: prop('lodash', 'lodash'),
  moment: fn('(date?)', 'moment.js'),
  atob: fn('(base64) → string'),
  btoa: fn('(text) → string'),
  setTimeout: fn('(fn, ms)'),
};

/** Fills the faker members once the catalog has loaded. */
export function setFakerFunctions(functions: FakerInfo[]) {
  const faker = fakerApi(functions);
  SCRIPT_API.rq!.children!.faker = faker;
  SCRIPT_API.insomnia!.children!.faker = faker;
  SCRIPT_API.faker = { ...faker, kind: 'object' };
}

export interface Snippet {
  id: string;
  /** i18n key under `scripts.snippet.` */
  label: string;
  phase: 'pre' | 'post' | 'both';
  code: string;
}

export const SNIPPETS: Snippet[] = [
  {
    id: 'status',
    label: 'status',
    phase: 'post',
    code: "rq.test('status is 200', () => {\n  rq.response.to.have.status(200);\n});\n",
  },
  {
    id: 'json-value',
    label: 'jsonValue',
    phase: 'post',
    code: "rq.test('has the expected value', () => {\n  const json = rq.response.json();\n  rq.expect(json.id).to.exist;\n});\n",
  },
  {
    id: 'time',
    label: 'responseTime',
    phase: 'post',
    code: "rq.test('responds within 500 ms', () => {\n  rq.expect(rq.response.responseTime).to.be.below(500);\n});\n",
  },
  {
    id: 'save-token',
    label: 'saveToken',
    phase: 'post',
    code: "rq.environment.set('token', rq.response.json().token);\n",
  },
  {
    id: 'set-variable',
    label: 'setVariable',
    phase: 'both',
    code: "rq.variables.set('name', 'value');\n",
  },
  {
    id: 'add-header',
    label: 'addHeader',
    phase: 'pre',
    code: "rq.request.headers.upsert({ key: 'X-Request-Id', value: require('uuid').v4() });\n",
  },
  {
    id: 'hmac',
    label: 'hmac',
    phase: 'pre',
    code: "const timestamp = String(Math.floor(Date.now() / 1000));\nconst signature = CryptoJS.HmacSHA256(rq.request.body.raw + timestamp, rq.environment.get('secret')).toString(CryptoJS.enc.Hex);\nrq.request.headers.upsert({ key: 'X-Timestamp', value: timestamp });\nrq.request.headers.upsert({ key: 'X-Signature', value: signature });\n",
  },
  {
    id: 'send',
    label: 'sendRequest',
    phase: 'both',
    code: "const res = await rq.sendRequest({\n  url: 'https://example.com/token',\n  method: 'POST',\n  header: { 'Content-Type': 'application/json' },\n  body: { mode: 'raw', raw: JSON.stringify({ id: 1 }) },\n});\nconsole.log(res.code, res.json());\n",
  },
  {
    id: 'skip',
    label: 'skip',
    phase: 'pre',
    code: "if (!rq.environment.get('token')) {\n  rq.execution.skipRequest();\n}\n",
  },
  {
    id: 'log',
    label: 'log',
    phase: 'post',
    code: 'console.log(rq.response.code, rq.response.json());\n',
  },
];
