# Libraries and recipes

Scripts can use these libraries. `_`, `CryptoJS`, `moment` and `faker` are
also globals, as in Postman, so `require` is optional for them.

| Library                               | Version | `require`                    | Global              |
| ------------------------------------- | ------- | ---------------------------- | ------------------- |
| chai (assertions)                     | 4.5     | `require('chai')`            | `rq.expect`         |
| lodash (data utilities)               | 4.17    | `require('lodash')`          | `_`                 |
| crypto-js (hashes, HMAC, AES, Base64) | 4.2     | `require('crypto-js')`       | `CryptoJS`          |
| moment (dates)                        | 2.30    | `require('moment')`          | `moment`            |
| uuid                                  | —       | `require('uuid')`            | —                   |
| faker-js (fake data)                  | 10      | `require('@faker-js/faker')` | `faker`, `rq.faker` |

Other modules (Node.js modules, `cheerio`, `xml2js`, `ajv`…) are not
available. To validate a response against a JSON Schema, use an assertion
in the request's **Tests** tab ([Tests](help:testing)).

All examples on this page are run by Reqly's own tests against a sample
response like this one:

```
{
  "token": "abc",
  "user": { "id": 42, "name": "Ada", "email": "ada@example.com", "roles": ["admin", "dev"] },
  "items": [
    { "id": 1, "name": "Pen",  "price": 2.5, "status": "paid", "createdAt": "2026-11-01T10:00:00Z" },
    { "id": 2, "name": "Book", "price": 12,  "status": "new",  "createdAt": "2026-11-02T10:00:00Z" },
    { "id": 3, "name": "Lamp", "price": 30,  "status": "paid", "createdAt": "2026-11-03T10:00:00Z" }
  ],
  "total": 3,
  "next": null
}
```

## chai

Assertions for tests. `rq.expect` is chai's `expect`; `assert` is the
classic style. A failed assertion throws, which fails the surrounding
`rq.test`.

```js
const { expect, assert } = require('chai');
const body = rq.response.json();

rq.test('expect style', () => {
  expect(body.total).to.equal(3); // strict equality
  expect(body.user).to.eql({
    // deep equality
    id: 42,
    name: 'Ada',
    email: 'ada@example.com',
    roles: ['admin', 'dev'],
  });
  expect(body.user).to.have.property('email').that.is.a('string');
  expect(body.user.roles).to.include('admin').and.have.lengthOf(2);
  expect(body.items).to.be.an('array').that.is.not.empty;
  expect(body.items[0]).to.have.all.keys('id', 'name', 'price', 'status', 'createdAt');
  expect(body.items[1].price).to.be.within(10, 20);
  expect(body.user.email).to.match(/^[^@]+@example\.com$/);
  expect(body.next).to.be.null;
  expect(() => JSON.parse('{')).to.throw(SyntaxError);
});

rq.test('assert style', () => {
  assert.strictEqual(body.total, 3);
  assert.deepEqual(body.user.roles, ['admin', 'dev']);
  assert.isArray(body.items);
  assert.lengthOf(body.items, 3);
  assert.isAbove(body.items[2].price, 20);
  assert.include(body.user.name, 'Ad');
});

rq.test('negation', () => {
  expect(body.items.map((i) => i.status)).to.not.include('cancelled');
  rq.expect(body.token).to.not.be.empty;
});
```

Frequent words: `equal` (`===`), `eql` (deep), `include`, `property`,
`keys`, `lengthOf`, `a` / `an` (type), `above` / `below` / `within`,
`match`, `ok`, `true`, `null`, `undefined`, `empty`, `throw`; `not` negates,
`deep` and `nested` change how values are compared.

## lodash

Utilities for objects and arrays: reading deep paths safely, grouping,
sorting, picking fields, comparing.

```js
const body = rq.response.json();

// Safe deep access with a default
console.log(_.get(body, 'user.roles[0]')); // "admin"
console.log(_.get(body, 'user.address.city', 'n/a')); // "n/a"

// Search, filter, sort
const paid = _.filter(body.items, { status: 'paid' });
const book = _.find(body.items, (i) => i.name === 'Book');
const byPrice = _.sortBy(body.items, 'price').map((i) => i.name);
console.log(paid.length, book.id, byPrice);

// Aggregate
console.log(_.sumBy(body.items, 'price')); // 44.5
console.log(_.groupBy(body.items, 'status')); // { paid: [...], new: [...] }
console.log(_.countBy(body.items, 'status')); // { paid: 2, new: 1 }
console.log(_.keyBy(body.items, 'id')[2].name); // "Book"

// Shape objects
console.log(_.pick(body.user, ['id', 'name']));
console.log(_.omit(body.user, ['email']));
console.log(_.uniq(['a', 'b', 'a']), _.chunk([1, 2, 3, 4, 5], 2));

rq.test('ids are unique and sorted', () => {
  const ids = _.map(body.items, 'id');
  rq.expect(_.uniq(ids)).to.eql(ids);
  rq.expect(_.isEqual(ids, _.sortBy(ids))).to.be.true;
});
```

