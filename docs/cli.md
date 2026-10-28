# reqly-cli

`reqly-cli run` runs a collection with its tests and assertions, for CI.
It imports the collection into a temporary in-memory workspace and sends the
requests from the machine it runs on, like the
[collection runner](testing.md#collection-runner) of the app.

Binaries for Linux (x86_64, aarch64), macOS and Windows are attached to each
[release](https://github.com/hightemp/reqly/releases); or build it with
`cargo build --release -p reqly-cli`.

## Usage

```sh
# From an export: Reqly, Insomnia, Postman, Bruno, HAR, OpenAPI…
reqly-cli run api.reqly.json --env staging --report results.xml

# From a Reqly server, with an API token (account settings → API tokens)
REQLY_TOKEN=… reqly-cli run --server https://reqly.example.com --collection "Billing API"
```

| Option | |
|---|---|
| `--server <URL>`, `--token <TOKEN>` | Export the collection from a Reqly server; the token defaults to `$REQLY_TOKEN` |
| `--collection <ID\|NAME>` | The collection; needed with `--server` and for files with several collections |
| `-e, --env <NAME>` | An environment of the collection instead of its active one |
| `--var <NAME=VALUE>` | Set a variable, overriding every other value (repeatable) |
| `-d, --data <FILE>` | CSV or JSON data file: one iteration per row |
| `-n, --iterations <N>` | Iterations (default: one per data row, or one) |
| `--folder <NAME\|PATH>` | Run one folder; `Parent/Child` for nested folders |
| `--delay <MS>` | Pause between requests |
| `--bail` | Stop at the first failed request |
| `-r, --reporter <pretty\|json\|junit>` | Console output (default `pretty`) |
| `--report <FILE>` | Also write a report: `.html`, `.xml` (JUnit) or `.json` (repeatable) |
| `--no-color` | Plain pretty output (also when `NO_COLOR` is set or the output is not a terminal) |

Exit codes: **0** every check passed, **1** a request failed or a check did
not pass, **2** the run could not start (bad options, unreadable file,
unknown collection, environment or folder, server errors).

A request fails when it gets no response or when one of its tests or
assertions fails; the status code alone does not fail it, so check it with
an assertion (`Status equals 200`) where it matters.

## GitHub Actions

```yaml
- name: API tests
  run: |
    curl -sL https://github.com/hightemp/reqly/releases/download/v0.9.0/reqly-cli-v0.9.0-linux-x86_64.tar.gz | tar xz
    ./reqly-cli run tests/api.reqly.json --env ci --var "base=$API_URL" --report reqly.xml
- uses: mikepenz/action-junit-report@v5
  if: always()
  with:
    report_paths: reqly.xml
```
