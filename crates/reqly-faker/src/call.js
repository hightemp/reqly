// Runs in the template runtime: `__call(locale, path, argsJson)` returns
// `{"value": …}` or `{"unknown": true}` as JSON.
(() => {
  // faker-js reports deprecations on the console.
  const quiet = () => {};
  globalThis.console = { log: quiet, info: quiet, warn: quiet, error: quiet, debug: quiet };

  const fakers = {};
  const fakerFor = (code) => (fakers[code] ??= __import(`faker/locale/${code}.js`).faker);

  // Positional shortcuts for functions that take an options object:
  // `number.int(1, 100)` is `number.int({ min: 1, max: 100 })`.
  const POSITIONAL = {
    'number.int': ['min', 'max', 'multipleOf'],
    'number.float': ['min', 'max', 'fractionDigits'],
    'number.bigInt': ['min', 'max'],
    'date.between': ['from', 'to'],
    'date.betweens': ['from', 'to', 'count'],
    'finance.amount': ['min', 'max', 'dec', 'symbol'],
    'internet.password': ['length', 'memorable', 'pattern', 'prefix'],
    'image.url': ['width', 'height'],
  };
  // Of those, the ones without a plain first parameter.
  const OPTIONS_ONLY = new Set([
    'date.between',
    'date.betweens',
    'finance.amount',
    'internet.password',
    'image.url',
  ]);

  function positional(path, args) {
    const keys = POSITIONAL[path];
    const primitive = args.every((a) => a === null || typeof a !== 'object');
    if (!keys || !args.length || !primitive) return args;
    if (args.length === 1 && !OPTIONS_ONLY.has(path)) return args;
    const options = {};
    keys.forEach((k, i) => {
      if (args[i] !== undefined && args[i] !== null) options[k] = args[i];
    });
    return [options];
  }

  function lookup(faker, path) {
    const parts = path.split('.');
    if (parts.length !== 2) return null;
    const [module, name] = parts;
    if (module === 'fakerCore' || !Object.hasOwn(faker, module)) return null;
    const target = faker[module];
    if (!target || typeof target !== 'object') return null;
    if (!/^[A-Za-z]\w*$/.test(name) || name === 'constructor' || name in Object.prototype) {
      return null;
    }
    return typeof target[name] === 'function' ? { target, fn: target[name] } : null;
  }

  globalThis.__call = (code, path, argsJson) => {
    const faker = fakerFor(code);
    if (path === '') return JSON.stringify({ value: null });
    const found = lookup(faker, path);
    if (!found) return JSON.stringify({ unknown: true });
    const value = found.fn.apply(found.target, positional(path, JSON.parse(argsJson)));
    return JSON.stringify({ value: value === undefined ? null : value }, (_, v) =>
      typeof v === 'bigint' ? v.toString() : v,
    );
  };
})();
