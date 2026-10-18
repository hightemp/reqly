# Templates, variables and environments

Templates work in the URL, query and path parameters, headers, the body (text
and form fields), auth fields, proxy settings and in variable values. The Rust
core renders them when sending, so desktop and server give the same result.
History stores the rendered request.

## Syntax

| Syntax | Meaning |
| --- | --- |
| `{{ name }}` | a variable |
| `{{ api.version }}`, `{{ hosts[0] }}`, `{{ _['odd key'] }}` | a path inside a JSON value |
| `{{ _.name }}` | same as `{{ name }}` (Insomnia compatibility) |
| `{% uuid 'v4' %}` | an Insomnia-style tag (function) |
| `{{ $uuid('v7') }}` | the same function as a call |
| `{{ $faker.person.firstName }}` | fake data |
| `{{ $guid }}`, `{{ $randomEmail }}` | Postman dynamic variables |
| `{# comment #}` | not rendered |
| `\{{` | a literal `{{` |

Arguments are strings in single or double quotes (`\'` escapes), numbers,
`true` / `false` / `null`, variables and nested calls:
`{% hash 'sha256', 'hex', token %}`, `{% base64 'encode', 'normal', $now('unix') %}`.

Variable values may contain templates too:
`base_url = {{ host }}/api/{{ version }}`. Cycles (`a → b → a`) fail with
`template.cycle`. An undefined variable stops the send with
`template.undefined`; the editor highlights it in red.

A call yields one value per send: `{{ $uuid }}` in the URL and in a header is
the same UUID.

## Where variables come from

Later levels override earlier ones:

1. the active global environment of the workspace;
2. the collection's base environment;
3. the collection's active environment (dev, stage, prod…);
4. folder variables, from the outermost folder inwards;
5. request variables.

Overriding works on top-level keys. Environments are edited in the
**Environments** window (`Ctrl+E`) as a table or as JSON. The active
environment is switched in the sidebar header. Folder variables are set in
the folder settings, request variables on the **Variables** tab.

### Secrets

An environment key can be marked as secret (the lock in its row):

- the value is masked in the table and in the hover preview;
- the database stores it encrypted (XChaCha20-Poly1305) with a key in
  `secret.key` next to the database (readable by the owner only);
- without `secret.key` secrets cannot be decrypted: copy it together with
  `reqly.db` when moving data;
- exports leave secret values empty unless you choose to include them.

## Functions

| Function | Arguments | Example |
| --- | --- | --- |
| `uuid` | `v4` (default), `v7` | `{% uuid 'v7' %}` |
| `now` | `iso-8601`, `unix`, `millis`, `rfc-2822`, `custom` + strftime format | `{% now 'custom', '%Y-%m-%d' %}` |
| `random` | `int` min max, `float` min max, `string` length [alphabet], `regex` pattern | `{% random 'regex', '[A-Z]{3}-\d{4}' %}` |
| `base64` | `encode`/`decode`, [`normal`/`url`], value (Insomnia's order) | `{% base64 'encode', 'normal', 'user:pass' %}` |
| `urlencode` | `encode`/`decode`, value | `{% urlencode 'encode', 'a b' %}` |
| `hash` | `md5`/`sha1`/`sha256`/`sha384`/`sha512`, `hex`/`base64`/`base64url`, value | `{% hash 'sha256', 'hex', body %}` |
| `hmac` | algorithm, encoding, key, value | `{% hmac 'sha256', 'hex', secret, payload %}` |
| `jsonpath` | JSON (a value or a string), expression | `{% jsonpath user, '$.roles[0]' %}` |
| `jwt` | `encode` payload key [algorithm]; `decode` token [`header`/`payload`] | `{% jwt 'encode', '{"sub":"1"}', secret, 'HS256' %}` |
| `faker` | function name, arguments | `{% faker 'number.int', { min: 1, max: 10 } %}` |
| `cookie` | URL, name | `{% cookie 'https://api.example.com', 'sid' %}` |
| `request` | `url`, `method`, `name`, `id`, `folder`, `header` name | `{% request 'header', 'Accept' %}` |
| `response` | field, request id, path, trigger, maxAge (s) | see below |
| `prompt` | title, label, default value, key | `{% prompt 'OTP', 'Code from the SMS' %}` |
| `file` | path (desktop) or the hash of an uploaded file (server) | `{% file '/tmp/token.txt' %}` |
| `os` | `platform`, `arch`, `hostname`, `username`, `env` name | `{% os 'env', 'HOME' %}` |

Hover a tag to see its value, and click **Edit…** (or `Ctrl`+click) to change
its arguments in a form with a live preview.

### Response chaining

`{% response 'body', '<request id>', '$.token', 'no-history', 300 %}`

- field: `body` (a `$…` path is JSONPath, `/…` is XPath, empty is the whole
  body), `header` (the path is the header name), `raw`, `url`, `status`;
- trigger: `never` uses the latest response; `no-history` sends when there is
  no response; `when-expired` sends when the latest response is older than
  maxAge seconds; `always` sends every time;
- requests that depend on each other fail with `template.response_cycle`.

### prompt

Before sending, Reqly collects all `prompt` tags of the request, its folders
and the variables and shows one window with all fields. Values can be
remembered for the session.

### Server restrictions

On the server, `file` only reads files uploaded through the UI, and
`os 'hostname'`, `os 'username'` and `os 'env'` are not available: templates
run on the server and must not reveal its environment.

## Fake data

Fake data comes from [faker-js](https://fakerjs.dev) (bundled, version in
`crates/reqly-faker/vendor/catalog.json`), so every faker-js function and
all of its locales are available: `{{ $faker.<module>.<method>(args) }}`.

```text
{{ $faker.person.firstName }}
{{ $faker.internet.email({ provider: 'example.com' }) }}
{{ $faker.number.int({ min: 1, max: 100 }) }}
{{ $faker.helpers.arrayElement(['new', 'paid', 'shipped']) }}
{{ $faker.helpers.fake('{{person.lastName}}-{{string.numeric(4)}}') }}
{{ $faker.date.between({ from: '2026-01-01', to: '2026-12-31' }) }}
{% faker 'string.alpha', { length: 8, casing: 'upper' } %}
```

Arguments are those of faker-js: strings, numbers, booleans, `null`,
variables, arrays (`[a, 'b']`) and objects (`{ min: 1, 'max': n }`). Dates
render as ISO 8601 strings; arrays and objects as JSON. The editor completes
the function names and shows their parameters and a sample value.

For the most common options there are positional shortcuts:
`number.int(1, 100)`, `number.float(0, 1, 2)`, `number.bigInt(1, 9)`,
`date.between(from, to)`, `finance.amount(min, max, dec, symbol)`,
`internet.password(length)` and `image.url(width, height)`.

The locale (**Settings → Templates**) applies to templates and scripts and
can be overridden per collection: `en`, `ru`, `de`, `de_AT`, `fr`, `ja`,
`pt_BR`, `uk`, `zh_CN` and the other faker-js locales. A locale falls back
to English for data it does not have, as in faker-js.

A call may run for at most 5 seconds; `{{ $faker.lorem.words(1000000000) }}`
fails with *faker ran too long* instead of blocking sends.

### Postman dynamic variables

`$guid`, `$randomUUID`, `$timestamp`, `$isoTimestamp`, `$randomInt` and the
`$random*` variables of Postman (`$randomFirstName`, `$randomEmail`,
`$randomCity`, `$randomPrice`, `$randomCatsImage`, `$randomLoremParagraphs`…)
map to faker-js functions. Two are not supported: `$randomLocale` and
`$randomCompanySuffix` (faker-js no longer has them).
