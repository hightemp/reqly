# Scripts

Requests, folders and collections can run JavaScript **before** a request is
sent (pre-request) and **after** its response arrives (after-response). Edit
them in the **Scripts** tab, or **Collection scripts…** in the collection
menu. Scripts run from the collection inwards to the request.

## Examples

```js
// Pre-request: a fresh timestamp and a signature
rq.variables.set('ts', Date.now());
rq.request.headers.upsert({ key: 'X-Sign', value: CryptoJS.SHA256('secret').toString() });

// After-response: keep the token and check the answer
const body = rq.response.json();
rq.environment.set('token', body.token);
rq.test('status is 200', () => rq.response.to.have.status(200));
rq.test('has items', () => rq.expect(body.items).to.not.be.empty);
```

## The API

|                                                          |                                                       |
| -------------------------------------------------------- | ----------------------------------------------------- |
| `rq.request`                                             | URL, method, headers and body of the request          |
| `rq.response`                                            | `code`, `headers`, `json()`, `text()`, `responseTime` |
| `rq.variables`                                           | read any variable; `set` makes a runtime variable     |
| `rq.environment`, `rq.collectionVariables`, `rq.globals` | change environments                                   |
| `rq.test(name, fn)`, `rq.expect(value)`                  | tests with chai assertions                            |
| `rq.sendRequest(request)`                                | send another request                                  |
| `rq.execution.skipRequest()`                             | do not send this request                              |
| `rq.faker`                                               | fake data from faker-js                               |
| `console.log(…)`                                         | output in the response's **Console** tab              |

`pm` (Postman) and `insomnia` are the same API, so imported scripts run
unchanged. `require()` gives `crypto-js`, `lodash`, `chai`, `moment`, `uuid`
and `@faker-js/faker`. `await` works at the top level.

## Runtime variables

`rq.variables.set` creates variables that apply to the collection until the
app restarts and override every other level. See and clear them in the
**Environments** window under **Runtime (scripts)**.

## Limits

Scripts have no access to files or the network (except `sendRequest`). A
script that runs longer than 5 seconds or uses more than 64 MB is stopped. A
pre-request error stops the request; the error is in the **Console**.
