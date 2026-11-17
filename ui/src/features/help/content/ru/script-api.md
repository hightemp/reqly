# API скриптов

Глобальный объект `rq` — это весь API скрипта. `pm` (Postman) и `insomnia`
(Insomnia) — тот же объект под другими именами. Как выполняются скрипты,
описано в разделе [Скрипты](help:scripts).

| Член                                                     | Что это                                                |
| -------------------------------------------------------- | ------------------------------------------------------ |
| `rq.request`                                             | отправляемый запрос: адрес, метод, заголовки, тело     |
| `rq.response`                                            | ответ (только в after-response скриптах)               |
| `rq.variables`                                           | переменные всех уровней; переменные времени выполнения |
| `rq.environment`, `rq.collectionVariables`, `rq.globals` | окружения                                              |
| `rq.iterationData`                                       | строка данных при запуске коллекции                    |
| `rq.test()`, `rq.expect()`                               | тесты                                                  |
| `rq.sendRequest()`                                       | отправить другой запрос                                |
| `rq.info`                                                | какой запрос и какая итерация выполняются              |
| `rq.execution`                                           | пропустить запрос, изменить порядок запуска            |
| `rq.faker`                                               | тестовые данные                                        |
| `console`, `setTimeout`, `atob`, `btoa`, `require`       | глобальные функции                                     |

## rq.request

Запрос в том виде, в каком он будет отправлен. В pre-request скрипте его
можно менять; в after-response скрипте он показывает, что было отправлено.
Шаблоны ещё не отрендерены: `{{base_url}}` остаётся как написан.