## crypto-js

Hashes, HMAC signatures, Base64 and AES encryption. Results are
`WordArray` objects: call `.toString()` (hex) or
`.toString(CryptoJS.enc.Base64)`.

```js
const text = 'hello';

// Hashes
console.log(CryptoJS.MD5(text).toString());
console.log(CryptoJS.SHA1(text).toString());
console.log(CryptoJS.SHA256(text).toString()); // hex
console.log(CryptoJS.SHA256(text).toString(CryptoJS.enc.Base64)); // base64
console.log(CryptoJS.SHA512(text).toString());

// HMAC with a secret from the environment
const secret = rq.environment.get('api_secret');
console.log(CryptoJS.HmacSHA256(text, secret).toString());

// Base64 of a UTF-8 string, and back
const b64 = CryptoJS.enc.Base64.stringify(CryptoJS.enc.Utf8.parse('Привет'));
console.log(b64, CryptoJS.enc.Base64.parse(b64).toString(CryptoJS.enc.Utf8));

// AES with a passphrase
const encrypted = CryptoJS.AES.encrypt('card 4111', 'passphrase').toString();
const decrypted = CryptoJS.AES.decrypt(encrypted, 'passphrase').toString(CryptoJS.enc.Utf8);
console.log(encrypted, decrypted);

// AES-CBC with an explicit key and IV (hex)
const key = CryptoJS.enc.Hex.parse('000102030405060708090a0b0c0d0e0f');
const iv = CryptoJS.enc.Hex.parse('0f0e0d0c0b0a09080706050403020100');
const cipher = CryptoJS.AES.encrypt('data', key, { iv, mode: CryptoJS.mode.CBC });
console.log(cipher.toString());

// Key derivation
console.log(
  CryptoJS.PBKDF2('password', 'salt', { keySize: 256 / 32, iterations: 1000 }).toString(),
);
```

The same in a module form: `const CryptoJS = require('crypto-js');`.

## moment

Parsing, formatting and calculating dates.

```js
const now = moment();
console.log(now.toISOString()); // 2026-11-17T10:00:00.000Z
console.log(now.format('YYYY-MM-DD HH:mm:ss'));
console.log(now.unix(), now.valueOf()); // seconds, milliseconds

// Calculations
console.log(moment().add(1, 'day').format('YYYY-MM-DD'));
console.log(moment().subtract(30, 'minutes').toISOString());
console.log(moment().startOf('month').format('YYYY-MM-DD'));
console.log(moment().endOf('week').format('dddd'));

// Parsing and comparing
const created = moment('2026-11-01T10:00:00Z');
console.log(created.isValid(), created.isBefore(moment()));
console.log(moment('2026-11-17').diff(created, 'days'));
console.log(moment('17.11.2026', 'DD.MM.YYYY').format('YYYY-MM-DD'));
console.log(moment.utc('2026-11-01T10:00:00Z').format('HH:mm'));

// For a request body: {{from}} and {{to}}
rq.variables.set('from', moment().subtract(7, 'days').format('YYYY-MM-DD'));
rq.variables.set('to', moment().format('YYYY-MM-DD'));
```

## uuid

```js
const { v4, v7 } = require('uuid');
const id = v4(); // random: 9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d
const sortable = v7(); // time-ordered: 019a8f6e-…
console.log(id, sortable, crypto.randomUUID());
rq.request.headers.upsert({ key: 'Idempotency-Key', value: id });
```

In templates the same is `{{ $uuid }}` or `{{ $guid }}`.

## faker-js

Realistic fake data in any locale. `faker` (and `rq.faker`) uses the data
locale from **Settings → Templates**; other locales load on first use.

```js
const { faker, fakerDE, fakerJA } = require('@faker-js/faker');

const user = {
  id: faker.string.uuid(),
  firstName: faker.person.firstName(),
  lastName: faker.person.lastName(),
  email: faker.internet.email(),
  phone: faker.phone.number(),
  birthday: faker.date.birthdate({ min: 18, max: 65, mode: 'age' }).toISOString().slice(0, 10),
  address: {
    city: fakerDE.location.city(),
    street: faker.location.streetAddress(),
  },
  tags: faker.helpers.arrayElements(['new', 'vip', 'beta'], 2),
  balance: faker.finance.amount({ min: 0, max: 1000, dec: 2 }),
  bio: faker.lorem.sentence(),
};
console.log(user, fakerJA.person.fullName());

// Repeatable values
faker.seed(42);
console.log(faker.number.int({ min: 1, max: 100 }));

// A locale chain of your own
const { Faker, ru, en } = require('@faker-js/faker');
const custom = new Faker({ locale: [ru, en] });
console.log(custom.person.fullName());

// Put it into the request body
rq.request.body.raw = JSON.stringify(user, null, 2);
```

