# Scripts

Scripts are JavaScript that runs around a request: a **pre-request** script
before it is sent, an **after-response** script when the response arrives.
Use them to compute values (timestamps, signatures, tokens), to change the
request on the fly, to save data from responses into variables and to test
responses.

This page explains how scripts work. The full list of objects and methods is
in [Script API](help:script-api); examples with every bundled library and
ready-made recipes are in [Libraries and recipes](help:script-libraries).

## Where scripts live

- **Request**: the **Scripts** tab of the request, with two editors —
  pre-request and after-response.
- **Folder**: **Folder settings… → Scripts**. Runs for every request inside
  the folder, including nested folders.
- **Collection**: **Collection scripts…** in the collection menu. Runs for
  every request of the collection.

The editor completes the API (`rq.`, `pm.`), variable names and faker-js
functions.

## Order of execution

For one request, scripts run from the outside in:

1. collection pre-request script;
2. folder pre-request scripts, from the outermost folder inwards;
3. the request's pre-request script;
4. _the request is rendered (templates are replaced) and sent_;
5. collection after-response script;
6. folder after-response scripts, outermost first;
7. the request's after-response script.

Each script sees the changes made by the scripts before it: a variable set
by the collection script can be read by the request script.

## Pre-request scripts

A pre-request script works on the request **before** templates are
rendered: `rq.request.url` returns `{{base_url}}/orders` as you wrote it.
Variables the script sets are used when the request is rendered right
after, so this is the place to prepare values for `{{ templates }}`.

```js
// A fresh timestamp and a request id for {{ts}} and {{requestId}}
rq.variables.set('ts', Date.now());
rq.variables.set('requestId', require('uuid').v4());

// Change the request directly
rq.request.headers.upsert({ key: 'X-Client', value: 'reqly' });
rq.request.url.query.upsert({ key: 'lang', value: 'en' });
```

## After-response scripts

An after-response script reads the response, saves what later requests need
and checks the result. Test results appear in the response's **Tests** tab,
`console.log` output in the **Console** tab.

```js
const body = rq.response.json();

// Save the token for the next requests: {{token}}
rq.environment.set('token', body.token);

// Tests
rq.test('status is 200', () => {
  rq.response.to.have.status(200);
});
rq.test('returns three items', () => {
  rq.expect(body.items).to.have.lengthOf(3);
});

console.log('user', body.user.name);
```

## Variables in scripts

| Object                   | Reads                                     | Writes to                                                 |
| ------------------------ | ----------------------------------------- | --------------------------------------------------------- |
| `rq.variables`           | every level, the nearest wins             | runtime variables (see below)                             |
| `rq.environment`         | the active environment                    | the active environment (the base one when none is active) |
| `rq.collectionVariables` | the collection's base environment         | the base environment                                      |
| `rq.globals`             | the active global environment             | the global environment                                    |
| `rq.iterationData`       | the current row of the runner's data file | —                                                         |

**Runtime variables** (`rq.variables.set`) apply to every request of the
collection until the app restarts and override all other levels. They are
handy for values that should not be saved. See and clear them in the
**Environments** window under **Runtime (scripts)**.

Values may be strings, numbers, booleans, arrays or objects. In templates,
objects and arrays become JSON.

## Asynchronous code

`await` works at the top level of a script. The script ends when all its
timers and requests have finished.

```js
const res = await rq.sendRequest(rq.variables.replaceIn('{{base_url}}/health'));
console.log('health', res.code);

await new Promise((resolve) => setTimeout(resolve, 100));
console.log('100 ms later');
```

## Errors and limits

- A **pre-request** error stops the request: it is not sent, and the error
  with the console output is shown and kept in the history.
- An **after-response** error appears in the **Console**; the response is
  kept.
- A script that runs longer than **5 seconds** or uses more than **64 MB**
  is stopped. Waiting for `sendRequest` and timers does not count.
- Scripts run in a sandbox (QuickJS): no files, no network except
  `sendRequest`, no access to the system. `require()` gives only the
  [bundled libraries](help:script-libraries).

## Postman and Insomnia scripts

`pm` and `insomnia` are the same API as `rq`, so scripts imported from
Postman and Insomnia run unchanged. Old Postman globals work too:
`tests['name'] = condition`, `responseBody`, `responseCode`,
`postman.setEnvironmentVariable`. Bruno scripts use another API and are
imported commented out.

```js
// A Postman script runs as it is
pm.test('Status code is 200', function () {
  pm.response.to.have.status(200);
});
const jsonData = pm.response.json();
pm.environment.set('userId', jsonData.user.id);
```
