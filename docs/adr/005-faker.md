# ADR-005: Faker and data locales

- Status: accepted
- Date: 2026-10-17

## Context

We need a fake data generator with the `{{ $faker.person.firstName }}` syntax
(category names as in faker-js) and English and Russian locales (PRD §7.11).
Generation must run in Rust so that desktop and server give the same result.

## Options

1. The `fake` crate 5.1: rich datasets, 14 locales.
2. Our own implementation with built-in datasets.

## Spike

`fake` 5.1 has the locales ar_sa, cy_gb, de_de, en, fa_ir, fr_fr, it_it,
ja_jp, nl_nl, pt_br, pt_pt, tr_tr, zh_cn and zh_tw. There is no Russian one;
it would have to be written outside the crate, mixing two ways of generating
data.

## Decision

Our own `reqly-template::faker` module:

- a function registry with faker-js naming (`person`, `internet`, `location`,
  `phone`, `company`, `lorem`, `number`, `string`, `datatype`, `date`,
  `color`, `commerce`, `finance`, `image`);
- English and Russian datasets; for Russian: grammatical gender of last names
  (Ivanov → Ivanova), patronymics, `+7` phone numbers, six-digit postal codes,
  `RU…` IBANs and transliteration for e-mail addresses;
- Postman dynamic variables (`$guid`, `$randomFirstName`, …) map to the same
  functions.

## Consequences

- The datasets are smaller than `fake`'s but can grow without external
  dependencies.
- A new locale means a new data module and `Locale` branches in the generators.
