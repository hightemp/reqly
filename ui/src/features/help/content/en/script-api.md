# Script API

The global `rq` object is the whole API of a script. `pm` (Postman) and
`insomnia` (Insomnia) are the same object under other names. How scripts
run is explained in [Scripts](help:scripts).

| Member                                                   | What it is                                         |
| -------------------------------------------------------- | -------------------------------------------------- |
| `rq.request`                                             | the request being sent: URL, method, headers, body |
| `rq.response`                                            | the response (after-response scripts only)         |
| `rq.variables`                                           | variables of every level; runtime variables        |
| `rq.environment`, `rq.collectionVariables`, `rq.globals` | environments                                       |
| `rq.iterationData`                                       | the runner's data row                              |
| `rq.test()`, `rq.expect()`                               | tests                                              |
| `rq.sendRequest()`                                       | send another request                               |
| `rq.info`                                                | which request and iteration is running             |
| `rq.execution`                                           | skip the request, change the runner's order        |
| `rq.faker`                                               | fake data                                          |
| `console`, `setTimeout`, `atob`, `btoa`, `require`       | globals                                            |

## rq.request

The request as it will be sent. In a pre-request script you can change it;
in an after-response script it shows what was sent. Templates are not
rendered yet: `{{base_url}}` stays as written.

