# Библиотеки и рецепты

В скриптах доступны эти библиотеки. `_`, `CryptoJS`, `moment` и `faker` —
ещё и глобальные переменные, как в Postman, поэтому `require` для них не
обязателен.

| Библиотека                          | Версия | `require`                    | Глобальная          |
| ----------------------------------- | ------ | ---------------------------- | ------------------- |
| chai (проверки)                     | 4.5    | `require('chai')`            | `rq.expect`         |
| lodash (работа с данными)           | 4.17   | `require('lodash')`          | `_`                 |
| crypto-js (хеши, HMAC, AES, Base64) | 4.2    | `require('crypto-js')`       | `CryptoJS`          |
| moment (даты)                       | 2.30   | `require('moment')`          | `moment`            |
| uuid                                | —      | `require('uuid')`            | —                   |
| faker-js (тестовые данные)          | 10     | `require('@faker-js/faker')` | `faker`, `rq.faker` |

Другие модули (модули Node.js, `cheerio`, `xml2js`, `ajv`…) недоступны.
Чтобы проверить ответ по JSON Schema, используйте проверку на вкладке
**Тесты** запроса ([Тесты](help:testing)).

Все примеры на этой странице прогоняются тестами самого Reqly на таком
образце ответа:

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

Проверки для тестов. `rq.expect` — это `expect` из chai; `assert` —
классический стиль. Непрошедшая проверка бросает исключение, и окружающий
`rq.test` падает.

```js
const { expect, assert } = require('chai');
const body = rq.response.json();

rq.test('стиль expect', () => {
  expect(body.total).to.equal(3); // строгое равенство
  expect(body.user).to.eql({
    // глубокое равенство
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

rq.test('стиль assert', () => {
  assert.strictEqual(body.total, 3);
  assert.deepEqual(body.user.roles, ['admin', 'dev']);
  assert.isArray(body.items);
  assert.lengthOf(body.items, 3);
  assert.isAbove(body.items[2].price, 20);
  assert.include(body.user.name, 'Ad');
});

rq.test('отрицание', () => {
  expect(body.items.map((i) => i.status)).to.not.include('cancelled');
  rq.expect(body.token).to.not.be.empty;
});
```

Частые слова: `equal` (`===`), `eql` (глубоко), `include`, `property`,
`keys`, `lengthOf`, `a` / `an` (тип), `above` / `below` / `within`,
`match`, `ok`, `true`, `null`, `undefined`, `empty`, `throw`; `not`
отрицает, `deep` и `nested` меняют способ сравнения.

## lodash

Утилиты для объектов и массивов: безопасное чтение вложенных путей,
группировка, сортировка, выбор полей, сравнение.

```js
const body = rq.response.json();

// Безопасный доступ по пути со значением по умолчанию
console.log(_.get(body, 'user.roles[0]')); // "admin"
console.log(_.get(body, 'user.address.city', 'n/a')); // "n/a"

// Поиск, фильтр, сортировка
const paid = _.filter(body.items, { status: 'paid' });
const book = _.find(body.items, (i) => i.name === 'Book');
const byPrice = _.sortBy(body.items, 'price').map((i) => i.name);
console.log(paid.length, book.id, byPrice);

// Агрегаты
console.log(_.sumBy(body.items, 'price')); // 44.5
console.log(_.groupBy(body.items, 'status')); // { paid: [...], new: [...] }
console.log(_.countBy(body.items, 'status')); // { paid: 2, new: 1 }
console.log(_.keyBy(body.items, 'id')[2].name); // "Book"

// Форма объектов
console.log(_.pick(body.user, ['id', 'name']));
console.log(_.omit(body.user, ['email']));
console.log(_.uniq(['a', 'b', 'a']), _.chunk([1, 2, 3, 4, 5], 2));

rq.test('id уникальны и отсортированы', () => {
  const ids = _.map(body.items, 'id');
  rq.expect(_.uniq(ids)).to.eql(ids);
  rq.expect(_.isEqual(ids, _.sortBy(ids))).to.be.true;
});
```

## crypto-js

Хеши, HMAC-подписи, Base64 и шифрование AES. Результаты — объекты
`WordArray`: вызовите `.toString()` (hex) или
`.toString(CryptoJS.enc.Base64)`.

```js
const text = 'hello';

// Хеши
console.log(CryptoJS.MD5(text).toString());
console.log(CryptoJS.SHA1(text).toString());
console.log(CryptoJS.SHA256(text).toString()); // hex
console.log(CryptoJS.SHA256(text).toString(CryptoJS.enc.Base64)); // base64
console.log(CryptoJS.SHA512(text).toString());

// HMAC с секретом из окружения
const secret = rq.environment.get('api_secret');
console.log(CryptoJS.HmacSHA256(text, secret).toString());

// Base64 от строки UTF-8 и обратно
const b64 = CryptoJS.enc.Base64.stringify(CryptoJS.enc.Utf8.parse('Привет'));
console.log(b64, CryptoJS.enc.Base64.parse(b64).toString(CryptoJS.enc.Utf8));

// AES с паролем
const encrypted = CryptoJS.AES.encrypt('card 4111', 'passphrase').toString();
const decrypted = CryptoJS.AES.decrypt(encrypted, 'passphrase').toString(CryptoJS.enc.Utf8);
console.log(encrypted, decrypted);

// AES-CBC с явным ключом и IV (hex)
const key = CryptoJS.enc.Hex.parse('000102030405060708090a0b0c0d0e0f');
const iv = CryptoJS.enc.Hex.parse('0f0e0d0c0b0a09080706050403020100');
const cipher = CryptoJS.AES.encrypt('data', key, { iv, mode: CryptoJS.mode.CBC });
console.log(cipher.toString());

// Получение ключа из пароля
console.log(
  CryptoJS.PBKDF2('password', 'salt', { keySize: 256 / 32, iterations: 1000 }).toString(),
);
```

