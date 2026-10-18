# ADR-009: faker-js for fake data

- Status: accepted
- Date: 2026-10-18
- Supersedes: [ADR-005](005-faker.md)

## Context

ADR-005 chose our own faker in Rust with faker-js naming and English and
Russian data. It covers about 70 functions. Users coming from Insomnia and
Postman expect the whole faker-js API (`helpers.arrayElement`,
`string.alpha({ casing })`, `date.between`, `finance.bitcoinAddress`…), its
locales, and `require('@faker-js/faker')` in scripts. Keeping our datasets
and generators in step with faker-js is not realistic.

Since stage 6 Reqly embeds QuickJS (ADR-006), which can run faker-js itself.

## Options

1. faker-js in scripts only; templates keep the Rust faker. Two generators
   with different data and functions.
2. faker-js everywhere; the Rust faker is removed.
3. Rust first, faker-js for what Rust lacks. Two behaviours behind one name.
4. Load faker-js from npm on demand. Needs the network and trusts a registry
   at run time.
5. Port faker-js data and generators to Rust. A permanent chase.

## Spike

`@faker-js/faker` 10.6.0 (MIT) ships prebuilt minified ES modules split into
one chunk per locale: 2.4 MB of JavaScript for all 76 locales, 0.6 MB for
English and Russian. It needs no Node APIs; `image.dataUri` uses
`TextEncoder` and `btoa`, which a small polyfill provides. In QuickJS with
bytecode cached per process, a script that uses faker takes 23 ms the first
time and 6 ms after that (release build); a template call on a warm runtime
takes microseconds.

## Decision

Option 2, with all locales:

- A `reqly-faker` crate embeds the faker-js modules unchanged
  (`scripts/vendor-faker.mjs` updates them and generates `catalog.json` with
  locales, functions with their parameters, and package exports).
- Modules are compiled to QuickJS bytecode once per process and loaded by a
  module loader; a locale loads on first use.
- Templates call faker-js on one long-lived runtime in a background thread
  (256 MB, 5 s per call; the runtime restarts after a runaway call). Only
  `module.method` functions of a faker instance can be called.
- The template language gains array and object literals so that every
  faker-js signature can be written; positional shortcuts keep
  `number.int(1, 100)` working.
- Scripts load the same modules into their own sandbox: `faker`, `rq.faker`,
  `require('@faker-js/faker')` (lazy per locale) and
  `require('@faker-js/faker/locale/<code>')`.
- `Locale` becomes a faker-js locale code; unknown codes fall back to English.
- Postman dynamic variables map to faker-js functions.

## Consequences

- The binary grows by about 2.5 MB.
- The data matches faker-js and Insomnia; our hand-made Russian specifics
  (`+7` numbers by default, six-digit postal codes) are now faker-js's
  `ru` locale.
- `rq.faker` returns faker-js values (numbers stay numbers), not strings.
- Updating faker-js is `node scripts/vendor-faker.mjs <version>` and a test
  run.