`require('@faker-js/faker/locale/de_AT')` gives `{ faker }` for one locale.
In templates the same data is `{{ $faker.person.firstName }}`.

## Recipes

### Get a token once and reuse it

Collection pre-request script: fetches an OAuth token when there is none or
it has expired, then adds it to every request.

```js
const expires = Number(rq.environment.get('tokenExpires') || 0);
if (!rq.environment.get('accessToken') || Date.now() > expires) {
  const res = await rq.sendRequest({
    url: rq.variables.replaceIn('{{base_url}}/oauth/token'),
    method: 'POST',
    body: {
      mode: 'urlencoded',
      urlencoded: [
        { key: 'grant_type', value: 'client_credentials' },
        { key: 'client_id', value: rq.environment.get('client_id') },
        { key: 'client_secret', value: rq.environment.get('client_secret') },
      ],
    },
  });
  const { access_token, expires_in } = res.json();
  rq.environment.set('accessToken', access_token);
  rq.environment.set('tokenExpires', Date.now() + (expires_in - 60) * 1000);
}
rq.request.headers.upsert({
  key: 'Authorization',
  value: `Bearer ${rq.environment.get('accessToken')}`,
});
```

For standard OAuth 2.0 the **Auth** tab does this without a script
([Authentication](help:auth)).

### Sign a request with HMAC

Many APIs want a signature of the method, path, timestamp and body.

```js
const secret = rq.environment.get('api_secret');
const timestamp = Math.floor(Date.now() / 1000).toString();
const body = rq.variables.replaceIn(rq.request.body.raw || '');
const path = rq.request.url.getPath();
const payload = [rq.request.method, path, timestamp, CryptoJS.SHA256(body).toString()].join('\n');
const signature = CryptoJS.HmacSHA256(payload, secret).toString(CryptoJS.enc.Base64);

rq.request.headers.upsert({ key: 'X-Timestamp', value: timestamp });
rq.request.headers.upsert({ key: 'X-Signature', value: signature });
```

### Basic auth header by hand

```js
const user = rq.environment.get('username');
const password = rq.environment.get('password');
rq.request.headers.upsert({ key: 'Authorization', value: `Basic ${btoa(`${user}:${password}`)}` });
```

### Save data for the next requests

After-response script of a "create" request: the next requests use
`{{orderId}}` and `{{lastItemName}}`.

```js
const body = rq.response.json();
if (rq.response.code === 200) {
  rq.environment.set('orderId', body.items[0].id);
  rq.environment.set('lastItemName', _.last(body.items).name);
  rq.variables.set('requestId', rq.response.headers.get('X-Request-Id'));
}
```

### Check every item of a list

```js
const { items } = rq.response.json();

rq.test('every item is valid', () => {
  items.forEach((item, i) => {
    rq.expect(item, `item ${i}`).to.include.all.keys('id', 'name', 'price');
    rq.expect(item.price, `price of ${item.name}`).to.be.a('number').and.above(0);
    rq.expect(['new', 'paid', 'shipped']).to.include(item.status);
    rq.expect(moment(item.createdAt).isValid(), `date of ${item.name}`).to.be.true;
  });
});

rq.test('sorted by date', () => {
  const dates = items.map((i) => i.createdAt);
  rq.expect(dates).to.eql(_.sortBy(dates));
});
```

### Data-driven runs

In the runner with a CSV or JSON data file, each row's values are
variables (`{{email}}`) and are in `rq.iterationData`.

```js
const email = rq.iterationData.get('email');
const qty = Number(rq.iterationData.get('qty') || 1);
console.log(`iteration ${rq.info.iteration}: ${email} × ${qty}`);
rq.request.body.raw = JSON.stringify({ email, qty });
```

### Control the runner's order

After-response script: keep fetching pages while there is a next one, then
continue with "Summary".

```js
const { next } = rq.response.json();
if (next) {
  rq.variables.set('pageUrl', next);
  rq.execution.setNextRequest(rq.info.requestName); // run this request again
} else {
  rq.execution.setNextRequest('Summary');
}
```

### Generate a request body

Pre-request script: a new random order on every send.

```js
const order = {
  customer: { name: faker.person.fullName(), email: faker.internet.email() },
  items: Array.from({ length: faker.number.int({ min: 1, max: 3 }) }, () => ({
    sku: faker.string.alphanumeric(8).toUpperCase(),
    qty: faker.number.int({ min: 1, max: 5 }),
    price: Number(faker.commerce.price({ min: 1, max: 100 })),
  })),
  deliverOn: moment()
    .add(faker.number.int({ min: 1, max: 7 }), 'days')
    .format('YYYY-MM-DD'),
};
order.total = _.round(
  _.sumBy(order.items, (i) => i.qty * i.price),
  2,
);
rq.request.body.raw = JSON.stringify(order, null, 2);
```