То же в виде модуля: `const CryptoJS = require('crypto-js');`.

## moment

Разбор, форматирование и вычисления с датами.

```js
const now = moment();
console.log(now.toISOString()); // 2026-11-17T10:00:00.000Z
console.log(now.format('YYYY-MM-DD HH:mm:ss'));
console.log(now.unix(), now.valueOf()); // секунды, миллисекунды

// Вычисления
console.log(moment().add(1, 'day').format('YYYY-MM-DD'));
console.log(moment().subtract(30, 'minutes').toISOString());
console.log(moment().startOf('month').format('YYYY-MM-DD'));
console.log(moment().endOf('week').format('dddd'));

// Разбор и сравнение
const created = moment('2026-11-01T10:00:00Z');
console.log(created.isValid(), created.isBefore(moment()));
console.log(moment('2026-11-17').diff(created, 'days'));
console.log(moment('17.11.2026', 'DD.MM.YYYY').format('YYYY-MM-DD'));
console.log(moment.utc('2026-11-01T10:00:00Z').format('HH:mm'));

// Для тела запроса: {{from}} и {{to}}
rq.variables.set('from', moment().subtract(7, 'days').format('YYYY-MM-DD'));
rq.variables.set('to', moment().format('YYYY-MM-DD'));
```

## uuid

```js
const { v4, v7 } = require('uuid');
const id = v4(); // случайный: 9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d
const sortable = v7(); // упорядоченный по времени: 019a8f6e-…
console.log(id, sortable, crypto.randomUUID());
rq.request.headers.upsert({ key: 'Idempotency-Key', value: id });
```

В шаблонах то же самое — `{{ $uuid }}` или `{{ $guid }}`.

## faker-js

Правдоподобные тестовые данные на любой локали. `faker` (и `rq.faker`)
использует локаль данных из **Настройки → Шаблоны**; другие локали
загружаются при первом обращении.

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

// Повторяемые значения
faker.seed(42);
console.log(faker.number.int({ min: 1, max: 100 }));

// Своя цепочка локалей
const { Faker, ru, en } = require('@faker-js/faker');
const custom = new Faker({ locale: [ru, en] });
console.log(custom.person.fullName());

// Положить в тело запроса
rq.request.body.raw = JSON.stringify(user, null, 2);
```

`require('@faker-js/faker/locale/de_AT')` даёт `{ faker }` для одной
локали. В шаблонах те же данные — `{{ $faker.person.firstName }}`.

## Рецепты

### Получить токен один раз и переиспользовать

Pre-request скрипт коллекции: получает OAuth-токен, если его нет или он
истёк, и добавляет его в каждый запрос.

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

Для стандартного OAuth 2.0 вкладка **Авторизация** делает это без скрипта
([Авторизация](help:auth)).

### Подписать запрос через HMAC

Многие API требуют подпись из метода, пути, метки времени и тела.

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

### Заголовок Basic auth вручную

```js
const user = rq.environment.get('username');
const password = rq.environment.get('password');
rq.request.headers.upsert({ key: 'Authorization', value: `Basic ${btoa(`${user}:${password}`)}` });
```

### Сохранить данные для следующих запросов

After-response скрипт запроса «создать»: следующие запросы используют
`{{orderId}}` и `{{lastItemName}}`.

```js
const body = rq.response.json();
if (rq.response.code === 200) {
  rq.environment.set('orderId', body.items[0].id);
  rq.environment.set('lastItemName', _.last(body.items).name);
  rq.variables.set('requestId', rq.response.headers.get('X-Request-Id'));
}
```

### Проверить каждый элемент списка

```js
const { items } = rq.response.json();

rq.test('все элементы корректны', () => {
  items.forEach((item, i) => {
    rq.expect(item, `элемент ${i}`).to.include.all.keys('id', 'name', 'price');
    rq.expect(item.price, `цена ${item.name}`).to.be.a('number').and.above(0);
    rq.expect(['new', 'paid', 'shipped']).to.include(item.status);
    rq.expect(moment(item.createdAt).isValid(), `дата ${item.name}`).to.be.true;
  });
});

rq.test('отсортированы по дате', () => {
  const dates = items.map((i) => i.createdAt);
  rq.expect(dates).to.eql(_.sortBy(dates));
});
```

### Запуск с файлом данных

При запуске коллекции с файлом данных CSV или JSON значения каждой строки —
это переменные (`{{email}}`), они же есть в `rq.iterationData`.

```js
const email = rq.iterationData.get('email');
const qty = Number(rq.iterationData.get('qty') || 1);
console.log(`итерация ${rq.info.iteration}: ${email} × ${qty}`);
rq.request.body.raw = JSON.stringify({ email, qty });
```

### Управлять порядком запуска

After-response скрипт: запрашивать страницы, пока есть следующая, затем
перейти к запросу «Summary».

```js
const { next } = rq.response.json();
if (next) {
  rq.variables.set('pageUrl', next);
  rq.execution.setNextRequest(rq.info.requestName); // выполнить этот запрос ещё раз
} else {
  rq.execution.setNextRequest('Summary');
}
```

### Сгенерировать тело запроса

Pre-request скрипт: новый случайный заказ при каждой отправке.

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
