# Import, export and code generation

## Import

**Workspace → Import…** (or **Collection → Import into this collection…**, or
the command palette). Sources: files, a folder, drag and drop, pasted text or
a URL. The format is detected automatically and can be chosen by hand. Before
importing, the wizard shows the tree, the number of requests, folders and
environments, and a **report** of everything that cannot be imported exactly.
The report can be copied or saved. Nothing is dropped silently.

| Format | What is imported |
| --- | --- |
| Reqly (JSON / YAML) | everything: collections, environments, secrets (if exported), cookies, history |
| Insomnia v3/v4 (JSON / YAML), v5 (YAML) | workspaces, folders (headers, auth, variables, scripts), requests, every body type, path parameters, settings, environments and sub-environments, global environments, cookie jar, template tags |
| Postman Collection v2.0 / v2.1 | folders, requests, every body mode, auth, collection variables (`secret` type), path variables, `protocolProfileBehavior`, descriptions, scripts |
| Postman Environment, Globals | environments; can be imported together with the collection file |
| OpenAPI 3.0 / 3.1, Swagger 2.0 (JSON / YAML) | tags → folders, `servers` and server variables → environment variables (`base_url`), parameters with examples, bodies from examples or schemas, security schemes → auth with variables |
| HAR 1.2 | requests (a log, a single entry or a single request) |
| cURL | one or several commands; lines that are not curl commands are reported |
| Bruno | a collection folder or a zip (`bruno.json`, `collection.bru`, `folder.bru`, `environments/`), single `.bru` files |

### Where to import

- **New collection**: one collection per imported collection.
- **Into an existing collection**:
  - *Merge*: folders and requests with the same name are updated, the rest
    is added;
  - *Add alongside*: everything is added, existing items stay as they are;
  - *Replace*: existing items go to the trash first (they can be restored).

Collection environments become environments of the collection, and collection
variables go to its base environment. Standalone environments (a Postman
environment file, Insomnia or Reqly global environments) become global
environments of the workspace when importing into a new collection, and
environments of the target collection when importing into an existing one.

References between requests (`{% response 'body', '<id>', … %}`) are
rewritten to the new requests.

### What goes into the report

| Situation | What Reqly does |
| --- | --- |
| Scripts (pre-request, tests, after-response) | kept in the request, not run until stage 6 |
| Auth types that are not supported yet (Digest, OAuth 1/2, AWS, NTLM, Hawk…) | the request is imported without auth; the original parameters are in the report |
| WebSocket, gRPC, Socket.IO, MCP | skipped; the data is in the report |
| GraphQL bodies | imported as JSON `{ "query", "variables" }` |
| Collection-level auth and headers (Postman, Bruno) | applied to the top-level items |
| Collection-level scripts | listed in the report |
| Unknown template tags (Insomnia plugins) | kept in the text and reported |
| Postman variables with dots or spaces (`{{api.host}}`) | become `{{ _['api.host'] }}` |
| Saved Postman example responses, responses in HAR | not imported (reported) |
| Bruno secrets | keys are created empty: Bruno keeps the values outside the collection |

## Export

**Workspace → Export workspace…**, **Collection → Export collection…** or
**Export…** in the context menu of selected folders and requests.

| Format | What is saved |
| --- | --- |
| Reqly (JSON / YAML) | everything; optionally secret values (with a warning), cookies and response history |
| Insomnia v4 (JSON) | collections, environments, cookies; Reqly's `{{ $fn() }}` syntax becomes tags |
| Insomnia v5 (YAML) | one collection per file |
| Postman v2.1 | the collection plus one file per environment |
| HAR 1.2 | requests with folder headers applied (no responses) |
| cURL | a command for every request |

By default secret values are not exported: the keys stay, the values are
empty. Insomnia and Postman have no request-level variables or folder network
settings, so those are lost when exporting to them.

The Reqly format is specified in [`docs/format/reqly.schema.json`](format/reqly.schema.json).
`formatVersion` is a semantic version: older files are upgraded on import,
files from a newer major version are rejected.

## Code generation

Use the `{ }` button next to **Send**, **Copy as → …** in a request's context
menu, or the command palette. Variables are resolved by default; templates
can be kept as they are. The chosen language is remembered.

| Group | Targets |
| --- | --- |
| Shell | cURL (bash, cmd, PowerShell), Wget, HTTPie, PowerShell `Invoke-RestMethod` |
| JavaScript | fetch, Axios, XMLHttpRequest, Node.js `http`, undici |
| Python | Requests, HTTPX, `http.client` |
| Go, Rust | `net/http`, reqwest |
| Java, Kotlin | OkHttp, `HttpClient`; Kotlin OkHttp |
| C# | `HttpClient`, RestSharp |
| PHP, Ruby | cURL, Guzzle; `Net::HTTP` |
| Swift, Dart | `URLSession`, `http` |

Limitations: Wget cannot send multipart bodies (the snippet suggests curl),
and the browser targets (`fetch`, `XMLHttpRequest`) take files from a file
input.
