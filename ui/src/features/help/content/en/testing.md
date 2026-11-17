# Tests and the runner

## Assertions

The **Tests** tab of a request checks the response without code. Each row
picks a source, an operator and the expected value:

| Source        | Checks                     |
| ------------- | -------------------------- |
| Status        | the status code            |
| Response time | milliseconds               |
| Header        | a header by name           |
| Body          | the body as text           |
| JSONPath, jq  | a value inside a JSON body |

Operators: equals, contains, matches regex, less / greater than, exists, and
**matches JSON Schema**. Expected values can use
[variables](help:variables). Results appear in the response's **Tests** tab
together with script tests (`rq.test`, see [Scripts](help:scripts)).

## Collection runner

**Run collection…** (collection menu, command palette) or **Run…** on a
folder:

- choose and reorder the requests to run;
- pick the environment for this run;
- set **iterations** and a **data file** (CSV with a header row, or a JSON
  array) — each row's values become variables;
- add a delay between requests or stop at the first failure.

Results arrive as requests finish. **Rerun failed** repeats the failures.
**Save report** writes HTML, JUnit XML or JSON.

## In CI

`reqly-cli` runs a collection export from the command line and writes the
same reports, so tests can run in a pipeline.
