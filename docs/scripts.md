# Scripts

Requests, folders and collections can run JavaScript before a request is
sent (**pre-request**) and after its response arrives (**after-response**).
Edit them in the request's or folder's **Scripts** tab, or under
**Collection scripts…** in the collection menu.

## Order

Before sending: the collection script, then folder scripts from the
outermost folder inwards, then the request's script. After the response the
same order applies. Each script sees the changes made by the ones before it.

Pre-request scripts work on the request **before** templates are rendered:
`{{ variables }}` still appear as written, and variables a script sets are
used when the request is rendered right after.

## API

`rq` is Reqly's API. `pm` (Postman) and `insomnia` (Insomnia) are the same
API under the names those tools use, so imported scripts run unchanged
within what is supported.

| | |
| --- | --- |
| `rq.request` | `url` (`toString()`, `update()`, `query`), `method`, `headers` (`get`, `add`, `upsert`, `remove`, `toObject`…), `body` (`mode`, `raw`, `urlencoded`, `formdata`, `update()`) |
| `rq.response` | `code`, `status`, `headers`, `responseTime`, `responseSize`, `json()`, `text()`, `to.have.status()`, `to.have.header()`, `to.have.jsonBody()`, `to.be.ok`… |
| `rq.variables` | Reads every level (runtime, iteration data, environment, collection, global); `set` writes a runtime variable |
| `rq.environment` | The active environment (the base environment when none is active) |
| `rq.collectionVariables` | The collection's base environment |
| `rq.globals` | The active global environment (one is created when needed) |
| `rq.test(name, fn)`, `rq.expect(value)` | Tests with chai assertions; results appear in the response's **Tests** tab |
| `rq.sendRequest(request, callback?)` | Sends another request; returns a promise and calls the callback (`(err, res)`) if given |
| `rq.info` | `requestName`, `requestId`, `eventName`, `iteration` |
| `rq.execution.skipRequest()` | Pre-request: do not send the request |
| `rq.faker(path)` | Fake data, e.g. `rq.faker('person.firstName')` |
| `console.log/info/warn/error/debug` | Output in the response's **Console** tab |

`await` works at the top level of a script. `setTimeout` works; the script
ends when its timers and requests have finished.

Legacy Postman globals are supported too: `postman.setEnvironmentVariable`,
`tests["name"] = condition`, `responseBody`, `responseCode`.

## Modules

`require()` gives `crypto-js`, `lodash`, `chai`, `moment` and `uuid`.
`CryptoJS`, `_` and `moment` are also globals, as in Postman, and `atob` and
`btoa` are available. Other modules (`cheerio`, `xml2js`, Node modules) are
not; imports report scripts that use them.

## Variables set by scripts

`rq.variables.set` (`pm.variables.set`) creates **runtime variables**: they
apply to the collection's requests until the app restarts and override every
other level. See and clear them in the environment manager under
**Runtime (scripts)**. `rq.environment.set`, `rq.collectionVariables.set` and
`rq.globals.set` change the environments themselves.

## Sandbox

Scripts run in an embedded QuickJS engine (ADR-006) without access to files,
the network (except `sendRequest`) or the system. A script that runs longer
than 5 seconds or uses more than 64 MB is stopped; time spent waiting for
`sendRequest` and timers does not count. The limits can be changed with the
`scripts.settings` setting (`timeoutMs`, `memoryMb`).

A pre-request script error stops the request; the error and the console
output are kept in the history. After-response script errors appear in the
Console; the response is kept.

## Imported scripts

Postman and Insomnia scripts (requests, folders and collections) run as
they are; the import report lists APIs Reqly does not provide (`pm.vault`,
`pm.visualizer`, `cheerio`…). Bruno scripts use Bruno's own API (`bru`,
`req`, `res`), so they are imported commented out.