| Свойство / метод       | Описание                                                                |
| ---------------------- | ----------------------------------------------------------------------- |
| `id`, `name`           | id и имя запроса (только чтение)                                        |
| `method`               | HTTP-метод; присваивание меняет его (переводится в верхний регистр)     |
| `url`                  | объект адреса, см. ниже; присваивание строки заменяет адрес и параметры |
| `headers`              | заголовки, [список свойств](help:script-api#списки-свойств)             |
| `body`                 | тело, см. ниже                                                          |
| `auth`                 | настройки авторизации, только чтение (`auth.type`, …)                   |
| `addHeader(header)`    | то же, что `headers.add(header)`                                        |
| `upsertHeader(header)` | то же, что `headers.upsert(header)`                                     |
| `removeHeader(name)`   | то же, что `headers.remove(name)`                                       |
| `toJSON()`             | `{ method, url, headers, body }`                                        |

```js
console.log(rq.request.name, rq.request.method, rq.request.url.toString());

rq.request.method = 'put'; // станет PUT
rq.request.upsertHeader({ key: 'X-Trace', value: 'on' });
rq.request.removeHeader('Accept');
```

### rq.request.url

Адрес без строки запроса и отдельно — параметры запроса списком.
Выключенные параметры пропускаются.

| Метод                     | Возвращает     | Описание                                                         |
| ------------------------- | -------------- | ---------------------------------------------------------------- |
| `toString()`              | строку         | полный адрес с включёнными параметрами                           |
| `update(url)`             | —              | заменяет адрес; его строка запроса становится списком параметров |
| `query`                   | список свойств | параметры запроса                                                |
| `getHost()`               | строку         | хост: `api.example.com`                                          |
| `getPath()`               | строку         | путь: `/orders` (`/`, если пусто)                                |
| `getQueryString()`        | строку         | `page=1&lang=ru`                                                 |
| `addQueryParams(params)`  | —              | добавляет `'a=1&b=2'`, `{ key, value }` или их массив            |
| `removeQueryParams(keys)` | —              | удаляет параметры по имени: `'a'` или `['a', 'b']`               |

```js
const url = rq.request.url;
url.addQueryParams('limit=50&sort=name');
url.removeQueryParams('page');
url.query.upsert({ key: 'lang', value: 'ru' });
console.log(url.getPath(), url.getQueryString());

// Заменить адрес целиком (строка запроса станет параметрами)
rq.request.url = 'https://api.example.com/v2/orders?page=2';
console.log(rq.request.url.toString());
```

### rq.request.body

| Член                     | Описание                                                                                                        |
| ------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `mode`                   | `none`, `raw` (JSON, XML, текст…), `urlencoded`, `formdata` (multipart), `file` или `graphql`; можно присвоить  |
| `raw`                    | текст тела (для `graphql` — запрос); можно присвоить                                                            |
| `urlencoded`, `formdata` | поля формы, список свойств                                                                                      |
| `update(body)`           | строка заменяет текст; `{ mode, raw }` или `{ mode: 'urlencoded', urlencoded: [{ key, value }] }` заменяют тело |
| `isEmpty()`              | true, если тела нет или текст пустой                                                                            |
| `toString()`             | текст тела                                                                                                      |

Присваивание `raw` сохраняет тип тела (JSON остаётся JSON). Если у тела не
было текстового типа, JSON-объект или массив становится JSON-телом, всё
остальное — текстом.

```js
// Прочитать, изменить и записать обратно JSON-тело
const order = JSON.parse(rq.request.body.raw);
order.qty = 5;
order.note = `создан ${new Date().toISOString()}`;
rq.request.body.raw = JSON.stringify(order, null, 2);

// Или превратить его в форму
rq.request.body.update({
  mode: 'urlencoded',
  urlencoded: [
    { key: 'item', value: '1' },
    { key: 'qty', value: '5' },
  ],
});
console.log(rq.request.body.mode, rq.request.body.urlencoded.toObject());
```

## rq.response

Ответ, в after-response скриптах (до отправки — `undefined`).

| Член              | Тип            | Описание                                                         |
| ----------------- | -------------- | ---------------------------------------------------------------- |
| `code`            | число          | код статуса: `200`                                               |
| `status`          | строка         | текст статуса: `OK`                                              |
| `reason()`        | строка         | то же, что `status`                                              |
| `headers`         | список свойств | заголовки ответа (имена без учёта регистра)                      |
| `responseTime`    | число          | общее время в миллисекундах                                      |
| `responseSize`    | число          | размер тела в байтах                                             |
| `text()`          | строка         | тело как текст                                                   |
| `json()`          | любое          | тело, разобранное как JSON; бросает исключение, если это не JSON |
| `body`            | строка         | тело как текст (Insomnia)                                        |
| `getStatusCode()` | число          | то же, что `code` (Insomnia)                                     |
| `getHeader(name)` | строка         | значение заголовка (Insomnia)                                    |
| `to`              |                | проверки, см. ниже                                               |

```js
console.log(rq.response.code, rq.response.status, `${rq.response.responseTime} мс`);
console.log(rq.response.headers.get('content-type'));

const body = rq.response.json();
console.log(body.items.map((i) => i.name).join(', '));
```

### Проверки ответа

`rq.response.to…` бросает исключение, если проверка не прошла, поэтому
используйте её внутри `rq.test()`. Свойства вроде `to.be.ok` проверяют
при чтении.

| Проверка                                                | Проходит, если                                         |
| ------------------------------------------------------- | ------------------------------------------------------ |
| `to.have.status(200)`, `to.have.status('OK')`           | код или текст статуса совпадает                        |
| `to.have.header('X-Id')`, `to.have.header('X-Id', '7')` | заголовок есть (с этим значением)                      |
| `to.have.body()`, `to.have.body('text')`                | тело есть (и равно тексту)                             |
| `to.have.jsonBody()`                                    | тело — JSON                                            |
| `to.have.jsonBody('user.name')`                         | в JSON есть этот путь (синтаксис путей lodash)         |
| `to.have.jsonBody('user.name', 'Ada')`                  | значение по пути равно указанному (глубокое сравнение) |
| `to.be.ok`, `to.be.success`                             | статус 2xx                                             |
| `to.be.json`                                            | тело — JSON                                            |
| `to.be.notFound`                                        | статус 404                                             |
| `to.be.clientError`, `to.be.serverError`                | статус 4xx / 5xx                                       |
| `to.not.have.status(500)`                               | статус другой                                          |
| `to.not.have.header('X-Debug')`                         | заголовка нет                                          |
| `to.not.be.error`                                       | статус меньше 400                                      |

```js
rq.test('с ответом всё в порядке', () => {
  rq.response.to.have.status(200);
  rq.response.to.be.ok;
  rq.response.to.be.json;
  rq.response.to.have.header('X-Request-Id');
  rq.response.to.have.jsonBody('user.name', 'Ada');
  rq.response.to.not.have.header('X-Debug');
});
```

## Списки свойств

У заголовков, параметров запроса и полей формы общий интерфейс. Элемент —
это `{ key, value, disabled }`. Имена сравниваются без учёта регистра;
выключенные элементы не учитываются.

| Метод                               | Описание                                                                            |
| ----------------------------------- | ----------------------------------------------------------------------------------- |
| `get(key)`                          | значение или `undefined`                                                            |
| `has(key)`, `has(key, value)`       | есть ли элемент (с этим значением)                                                  |
| `one(key)`                          | элемент `{ key, value, disabled }`                                                  |
| `all()`                             | включённые элементы                                                                 |
| `count()`                           | число включённых элементов                                                          |
| `add(item)`                         | добавляет `{ key, value }` или `'Key: value'` (`'key=value'` для параметров и форм) |
| `append(item)`                      | то же, что `add`                                                                    |
| `upsert(item)`                      | меняет значение существующего элемента или добавляет новый                          |
| `remove(key)`, `remove(fn)`         | удаляет элементы по имени или по условию                                            |
| `clear()`                           | удаляет всё                                                                         |
| `each(fn)`, `map(fn)`, `filter(fn)` | обход включённых элементов                                                          |
| `toObject()`                        | `{ key: value }`                                                                    |
| `toString()`                        | строки `Key: value`                                                                 |

```js
const h = rq.request.headers;
h.add('X-One: 1');
h.add({ key: 'X-Two', value: '2' });
h.upsert({ key: 'Accept', value: 'application/xml' });
h.remove((item) => item.key.startsWith('X-T'));

console.log(h.get('accept'), h.has('X-One'), h.count());
h.each((item) => console.log(item.key, '=', item.value));
console.log(h.toObject());
```

## Переменные

У `rq.variables`, `rq.environment`, `rq.collectionVariables`, `rq.globals`
и `rq.iterationData` одинаковые методы:

| Метод             | Описание                                                           |
| ----------------- | ------------------------------------------------------------------ |
| `get(key)`        | значение или `undefined`                                           |
| `has(key)`        | задан ли ключ                                                      |
| `set(key, value)` | задаёт значение: строку, число, булево значение, массив или объект |
| `unset(key)`      | удаляет ключ                                                       |
| `clear()`         | удаляет все ключи этого уровня                                     |
| `toObject()`      | все ключи и значения                                               |
| `replaceIn(text)` | заменяет `{{name}}` и динамические переменные в тексте             |

Уровни:

- `rq.variables.get` ищет по всем уровням и возвращает ближайшее значение:
  переменные времени выполнения, строку данных при запуске коллекции,
  окружение, коллекцию, глобальные. `rq.variables.set` создаёт
  **переменную времени выполнения**: она переопределяет все остальные
  уровни для всех запросов коллекции до перезапуска приложения.
- `rq.environment` — активное окружение коллекции (базовое, если активного
  нет).
- `rq.collectionVariables` — базовое окружение коллекции.
- `rq.globals` — активное глобальное окружение пространства (создаётся при
  необходимости).
- `rq.iterationData` — текущая строка файла данных при запуске коллекции
  (только чтение).

```js
rq.environment.set('lastOrderAt', new Date().toISOString());
rq.collectionVariables.set('limits', { perPage: 50, max: 500 });
rq.globals.set('runs', (rq.globals.get('runs') || 0) + 1);

console.log(rq.variables.get('base_url')); // с любого уровня
console.log(rq.variables.has('token'));
console.log(rq.variables.replaceIn('{{base_url}}/users/{{$randomInt}}'));
console.log(Object.keys(rq.environment.toObject()));

rq.environment.unset('lastOrderAt');
```

`replaceIn` заменяет переменные и динамические переменные (`{{$guid}}`,
`{{$timestamp}}`, `{{$faker.person.firstName}}`…); теги шаблонов
(`{% … %}`) остаются как есть.

## Тесты

### rq.test(name, fn)

Регистрирует тест. Если `fn` бросает исключение, тест падает с текстом
ошибки, иначе проходит. `fn` может быть `async`. Результаты видны на
вкладке **Тесты** ответа, при запуске коллекции и в отчётах.
`rq.test.skip(name)` записывает пропущенный тест.

### rq.expect(value)

`expect` из [chai](help:script-libraries#chai): `rq.expect(x).to.equal(1)`,
`.to.eql({…})`, `.to.include('a')`, `.to.have.property('id')`,
`.to.be.above(3)`, `.to.match(/re/)`…

```js
const body = rq.response.json();

rq.test('статус 200', () => rq.response.to.have.status(200));

rq.test('пользователь', () => {
  rq.expect(body.user).to.include({ name: 'Ada' });
  rq.expect(body.user.roles).to.include('admin');
  rq.expect(body.user.email).to.match(/@example\.com$/);
});

rq.test('достаточно быстро', () => {
  rq.expect(rq.response.responseTime).to.be.below(1000);
});

rq.test.skip('ещё не готово');
```

## rq.sendRequest(request, callback?)

Отправляет другой запрос из скрипта и возвращает promise с его ответом
(тот же объект, что `rq.response`). Необязательный callback получает
`(error, response)`.

`request` — строка с адресом или объект:

| Поле                   | Описание                                                                                                                          |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `url`                  | адрес                                                                                                                             |
| `method`               | по умолчанию `GET`                                                                                                                |
| `header` или `headers` | `{ имя: значение }` или `[{ key, value }]`                                                                                        |
| `body`                 | строка, `{ mode: 'raw', raw }`, `{ mode: 'urlencoded', urlencoded: [{ key, value }] }` или обычный объект (отправляется как JSON) |

Запрос отправляется как есть: `{{шаблоны}}` не рендерятся, поэтому
собирайте адрес через `rq.variables.get()` или `rq.variables.replaceIn()`.
Для JSON задайте заголовок `Content-Type` сами. Сетевая ошибка отклоняет
promise (или передаётся в callback); HTTP-статусы ошибок — нет.

```js
const base = rq.variables.get('base_url');

// GET через await
const health = await rq.sendRequest(`${base}/health`);
console.log('health', health.code);

// POST с JSON
const login = await rq.sendRequest({
  url: `${base}/auth/token`,
  method: 'POST',
  header: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    client_id: rq.environment.get('client_id'),
    client_secret: rq.environment.get('client_secret'),
  }),
});
rq.environment.set('token', login.json().access_token);

// В стиле Postman, с callback
rq.sendRequest(`${base}/version`, (err, res) => {
  if (err) console.error(err);
  else console.log('version', res.code);
});
```

## rq.info

| Свойство         | Описание                                 |
| ---------------- | ---------------------------------------- |
| `requestName`    | имя запроса                              |
| `requestId`      | его id                                   |
| `eventName`      | `prerequest` или `test` (after-response) |
| `iteration`      | итерация запуска коллекции, с 0          |
| `iterationCount` | число итераций                           |

```js
console.log(
  `${rq.info.eventName}: ${rq.info.requestName} (№${rq.info.iteration + 1} из ${rq.info.iterationCount})`,
);
```

## rq.execution

| Метод                  | Описание                                                                              |
| ---------------------- | ------------------------------------------------------------------------------------- |
| `skipRequest()`        | pre-request: не отправлять запрос; остальные скрипты всё равно выполняются            |
| `setNextRequest(name)` | запуск коллекции: следующим выполнить запрос с этим именем; `null` завершает итерацию |

```js
// Пропустить запрос, если отправлять нечего
if (!rq.variables.get('orderId')) {
  console.warn('orderId ещё нет, пропускаю');
  rq.execution.skipRequest();
}
```

## rq.faker

[faker-js](help:script-libraries#faker-js) на локали данных из
**Настройки → Шаблоны**: `rq.faker.person.firstName()` или по имени:
`rq.faker('number.int', { min: 1, max: 10 })`. Глобальный `faker` — то же
самое. (`pm.faker` в Postman нет; там используйте `faker`.)

```js
rq.variables.set('name', rq.faker.person.fullName());
rq.variables.set('qty', rq.faker('number.int', { min: 1, max: 10 }));
```

## Глобальные функции

| Глобальное                                             | Описание                                                  |
| ------------------------------------------------------ | --------------------------------------------------------- |
| `console.log/info/warn/error/debug`                    | вывод на вкладке **Консоль**; объекты печатаются как JSON |
| `setTimeout(fn, ms)`, `clearTimeout(id)`               | таймеры; скрипт их дожидается                             |
| `atob(text)`, `btoa(text)`                             | декодирование / кодирование Base64 (текст Latin-1)        |
| `crypto.getRandomValues(array)`, `crypto.randomUUID()` | криптостойкие случайные байты и UUID (Web Crypto)         |
| `require(name)`                                        | [встроенная библиотека](help:script-libraries)            |
| `_`, `CryptoJS`, `moment`, `faker`                     | lodash, crypto-js, moment и faker-js, уже загруженные     |

```js
console.info('объект:', { a: 1, list: [1, 2] });
console.warn('предупреждение');
console.log(btoa('user:password'), atob('dXNlcjpwYXNzd29yZA=='));
setTimeout(() => console.log('позже'), 50);
```

## Имена Postman и Insomnia

| Имя                                                                                    | Замечания                                                                         |
| -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `pm.*`                                                                                 | всё перечисленное выше                                                            |
| `insomnia.*`                                                                           | всё перечисленное выше; `insomnia.baseEnvironment` — это `rq.collectionVariables` |
| `tests['name'] = условие`                                                              | тест (старые скрипты Postman)                                                     |
| `responseBody`, `responseCode.code`, `responseTime`, `responseHeaders`                 | ответ (старые скрипты Postman)                                                    |
| `postman.setEnvironmentVariable`, `getEnvironmentVariable`, `clearEnvironmentVariable` | `rq.environment`                                                                  |
| `postman.setGlobalVariable`, `getGlobalVariable`, `clearGlobalVariable`                | `rq.globals`                                                                      |
| `postman.setNextRequest(name)`                                                         | `rq.execution.setNextRequest`                                                     |

```js
// Тестовый скрипт Postman в старом стиле
var data = JSON.parse(responseBody);
tests['код статуса 200'] = responseCode.code === 200;
tests['есть токен'] = !!data.token;
postman.setEnvironmentVariable('token', data.token);
```
