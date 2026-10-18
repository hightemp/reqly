#!/usr/bin/env node
// Vendors @faker-js/faker into crates/reqly-faker/vendor (ADR-009).
//
//   node scripts/vendor-faker.mjs [version]
//
// Copies the package's prebuilt ES modules unchanged (they are already
// minified and split into one chunk per locale) and writes catalog.json:
// the locales with their names, the faker functions with their parameters
// (taken from the type declarations) and the package's exports.

import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const version = process.argv[2] ?? '10.6.0';
const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const vendor = join(root, 'crates/reqly-faker/vendor');
const tmp = mkdtempSync(join(tmpdir(), 'reqly-faker-'));

execFileSync('npm', ['pack', `@faker-js/faker@${version}`, '--silent'], { cwd: tmp });
const tarball = readdirSync(tmp).find((f) => f.endsWith('.tgz'));
execFileSync('tar', ['xzf', tarball], { cwd: tmp });
const pkg = join(tmp, 'package');
const dist = join(pkg, 'dist');

rmSync(join(vendor, 'faker'), { recursive: true, force: true });
cpSync(dist, join(vendor, 'faker'), {
  recursive: true,
  filter: (src) => !src.endsWith('.d.ts'),
});
cpSync(join(pkg, 'LICENSE'), join(vendor, 'LICENSE'));

const lib = await import(pathToFileURL(join(dist, 'index.js')).href);

const locales = readdirSync(join(dist, 'locale'))
  .filter((f) => f.endsWith('.js') && f !== 'base.js')
  .map((f) => f.slice(0, -3))
  .sort()
  .map((code) => {
    const meta = lib[code]?.metadata ?? {};
    return { code, title: meta.title ?? code, endonym: meta.endonym ?? meta.title ?? code };
  });

// Parameters from the declarations: `int(options?: number | { min?: number; … })`
// becomes `options?: number | {min, max, multipleOf}`.
const dts = readdirSync(dist)
  .filter((f) => f.endsWith('.d.ts'))
  .map((f) => readFileSync(join(dist, f), 'utf8'))
  .join('\n')
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/\/\/.*$/gm, '');

function classBodies() {
  const out = new Map();
  const re = /declare class (\w+)(?: extends (\w+))?[^{]*\{/g;
  let m;
  while ((m = re.exec(dts))) {
    let depth = 1;
    let i = re.lastIndex;
    while (depth > 0 && i < dts.length) {
      if (dts[i] === '{') depth++;
      else if (dts[i] === '}') depth--;
      i++;
    }
    out.set(m[1], { parent: m[2], body: dts.slice(re.lastIndex, i - 1) });
  }
  return out;
}

function simplify(params) {
  let s = params.replace(/\s+/g, ' ').trim();
  // Collapse object types to their keys, innermost first.
  for (let n = 0; n < 5; n++) {
    s = s.replace(/\{([^{}]*)\}/g, (_, inner) => {
      const keys = [...inner.matchAll(/(\w+)\??\s*:/g)].map((k) => k[1]);
      return `\u27e8${keys.join(', ')}\u27e9`;
    });
  }
  return s
    .replace(/\u27e8/g, '{')
    .replace(/\u27e9/g, '}')
    .replace(/<[^<>]*>/g, '')
    .replace(/:\s*/g, ': ')
    .trim();
}

function signatures(body) {
  const out = new Map();
  const re = /^\s*(\w+)(?:<[^(]*?>)?\(/gm;
  let m;
  while ((m = re.exec(body))) {
    let depth = 1;
    let i = re.lastIndex;
    while (depth > 0 && i < body.length) {
      if (body[i] === '(') depth++;
      else if (body[i] === ')') depth--;
      i++;
    }
    const params = simplify(body.slice(re.lastIndex, i - 1));
    // Overloads: keep the most general (longest) one.
    if (!out.has(m[1]) || out.get(m[1]).length < params.length) out.set(m[1], params);
  }
  return out;
}

const classes = classBodies();
function paramsOf(className, method) {
  for (let c = className; c && classes.has(c); c = classes.get(c).parent) {
    const p = signatures(classes.get(c).body).get(method);
    if (p !== undefined) return p;
  }
  return '';
}

const faker = lib.fakerEN;
const functions = [];
for (const module of Object.keys(faker).sort()) {
  const value = faker[module];
  if (!value || typeof value !== 'object' || module === 'fakerCore') continue;
  const className = `${module[0].toUpperCase()}${module.slice(1)}Module`;
  const methods = new Set();
  for (let p = Object.getPrototypeOf(value); p && p !== Object.prototype; p = Object.getPrototypeOf(p)) {
    for (const name of Object.getOwnPropertyNames(p)) {
      if (name !== 'constructor' && !name.startsWith('_') && typeof value[name] === 'function') {
        methods.add(name);
      }
    }
  }
  for (const name of [...methods].sort()) {
    functions.push({ name: `${module}.${name}`, args: paramsOf(className, name) });
  }
}

const catalog = { version, locales, functions, exports: Object.keys(lib).sort() };
writeFileSync(join(vendor, 'catalog.json'), `${JSON.stringify(catalog, null, 1)}\n`);
rmSync(tmp, { recursive: true, force: true });
console.log(`faker ${version}: ${locales.length} locales, ${functions.length} functions`);
