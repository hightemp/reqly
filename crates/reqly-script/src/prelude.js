// Script API: `rq`, Postman-compatible `pm`, Insomnia-compatible `insomnia`,
// `console`, `require` and timers. Runs before every script; the host
// passes `__input` (JSON) and reads `__collect()` (JSON) afterwards.
//
// Native helpers (set by the host): __rq_log(level, text),
// __rq_send(json) -> Promise<json>, __rq_module(name) -> exports,
// __rq_uuid(), __rq_sleep(ms) -> Promise, __rq_replace(text) -> Promise<string>.

(function () {
  'use strict';
  const input = JSON.parse(__input);
  const state = {
    request: input.request,
    changes: [],
    tests: [],
    pending: [],
    skipRequest: false,
    nextRequest: undefined,
  };

  // ----- helpers -----------------------------------------------------------

  function format(value) {
    if (typeof value === 'string') return value;
    if (value === undefined) return 'undefined';
    if (typeof value === 'function') return value.toString();
    if (value instanceof Error) return `${value.name}: ${value.message}`;
    try {
      return JSON.stringify(value, null, 2);
    } catch (_) {
      return String(value);
    }
  }

  const levels = ['log', 'info', 'warn', 'error', 'debug'];
  const console = {};
  for (const level of levels) {
    console[level] = (...args) => __rq_log(level, args.map(format).join(' '));
  }
  console.trace = console.debug;
  console.table = console.log;
  console.dir = console.log;
  globalThis.console = console;

  const modules = {};
  function load(name) {
    if (!(name in modules)) modules[name] = __rq_module(name);
    return modules[name];
  }

  // Values in scope maps are JSON values; unset keys read as undefined.
  function copy(map) {
    return Object.assign({}, map || {});
  }

  // ----- variables ----------------------------------------------------------

  const scopes = {
    globals: copy(input.variables.globals),
    collection: copy(input.variables.collection),
    environment: copy(input.variables.environment),
    data: copy(input.variables.data),
    local: copy(input.variables.local),
  };

  function scopeApi(scope, label) {
    const map = scopes[scope];
    const api = {
      name: label,
      get: (key) => map[key],
      has: (key) => Object.prototype.hasOwnProperty.call(map, key),
      set: (key, value) => {
        map[key] = value;
        state.changes.push({ scope, key, value: value === undefined ? null : value, unset: false });
      },
      unset: (key) => {
        delete map[key];
        state.changes.push({ scope, key, value: null, unset: true });
      },
      clear: () => {
        for (const key of Object.keys(map)) api.unset(key);
      },
      toObject: () => copy(map),
      toJSON: () => copy(map),
      replaceIn: (text) => replaceIn(text),
    };
    return api;
  }

  // Resolution order of `pm.variables`: local > data > environment > collection > globals.
  function resolve(key) {
    for (const s of ['local', 'data', 'environment', 'collection', 'globals']) {
      if (Object.prototype.hasOwnProperty.call(scopes[s], key)) return scopes[s][key];
    }
    return undefined;
  }

  function replaceIn(text) {
    // Plain `{{name}}` from the script's view of variables; the rest (tags,
    // faker) stays for the request's own rendering.
    return String(text).replace(/\{\{\s*(?:_\.)?([\w.-]+)\s*\}\}/g, (m, name) => {
      const v = resolve(name);
      return v === undefined ? m : typeof v === 'string' ? v : JSON.stringify(v);
    });
  }

  const variables = Object.assign(scopeApi('local', 'variables'), {
    get: (key) => resolve(key),
    has: (key) => resolve(key) !== undefined,
    toObject: () =>
      Object.assign(
        {},
        scopes.globals,
        scopes.collection,
        scopes.environment,
        scopes.data,
        scopes.local,
      ),
  });

  // ----- property lists (headers, query, form fields) --------------------------

  class PropertyList {
    constructor(items, onChange) {
      this.items = items;
      this.onChange = onChange;
    }
    find(key) {
      const k = String(key).toLowerCase();
      return this.items.find((i) => String(i.key).toLowerCase() === k && !i.disabled);
    }
    get(key) {
      const item = this.find(key);
      return item ? item.value : undefined;
    }
    has(key, value) {
      const item = this.find(key);
      return !!item && (value === undefined || item.value === value);
    }
    one(key) {
      return this.find(key);
    }
    all() {
      return this.items.filter((i) => !i.disabled);
    }
    count() {
      return this.all().length;
    }
    add(item, value) {
      if (typeof item === 'string') item = parseHeader(item, value);
      this.items.push({ key: String(item.key), value: String(item.value ?? ''), disabled: !!item.disabled });
      this.onChange();
    }
    append(item, value) {
      this.add(item, value);
    }
    upsert(item, value) {
      if (typeof item === 'string') item = parseHeader(item, value);
      const existing = this.find(item.key);
      if (existing) existing.value = String(item.value ?? '');
      else this.items.push({ key: String(item.key), value: String(item.value ?? ''), disabled: false });
      this.onChange();
    }
    remove(predicate) {
      const match =
        typeof predicate === 'function'
          ? predicate
          : (i) => String(i.key).toLowerCase() === String(predicate).toLowerCase();
      for (let i = this.items.length - 1; i >= 0; i--) if (match(this.items[i])) this.items.splice(i, 1);
      this.onChange();
    }
    clear() {
      this.items.length = 0;
      this.onChange();
    }
    each(fn) {
      this.all().forEach(fn);
    }
    map(fn) {
      return this.all().map(fn);
    }
    filter(fn) {
      return this.all().filter(fn);
    }
    toObject() {
      const out = {};
      for (const i of this.all()) out[i.key] = i.value;
      return out;
    }
    toJSON() {
      return this.all().map((i) => ({ key: i.key, value: i.value }));
    }
    toString() {
      return this.all()
        .map((i) => `${i.key}: ${i.value}`)
        .join('\n');
    }
  }

  function parseHeader(text, value) {
    if (value !== undefined) return { key: text, value };
    const i = text.indexOf(':');
    return i < 0 ? { key: text, value: '' } : { key: text.slice(0, i).trim(), value: text.slice(i + 1).trim() };
  }

  // ----- request ------------------------------------------------------------------

  function splitUrl(raw) {
    const hash = raw.indexOf('#');
    const noHash = hash < 0 ? raw : raw.slice(0, hash);
    const q = noHash.indexOf('?');
    return q < 0
      ? { base: noHash, query: [] }
      : {
          base: noHash.slice(0, q),
          query: noHash
            .slice(q + 1)
            .split('&')
            .filter(Boolean)
            .map((p) => {
              const eq = p.indexOf('=');
              return eq < 0
                ? { key: decode(p), value: '' }
                : { key: decode(p.slice(0, eq)), value: decode(p.slice(eq + 1)) };
            }),
        };
  }

  function decode(s) {
    try {
      return decodeURIComponent(s.replace(/\+/g, ' '));
    } catch (_) {
      return s;
    }
  }

  function makeRequest() {
    const r = state.request;
    const headers = new PropertyList(r.headers, () => {});
    const query = new PropertyList(r.query, () => {});
    const url = {
      query,
      toString() {
        const q = query.all().map((p) => (p.value === '' ? p.key : `${p.key}=${p.value}`));
        return q.length ? `${r.url}?${q.join('&')}` : r.url;
      },
      update(value) {
        const { base, query: items } = splitUrl(String(value));
        r.url = base;
        r.query.length = 0;
        for (const i of items) r.query.push({ key: i.key, value: i.value, disabled: false });
      },
      getHost() {
        const m = /^[a-z]+:\/\/([^/:?#]+)/i.exec(r.url);
        return m ? m[1] : '';
      },
      getPath() {
        const m = /^[a-z]+:\/\/[^/?#]*(\/[^?#]*)?/i.exec(r.url);
        return (m && m[1]) || '/';
      },
      getQueryString() {
        return query
          .all()
          .map((p) => (p.value === '' ? p.key : `${p.key}=${p.value}`))
          .join('&');
      },
      addQueryParams(params) {
        for (const p of Array.isArray(params) ? params : [params]) {
          if (typeof p === 'string') splitUrl(`?${p}`).query.forEach((i) => query.add(i));
          else query.add(p);
        }
      },
      removeQueryParams(keys) {
        for (const k of Array.isArray(keys) ? keys : [keys]) query.remove(typeof k === 'string' ? k : k.key);
      },
      toJSON() {
        return url.toString();
      },
    };
    const body = {
      get mode() {
        return r.body.mode;
      },
      set mode(m) {
        r.body.mode = m;
      },
      get raw() {
        return r.body.raw;
      },
      set raw(v) {
        r.body.raw = String(v);
      },
      urlencoded: new PropertyList(r.body.fields, () => {}),
      formdata: new PropertyList(r.body.fields, () => {}),
      update(value) {
        if (typeof value === 'string') {
          r.body.raw = value;
          if (r.body.mode === 'none' || r.body.mode === 'formdata' || r.body.mode === 'urlencoded')
            r.body.mode = 'raw';
        } else if (value && value.mode) {
          r.body.mode = value.mode;
          if (value.raw !== undefined) r.body.raw = String(value.raw);
          const fields = value[value.mode];
          if (Array.isArray(fields)) {
            r.body.fields.length = 0;
            for (const f of fields)
              r.body.fields.push({ key: String(f.key), value: String(f.value ?? ''), disabled: !!f.disabled });
          }
        }
      },
      toString() {
        return r.body.raw;
      },
      isEmpty() {
        return r.body.mode === 'none' || (r.body.mode === 'raw' && !r.body.raw);
      },
    };
    const request = {
      id: r.id,
      name: r.name,
      headers,
      body,
      auth: r.auth,
      get url() {
        return url;
      },
      set url(v) {
        url.update(v);
      },
      get method() {
        return r.method;
      },
      set method(m) {
        r.method = String(m).toUpperCase();
      },
      addHeader(item, value) {
        headers.add(item, value);
      },
      upsertHeader(item, value) {
        headers.upsert(item, value);
      },
      removeHeader(key) {
        headers.remove(key);
      },
      toJSON() {
        return { method: r.method, url: url.toString(), headers: headers.toJSON(), body: r.body };
      },
    };
    return request;
  }

  // ----- responses ---------------------------------------------------------------

  function assertion(ok, message) {
    if (!ok) {
      const e = new Error(message);
      e.name = 'AssertionError';
      throw e;
    }
  }

  function makeResponse(res) {
    if (!res) return undefined;
    const headers = new PropertyList(
      (res.headers || []).map(([key, value]) => ({ key, value, disabled: false })),
      () => {},
    );
    let parsed;
    const response = {
      code: res.code,
      status: res.status,
      reason: () => res.status,
      headers,
      responseTime: res.timeMs,
      responseSize: res.size,
      text: () => res.body,
      json: () => {
        if (parsed === undefined) parsed = JSON.parse(res.body);
        return parsed;
      },
      cookies: new PropertyList([], () => {}),
      // Insomnia aliases.
      get body() {
        return res.body;
      },
      getStatusCode: () => res.code,
      getHeader: (name) => headers.get(name),
      toJSON() {
        return { code: res.code, status: res.status, headers: headers.toJSON(), body: res.body };
      },
    };
    const to = {
      have: {
        status(expected) {
          const ok = typeof expected === 'number' ? res.code === expected : res.status === expected;
          assertion(ok, `expected response to have status ${expected} but got ${res.code} ${res.status}`);
        },
        header(name, value) {
          assertion(headers.has(name), `expected response to have header ${name}`);
          if (value !== undefined)
            assertion(headers.get(name) === value, `expected header ${name} to be ${value} but got ${headers.get(name)}`);
        },
        body(expected) {
          if (expected === undefined) assertion(res.body.length > 0, 'expected response to have a body');
          else assertion(res.body === expected, 'expected response body to match');
        },
        jsonBody(path, value) {
          let json;
          try {
            json = JSON.parse(res.body);
          } catch (_) {
            assertion(false, 'expected response to have a JSON body');
          }
          if (path !== undefined) {
            const got = load('lodash').get(json, path);
            if (value === undefined) assertion(got !== undefined, `expected JSON body to have ${path}`);
            else assertion(load('lodash').isEqual(got, value), `expected ${path} to equal ${format(value)} but got ${format(got)}`);
          }
        },
      },
      be: {
        get ok() {
          assertion(res.code >= 200 && res.code < 300, `expected a 2xx status but got ${res.code}`);
          return true;
        },
        get success() {
          return to.be.ok;
        },
        get json() {
          try {
            JSON.parse(res.body);
          } catch (_) {
            assertion(false, 'expected response body to be JSON');
          }
          return true;
        },
        get notFound() {
          assertion(res.code === 404, `expected 404 but got ${res.code}`);
          return true;
        },
        get clientError() {
          assertion(res.code >= 400 && res.code < 500, `expected a 4xx status but got ${res.code}`);
          return true;
        },
        get serverError() {
          assertion(res.code >= 500, `expected a 5xx status but got ${res.code}`);
          return true;
        },
      },
    };
    to.not = {
      have: {
        status(expected) {
          const ok = typeof expected === 'number' ? res.code !== expected : res.status !== expected;
          assertion(ok, `expected response not to have status ${expected}`);
        },
        header(name) {
          assertion(!headers.has(name), `expected response not to have header ${name}`);
        },
      },
      be: {
        get error() {
          assertion(res.code < 400, `expected no error but got ${res.code}`);
          return true;
        },
      },
    };
    response.to = to;
    return response;
  }

  // ----- tests ----------------------------------------------------------------------

  function test(name, fn) {
    const record = { name: String(name), passed: true, error: null, skipped: false };
    state.tests.push(record);
    if (typeof fn !== 'function') {
      record.skipped = true;
      return;
    }
    const fail = (e) => {
      record.passed = false;
      record.error = format(e && e.message ? e.message : e);
    };
    try {
      const out = fn();
      if (out && typeof out.then === 'function') state.pending.push(out.then(() => {}, fail));
    } catch (e) {
      fail(e);
    }
  }
  test.skip = (name) => state.tests.push({ name: String(name), passed: true, error: null, skipped: true });

  const expect = (...args) => load('chai').expect(...args);

  // ----- sendRequest -------------------------------------------------------------

  function normalizeRequest(req) {
    if (typeof req === 'string') return { method: 'GET', url: req, headers: [], body: null };
    const headers = [];
    const h = req.header || req.headers;
    if (Array.isArray(h)) for (const i of h) headers.push([String(i.key), String(i.value)]);
    else if (h && typeof h === 'object')
      for (const [k, v] of Object.entries(h)) headers.push([k, String(v)]);
    let body = null;
    const b = req.body;
    if (typeof b === 'string') body = b;
    else if (b && b.mode === 'raw') body = String(b.raw ?? '');
    else if (b && b.mode === 'urlencoded')
      body = (b.urlencoded || [])
        .map((p) => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value ?? '')}`)
        .join('&');
    else if (b && typeof b === 'object' && !b.mode) body = JSON.stringify(b);
    if (b && b.mode === 'urlencoded' && !headers.some(([k]) => k.toLowerCase() === 'content-type'))
      headers.push(['Content-Type', 'application/x-www-form-urlencoded']);
    return {
      method: String(req.method || 'GET').toUpperCase(),
      url: typeof req.url === 'string' ? req.url : String(req.url),
      headers,
      body,
    };
  }

  function sendRequest(req, callback) {
    const promise = __rq_send(JSON.stringify(normalizeRequest(req))).then((text) => {
      const out = JSON.parse(text);
      if (out.error) throw new Error(out.error);
      return makeResponse(out);
    });
    const done = promise.then(
      (res) => {
        if (typeof callback === 'function') callback(null, res);
        return res;
      },
      (err) => {
        if (typeof callback === 'function') callback(err, undefined);
        else throw err;
      },
    );
    state.pending.push(done.catch(() => {}));
    return done;
  }

  // ----- timers ----------------------------------------------------------------------

  let timerId = 0;
  const cancelled = new Set();
  globalThis.setTimeout = (fn, ms, ...args) => {
    const id = ++timerId;
    state.pending.push(
      __rq_sleep(Number(ms) || 0).then(() => {
        if (!cancelled.has(id) && typeof fn === 'function') fn(...args);
      }),
    );
    return id;
  };
  globalThis.clearTimeout = (id) => cancelled.add(id);

  // ----- modules and globals -----------------------------------------------------------

  const builtins = {
    'crypto-js': () => load('crypto-js'),
    lodash: () => load('lodash'),
    chai: () => load('chai'),
    moment: () => load('moment'),
    uuid: () => ({ v4: () => __rq_uuid(), v7: () => __rq_uuid() }),
    atob: () => globalThis.atob,
    btoa: () => globalThis.btoa,
  };
  globalThis.require = (name) => {
    const f = builtins[name];
    if (!f) throw new Error(`module "${name}" is not available in Reqly scripts`);
    return f();
  };
  // Postman's sandbox globals.
  for (const [name, mod] of [
    ['_', 'lodash'],
    ['CryptoJS', 'crypto-js'],
    ['moment', 'moment'],
  ]) {
    Object.defineProperty(globalThis, name, {
      configurable: true,
      get() {
        // Libraries read their own global while loading: replace the getter first.
        Object.defineProperty(globalThis, name, { value: undefined, writable: true, configurable: true });
        const value = load(mod);
        globalThis[name] = value;
        return value;
      },
    });
  }

  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  globalThis.btoa = (input) => {
    const s = String(input);
    let out = '';
    for (let i = 0; i < s.length; i += 3) {
      const a = s.charCodeAt(i),
        b = s.charCodeAt(i + 1),
        c = s.charCodeAt(i + 2);
      if (a > 255 || b > 255 || c > 255) throw new Error('btoa: the string contains characters outside Latin1');
      const n = (a << 16) | ((b || 0) << 8) | (c || 0);
      out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (isNaN(b) ? '=' : B64[(n >> 6) & 63]) + (isNaN(c) ? '=' : B64[n & 63]);
    }
    return out;
  };
  globalThis.atob = (input) => {
    const s = String(input).replace(/[\s=]+/g, '');
    let out = '';
    let bits = 0,
      value = 0;
    for (const ch of s) {
      const i = B64.indexOf(ch);
      if (i < 0) throw new Error('atob: invalid base64');
      value = (value << 6) | i;
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        out += String.fromCharCode((value >> bits) & 255);
      }
    }
    return out;
  };

  // ----- the APIs ---------------------------------------------------------------------

  const request = makeRequest();
  const response = makeResponse(input.response);
  const info = {
    eventName: input.phase === 'pre' ? 'prerequest' : 'test',
    requestName: input.info.requestName,
    requestId: input.info.requestId,
    iteration: input.info.iteration,
    iterationCount: input.info.iterationCount,
  };
  const execution = {
    skipRequest: () => {
      state.skipRequest = true;
    },
    setNextRequest: (name) => {
      state.nextRequest = name === null ? null : String(name);
    },
    location: [input.info.requestName],
  };

  const faker = (path, ...args) => __rq_faker(String(path), JSON.stringify(args));

  const rq = {
    request,
    response,
    info,
    execution,
    variables,
    environment: scopeApi('environment', 'environment'),
    collectionVariables: scopeApi('collection', 'collection'),
    globals: scopeApi('globals', 'globals'),
    iterationData: Object.assign(scopeApi('data', 'data'), { set: undefined, unset: undefined }),
    test,
    expect,
    sendRequest,
    faker,
    require: globalThis.require,
  };
  const pm = Object.assign({}, rq, { cookies: new PropertyList([], () => {}) });
  delete pm.faker;
  const insomnia = Object.assign({}, rq, {
    baseEnvironment: rq.collectionVariables,
  });

  globalThis.rq = rq;
  globalThis.pm = pm;
  globalThis.insomnia = insomnia;
  // Legacy Postman globals used by old scripts.
  globalThis.postman = {
    setEnvironmentVariable: (k, v) => rq.environment.set(k, v),
    getEnvironmentVariable: (k) => rq.environment.get(k),
    clearEnvironmentVariable: (k) => rq.environment.unset(k),
    setGlobalVariable: (k, v) => rq.globals.set(k, v),
    getGlobalVariable: (k) => rq.globals.get(k),
    clearGlobalVariable: (k) => rq.globals.unset(k),
    setNextRequest: execution.setNextRequest,
  };
  if (response) {
    globalThis.responseBody = input.response.body;
    globalThis.responseCode = { code: response.code, name: response.status };
    globalThis.responseTime = response.responseTime;
    globalThis.responseHeaders = response.headers.toObject();
    globalThis.tests = new Proxy(
      {},
      {
        set(_, name, value) {
          state.tests.push({ name: String(name), passed: !!value, error: value ? null : 'false', skipped: false });
          return true;
        },
      },
    );
  }

  globalThis.__settle = async () => {
    // Promises may add more (a request inside a callback, nested timers).
    while (state.pending.length) {
      const batch = state.pending.splice(0);
      await Promise.all(batch);
    }
  };

  globalThis.__collect = () =>
    JSON.stringify({
      request: state.request,
      changes: state.changes,
      tests: state.tests,
      skipRequest: state.skipRequest,
      nextRequest: state.nextRequest,
    });
})();