| Property / method      | Description                                                                  |
| ---------------------- | ---------------------------------------------------------------------------- |
| `id`, `name`           | the request's id and name (read-only)                                        |
| `method`               | the HTTP method; assigning a value changes it (upper-cased)                  |
| `url`                  | the URL object, see below; assigning a string replaces the URL and the query |
| `headers`              | the headers, a [property list](help:script-api#property-lists)               |
| `body`                 | the body, see below                                                          |
| `auth`                 | the auth settings, read-only (`auth.type`, …)                                |
| `addHeader(header)`    | same as `headers.add(header)`                                                |
| `upsertHeader(header)` | same as `headers.upsert(header)`                                             |
| `removeHeader(name)`   | same as `headers.remove(name)`                                               |
| `toJSON()`             | `{ method, url, headers, body }`                                             |

```js
console.log(rq.request.name, rq.request.method, rq.request.url.toString());

rq.request.method = 'put'; // becomes PUT
rq.request.upsertHeader({ key: 'X-Trace', value: 'on' });
rq.request.removeHeader('Accept');
```

### rq.request.url

The URL without its query string, plus the query as a list. Disabled query
parameters are left out.

| Method                    | Returns       | Description                                                   |
| ------------------------- | ------------- | ------------------------------------------------------------- |
| `toString()`              | string        | the full URL with the enabled query parameters                |
| `update(url)`             | —             | replaces the URL; its query string becomes the parameter list |
| `query`                   | property list | the query parameters                                          |
| `getHost()`               | string        | the host: `api.example.com`                                   |
| `getPath()`               | string        | the path: `/orders` (`/` when empty)                          |
| `getQueryString()`        | string        | `page=1&lang=en`                                              |
| `addQueryParams(params)`  | —             | adds `'a=1&b=2'`, `{ key, value }` or an array of them        |
| `removeQueryParams(keys)` | —             | removes parameters by name: `'a'` or `['a', 'b']`             |

```js
const url = rq.request.url;
url.addQueryParams('limit=50&sort=name');
url.removeQueryParams('page');
url.query.upsert({ key: 'lang', value: 'en' });
console.log(url.getPath(), url.getQueryString());

// Replace the whole URL (the query string becomes parameters)
rq.request.url = 'https://api.example.com/v2/orders?page=2';
console.log(rq.request.url.toString());
```

### rq.request.body

| Member                   | Description                                                                                                             |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `mode`                   | `none`, `raw` (JSON, XML, text…), `urlencoded`, `formdata` (multipart), `file` or `graphql`; assignable                 |
| `raw`                    | the body text (for `graphql`, the query); assignable                                                                    |
| `urlencoded`, `formdata` | the form fields, a property list                                                                                        |
| `update(body)`           | a string replaces the text; `{ mode, raw }` or `{ mode: 'urlencoded', urlencoded: [{ key, value }] }` replaces the body |
| `isEmpty()`              | true for no body or empty text                                                                                          |
| `toString()`             | the body text                                                                                                           |

Setting `raw` keeps the body type (JSON stays JSON). When the body had no
text type, a JSON object or array becomes a JSON body, anything else text.

```js
// Read, change and write back a JSON body
const order = JSON.parse(rq.request.body.raw);
order.qty = 5;
order.note = `created at ${new Date().toISOString()}`;
rq.request.body.raw = JSON.stringify(order, null, 2);

// Turn it into a form instead
rq.request.body.update({
  mode: 'urlencoded',
  urlencoded: [
    { key: 'item', value: '1' },
    { key: 'qty', value: '5' },
  ],
});
console.log(rq.request.body.mode, rq.request.body.urlencoded.toObject());
```

## rq.response

The response, in after-response scripts (`undefined` before sending).

| Member            | Type          | Description                                         |
| ----------------- | ------------- | --------------------------------------------------- |
| `code`            | number        | the status code: `200`                              |
| `status`          | string        | the status text: `OK`                               |
| `reason()`        | string        | same as `status`                                    |
| `headers`         | property list | the response headers (names are case-insensitive)   |
| `responseTime`    | number        | the total time in milliseconds                      |
| `responseSize`    | number        | the body size in bytes                              |
| `text()`          | string        | the body as text                                    |
| `json()`          | any           | the body parsed as JSON; throws when it is not JSON |
| `body`            | string        | the body as text (Insomnia)                         |
| `getStatusCode()` | number        | same as `code` (Insomnia)                           |
| `getHeader(name)` | string        | a header value (Insomnia)                           |
| `to`              |               | assertions, see below                               |

```js
console.log(rq.response.code, rq.response.status, `${rq.response.responseTime} ms`);
console.log(rq.response.headers.get('content-type'));

const body = rq.response.json();
console.log(body.items.map((i) => i.name).join(', '));
```

### Response assertions

`rq.response.to…` throws when the check fails, so use it inside
`rq.test()`. Properties such as `to.be.ok` check when they are read.

| Assertion                                               | Passes when                                  |
| ------------------------------------------------------- | -------------------------------------------- |
| `to.have.status(200)`, `to.have.status('OK')`           | the status code or text matches              |
| `to.have.header('X-Id')`, `to.have.header('X-Id', '7')` | the header exists (with this value)          |
| `to.have.body()`, `to.have.body('text')`                | there is a body (equal to the text)          |
| `to.have.jsonBody()`                                    | the body is JSON                             |
| `to.have.jsonBody('user.name')`                         | the JSON has this path (lodash path syntax)  |
| `to.have.jsonBody('user.name', 'Ada')`                  | the value at the path equals (deep equality) |
| `to.be.ok`, `to.be.success`                             | the status is 2xx                            |
| `to.be.json`                                            | the body is JSON                             |
| `to.be.notFound`                                        | the status is 404                            |
| `to.be.clientError`, `to.be.serverError`                | the status is 4xx / 5xx                      |
| `to.not.have.status(500)`                               | the status is different                      |
| `to.not.have.header('X-Debug')`                         | the header is missing                        |
| `to.not.be.error`                                       | the status is below 400                      |

```js
rq.test('the response is fine', () => {
  rq.response.to.have.status(200);
  rq.response.to.be.ok;
  rq.response.to.be.json;
  rq.response.to.have.header('X-Request-Id');
  rq.response.to.have.jsonBody('user.name', 'Ada');
  rq.response.to.not.have.header('X-Debug');
});
```

## Property lists

Headers, query parameters and form fields share one interface. An item is
`{ key, value, disabled }`. Names are matched case-insensitively; disabled
items are ignored.

| Method                              | Description                                                                    |
| ----------------------------------- | ------------------------------------------------------------------------------ |
| `get(key)`                          | the value, or `undefined`                                                      |
| `has(key)`, `has(key, value)`       | whether the item exists (with this value)                                      |
| `one(key)`                          | the item `{ key, value, disabled }`                                            |
| `all()`                             | the enabled items                                                              |
| `count()`                           | the number of enabled items                                                    |
| `add(item)`                         | appends `{ key, value }` or `'Key: value'` (`'key=value'` for query and forms) |
| `append(item)`                      | same as `add`                                                                  |
| `upsert(item)`                      | changes the value of an existing item or adds a new one                        |
| `remove(key)`, `remove(fn)`         | removes items by name or by a predicate                                        |
| `clear()`                           | removes everything                                                             |
| `each(fn)`, `map(fn)`, `filter(fn)` | iterate over the enabled items                                                 |
| `toObject()`                        | `{ key: value }`                                                               |
| `toString()`                        | `Key: value` lines                                                             |

```js
const h = rq.request.headers;
h.add('X-One: 1');
h.add({ key: 'X-Two', value: '2' });
h.upsert({ key: 'Accept', value: 'application/xml' });
h.remove((item) => item.key.startsWith('X-T'));

console.log(h.get('accept'), h.has('X-One'), h.count());
h.each((item) => console.log(item.key, '=', item.value));
console.log(h.toObject());
```

## Variables

`rq.variables`, `rq.environment`, `rq.collectionVariables`, `rq.globals` and
`rq.iterationData` have the same methods:

| Method            | Description                                              |
| ----------------- | -------------------------------------------------------- |
| `get(key)`        | the value, or `undefined`                                |
| `has(key)`        | whether the key is set                                   |
| `set(key, value)` | sets a value: a string, number, boolean, array or object |
| `unset(key)`      | removes the key                                          |
| `clear()`         | removes every key of this scope                          |
| `toObject()`      | all keys and values                                      |
| `replaceIn(text)` | replaces `{{name}}` and dynamic variables in the text    |

The scopes:

- `rq.variables.get` looks through every level and returns the nearest
  value: runtime variables, the runner's data row, the environment, the
  collection, the globals. `rq.variables.set` creates a **runtime variable**:
  it overrides every other level for all requests of the collection until
  the app restarts.
- `rq.environment` is the active environment of the collection (the base
  environment when none is active).
- `rq.collectionVariables` is the collection's base environment.
- `rq.globals` is the active global environment of the workspace (created
  when needed).
- `rq.iterationData` is the current row of the runner's data file
  (read-only).

```js
rq.environment.set('lastOrderAt', new Date().toISOString());
rq.collectionVariables.set('limits', { perPage: 50, max: 500 });
rq.globals.set('runs', (rq.globals.get('runs') || 0) + 1);

console.log(rq.variables.get('base_url')); // from any level
console.log(rq.variables.has('token'));
console.log(rq.variables.replaceIn('{{base_url}}/users/{{$randomInt}}'));
console.log(Object.keys(rq.environment.toObject()));

rq.environment.unset('lastOrderAt');
```

`replaceIn` replaces variables and dynamic variables (`{{$guid}}`,
`{{$timestamp}}`, `{{$faker.person.firstName}}`…); template tags
(`{% … %}`) are left as they are.

## Tests

### rq.test(name, fn)

Registers a test. When `fn` throws, the test fails with the error message;
otherwise it passes. `fn` may be `async`. Results appear in the **Tests** tab
of the response, in the runner and in reports. `rq.test.skip(name)` records
a skipped test.

### rq.expect(value)

[chai](help:script-libraries#chai) `expect`: `rq.expect(x).to.equal(1)`,
`.to.eql({…})`, `.to.include('a')`, `.to.have.property('id')`,
`.to.be.above(3)`, `.to.match(/re/)`…

```js
const body = rq.response.json();

rq.test('status is 200', () => rq.response.to.have.status(200));

rq.test('user', () => {
  rq.expect(body.user).to.include({ name: 'Ada' });
  rq.expect(body.user.roles).to.include('admin');
  rq.expect(body.user.email).to.match(/@example\.com$/);
});

rq.test('fast enough', () => {
  rq.expect(rq.response.responseTime).to.be.below(1000);
});

rq.test.skip('not ready yet');
```

## rq.sendRequest(request, callback?)

Sends another request from a script and returns a promise of its response
(the same object as `rq.response`). The optional callback gets
`(error, response)`.

`request` is a URL string or an object:

| Field                 | Description                                                                                                               |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `url`                 | the address                                                                                                               |
| `method`              | `GET` by default                                                                                                          |
| `header` or `headers` | `{ name: value }` or `[{ key, value }]`                                                                                   |
| `body`                | a string, `{ mode: 'raw', raw }`, `{ mode: 'urlencoded', urlencoded: [{ key, value }] }` or a plain object (sent as JSON) |

The request is sent as it is: `{{templates}}` are not rendered, so build the
URL with `rq.variables.get()` or `rq.variables.replaceIn()`. Set the
`Content-Type` header yourself for JSON. Network failures reject the promise
(or pass the error to the callback); HTTP error statuses do not.

```js
const base = rq.variables.get('base_url');

// GET with await
const health = await rq.sendRequest(`${base}/health`);
console.log('health', health.code);

// POST JSON
const login = await rq.sendRequest({
  url: `${base}/auth/token`,
  method: 'POST',
  header: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    client_id: rq.environment.get('client_id'),
    client_secret: rq.environment.get('client_secret'),
  }),
});
rq.environment.set('token', login.json().access_token);

// Postman style, with a callback
rq.sendRequest(`${base}/version`, (err, res) => {
  if (err) console.error(err);
  else console.log('version', res.code);
});
```

## rq.info

| Property         | Description                             |
| ---------------- | --------------------------------------- |
| `requestName`    | the name of the request                 |
| `requestId`      | its id                                  |
| `eventName`      | `prerequest` or `test` (after-response) |
| `iteration`      | the runner's iteration, from 0          |
| `iterationCount` | the number of iterations                |

```js
console.log(
  `${rq.info.eventName}: ${rq.info.requestName} (#${rq.info.iteration + 1} of ${rq.info.iterationCount})`,
);
```

## rq.execution

| Method                 | Description                                                            |
| ---------------------- | ---------------------------------------------------------------------- |
| `skipRequest()`        | pre-request: do not send the request; the remaining scripts still run  |
| `setNextRequest(name)` | runner: run the request with this name next; `null` ends the iteration |

```js
// Skip the request when there is nothing to send
if (!rq.variables.get('orderId')) {
  console.warn('no orderId yet, skipping');
  rq.execution.skipRequest();
}
```

## rq.faker

[faker-js](help:script-libraries#faker-js) in the data locale from
**Settings → Templates**: `rq.faker.person.firstName()`, or by name:
`rq.faker('number.int', { min: 1, max: 10 })`. The global `faker` is the
same. (`pm.faker` does not exist in Postman; use `faker` there.)

```js
rq.variables.set('name', rq.faker.person.fullName());
rq.variables.set('qty', rq.faker('number.int', { min: 1, max: 10 }));
```

## Globals

| Global                                                 | Description                                                |
| ------------------------------------------------------ | ---------------------------------------------------------- |
| `console.log/info/warn/error/debug`                    | output in the **Console** tab; objects are printed as JSON |
| `setTimeout(fn, ms)`, `clearTimeout(id)`               | timers; the script waits for them                          |
| `atob(text)`, `btoa(text)`                             | Base64 decode / encode (Latin-1 text)                      |
| `crypto.getRandomValues(array)`, `crypto.randomUUID()` | secure random bytes and UUIDs (Web Crypto)                 |
| `require(name)`                                        | a [bundled library](help:script-libraries)                 |
| `_`, `CryptoJS`, `moment`, `faker`                     | lodash, crypto-js, moment and faker-js, already loaded     |

```js
console.info('object:', { a: 1, list: [1, 2] });
console.warn('a warning');
console.log(btoa('user:password'), atob('dXNlcjpwYXNzd29yZA=='));
setTimeout(() => console.log('later'), 50);
```

## Postman and Insomnia names

| Name                                                                                   | Notes                                                                    |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `pm.*`                                                                                 | everything above                                                         |
| `insomnia.*`                                                                           | everything above; `insomnia.baseEnvironment` is `rq.collectionVariables` |
| `tests['name'] = condition`                                                            | a test (old Postman scripts)                                             |
| `responseBody`, `responseCode.code`, `responseTime`, `responseHeaders`                 | the response (old Postman scripts)                                       |
| `postman.setEnvironmentVariable`, `getEnvironmentVariable`, `clearEnvironmentVariable` | `rq.environment`                                                         |
| `postman.setGlobalVariable`, `getGlobalVariable`, `clearGlobalVariable`                | `rq.globals`                                                             |
| `postman.setNextRequest(name)`                                                         | `rq.execution.setNextRequest`                                            |

```js
// An old-style Postman test script
var data = JSON.parse(responseBody);
tests['status code is 200'] = responseCode.code === 200;
tests['has a token'] = !!data.token;
postman.setEnvironmentVariable('token', data.token);
```
