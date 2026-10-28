# Tests and the collection runner

## Assertions

The **Tests** tab of a request holds no-code assertions. Each row checks one
part of the response:

| Source | Property | Value checked |
|---|---|---|
| Status | — | The status code |
| Response time (ms) | — | Total time in milliseconds |
| Header | Header name (any case) | The header; repeated headers are joined with `, ` |
| Body | — | The body as text |
| JSONPath | Expression, e.g. `$.data.id` | The selected value; several matches form an array |
| jq | Program, e.g. `.items \| length` | The output; several outputs form an array |

| Operator | Passes when |
|---|---|
| equals, does not equal | The value equals the expected one. Expected values are read as JSON when they parse (`200`, `true`, `"x"`, `[1,2]`, `{"a":1}`), otherwise as text; numbers compare by value (`7` equals `7.0`) |
| contains, does not contain | Text contains the expected text; an array has an element equal to it; an object has the key |
| matches regex | The value as text matches the regular expression (Rust syntax) |
| less than, at most, greater than, at least | Both sides are numbers |
| exists, does not exist | The value is present (a header, a JSONPath match, a status) |
| matches JSON Schema | The value (the parsed body for **Body**) is valid against the schema in the editor below the row |

The property and the expected value are templates: `{{ id }}` and tags work,
and they render after the after-response scripts ran, so a script can set
the value an assertion expects. A request that gets no response fails its
assertions, except **does not exist**.

Results appear in the response's **Tests** tab, before the results of script
tests (`rq.test`, `pm.test`, see [scripts.md](scripts.md)), with the value
that was received when a check fails. They are kept with the response in the
history.

## Collection runner

**Run collection…** (collection menu, command palette) or **Run…** on a
folder opens the runner:

- **Requests** — all HTTP and GraphQL requests of the collection or folder in
  tree order; untick requests to skip them or move them up and down. WebSocket,
  SSE and gRPC requests are not run.
- **Environment** — the active one or another environment of the collection,
  for this run only.
- **Iterations** and **Data file** — a CSV file (a header row of variable
  names, then one row per iteration) or a JSON array of objects. Without a
  number of iterations there is one per row; with more iterations than rows
  the last row repeats. Row values are variables (above request variables,
  below values set by scripts) and `rq.iterationData` / `pm.iterationData`
  in scripts; `rq.info.iteration` is the zero-based iteration.
- **Delay between requests** and **Stop at the first failure** (an error or
  a failed check).

Scripts can change the order with `pm.execution.setNextRequest('name')`
(or `postman.setNextRequest`); `null` ends the iteration.

Results arrive as requests finish, grouped by iteration; open a row for its
checks. **Rerun failed** runs the failed requests again with the same
options. Responses of a run go to the history like any other.

### Reports

**Save report** writes the finished run as:

- **HTML** — a standalone page with the summary and every check;
- **JUnit XML** — a `testsuite` per request sent (with the iteration when
  there are several) and a `testcase` per assertion and script test; failed
  checks are `failure`s, requests without a response `error`s;
- **JSON** — the whole run with its statistics.

The same reports come from the [CLI](cli.md).
