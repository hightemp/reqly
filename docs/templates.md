# Шаблоны, переменные и окружения

Шаблоны работают в URL, query- и path-параметрах, заголовках, теле запроса
(текст и поля форм), полях авторизации, а также в значениях переменных.
Рендеринг выполняет ядро на Rust при отправке, поэтому desktop и сервер дают
одинаковый результат. В историю попадает уже отрендеренный запрос.

## Синтаксис

| Запись | Значение |
| --- | --- |
| `{{ name }}` | переменная |
| `{{ api.version }}`, `{{ hosts[0] }}`, `{{ _['odd key'] }}` | путь внутри JSON-значения |
| `{{ _.name }}` | то же, что `{{ name }}` (совместимость с Insomnia) |
| `{% uuid 'v4' %}` | тег (функция) в стиле Insomnia |
| `{{ $uuid('v7') }}` | та же функция в виде вызова |
| `{{ $faker.person.firstName }}` | генератор тестовых данных |
| `{{ $guid }}`, `{{ $randomEmail }}` | динамические переменные Postman |
| `{# комментарий #}` | не попадает в результат |
| `\{{` | буквальные `{{` |

Аргументы: строки в одинарных или двойных кавычках (`\'` — экранирование),
числа, `true`/`false`/`null`, переменные и вложенные вызовы:
`{% hash 'sha256', 'hex', token %}`, `{% base64 'encode', $now('unix') %}`.

Значения переменных тоже могут содержать шаблоны:
`base_url = {{ host }}/api/{{ version }}`. Циклы (`a → b → a`) дают ошибку
`template.cycle`. Неопределённая переменная останавливает отправку с ошибкой
`template.undefined`. В редакторе такие переменные подсвечиваются красным.

Значение вызова фиксируется на одну отправку: `{{ $uuid }}` в URL и в
заголовке даст один и тот же UUID.

## Откуда берутся переменные

Порядок применения, каждый следующий уровень переопределяет предыдущий:

1. глобальное окружение workspace (активное);
2. базовое окружение коллекции;
3. активное окружение коллекции (dev, stage, prod…);
4. переменные папок, от внешней к внутренней;
5. переменные запроса.

Переопределение идёт по ключам верхнего уровня. Окружения редактируются в
окне **Environments** (`Ctrl+E`) таблицей или как JSON. Активное окружение
переключается в шапке сайдбара. Переменные папки задаются в её настройках,
переменные запроса — на вкладке **Variables**.

### Секреты

Ключ окружения можно пометить как секрет (замок в строке):

- значение маскируется в таблице и в подсказке при наведении;
- в базе данных значение хранится зашифрованным (XChaCha20-Poly1305), ключ
  лежит в файле `secret.key` рядом с базой (доступ только владельцу);
- без `secret.key` секреты расшифровать нельзя: при переносе данных
  копируйте его вместе с `reqly.db`.

## Функции

| Функция | Аргументы | Пример |
| --- | --- | --- |
| `uuid` | `v4` (по умолчанию), `v7` | `{% uuid 'v7' %}` |
| `now` | `iso-8601`, `unix`, `millis`, `rfc-2822`, `custom` + формат strftime | `{% now 'custom', '%Y-%m-%d' %}` |
| `random` | `int` min max, `float` min max, `string` длина [алфавит], `regex` шаблон | `{% random 'regex', '[A-Z]{3}-\d{4}' %}` |
| `base64` | `encode`/`decode`, значение, [`url`] | `{% base64 'encode', 'user:pass' %}` |
| `urlencode` | `encode`/`decode`, значение | `{% urlencode 'encode', 'a b' %}` |
| `hash` | `md5`/`sha1`/`sha256`/`sha384`/`sha512`, `hex`/`base64`/`base64url`, значение | `{% hash 'sha256', 'hex', body %}` |
| `hmac` | алгоритм, кодировка, ключ, значение | `{% hmac 'sha256', 'hex', secret, payload %}` |
| `jsonpath` | JSON (значение или строка), выражение | `{% jsonpath user, '$.roles[0]' %}` |
| `jwt` | `encode` payload ключ [алгоритм]; `decode` токен [`header`/`payload`] | `{% jwt 'encode', '{"sub":"1"}', secret, 'HS256' %}` |
| `faker` | имя функции, аргументы | `{% faker 'number.int', 1, 10 %}` |
| `cookie` | URL, имя | `{% cookie 'https://api.example.com', 'sid' %}` |
| `request` | `url`, `method`, `name`, `id`, `folder`, `header` имя | `{% request 'header', 'Accept' %}` |
| `response` | поле, id запроса, путь, триггер, maxAge (с) | см. ниже |
| `prompt` | заголовок, подпись, значение по умолчанию, ключ | `{% prompt 'OTP', 'Код из SMS' %}` |
| `file` | путь (desktop) или хеш загруженного файла (сервер) | `{% file '/tmp/token.txt' %}` |
| `os` | `platform`, `arch`, `hostname`, `username`, `env` имя | `{% os 'env', 'HOME' %}` |

Наведите курсор на тег, чтобы увидеть его значение, и нажмите **Edit…**
(или `Ctrl`+клик), чтобы изменить аргументы в форме с живым превью.

### Цепочки ответов

`{% response 'body', '<id запроса>', '$.token', 'no-history', 300 %}`

- поле: `body` (путь `$…` — JSONPath, `/…` — XPath, пусто — всё тело),
  `header` (путь — имя заголовка), `raw`, `url`, `status`;
- триггер: `never` — взять последний ответ; `no-history` — отправить, если
  ответов нет; `when-expired` — отправить, если последний ответ старше maxAge
  секунд; `always` — отправлять каждый раз;
- взаимные зависимости запросов дают ошибку `template.response_cycle`.

### prompt

Перед отправкой Reqly собирает все `prompt` запроса, его папок и
переменных и показывает одно окно со всеми полями. Значения можно запомнить
до конца сессии.

### Ограничения сервера

В серверной версии `file` читает только загруженные через UI файлы, а
`os 'hostname'`, `os 'username'` и `os 'env'` недоступны: шаблоны
выполняются на сервере и не должны раскрывать его окружение.

## Faker

Имена функций как в faker-js. Язык данных — English или Русский
(**Settings → Templates**), его можно переопределить для коллекции.
Русские данные учитывают род (Иванов / Иванова), добавляют отчества,
телефоны `+7` и шестизначные индексы.

| Категория | Функции |
| --- | --- |
| `person` | `firstName`, `lastName`, `middleName`, `fullName`, `prefix`, `sex`, `jobTitle` |
| `internet` | `email`, `username`, `password(length?)`, `url`, `domainName`, `ipv4`, `ipv6`, `mac`, `port`, `userAgent` |
| `location` | `city`, `street`, `streetAddress`, `state`, `country`, `zipCode`, `latitude`, `longitude` |
| `phone` | `number` |
| `company` | `name`, `catchPhrase` |
| `lorem` | `word`, `words(count?)`, `sentence`, `sentences(count?)`, `paragraph`, `slug` |
| `number` | `int(min?, max?)`, `float(min?, max?, precision?)` |
| `string` | `uuid`, `alpha(length?)`, `alphanumeric(length?)`, `numeric(length?)` |
| `datatype` | `boolean` |
| `date` | `past`, `future`, `recent`, `soon`, `birthdate`, `anytime`, `month`, `weekday` |
| `color` | `human`, `rgb` |
| `commerce` | `product`, `productName`, `price`, `department` |
| `finance` | `amount`, `accountNumber`, `iban`, `currencyCode`, `creditCardNumber`, `bic` |
| `image` | `url`, `avatar` |

### Динамические переменные Postman

`$guid`, `$randomUUID`, `$timestamp`, `$isoTimestamp`, `$randomInt` и
`$random*` (`$randomFirstName`, `$randomEmail`, `$randomCity`,
`$randomPrice`, `$randomIP`…) сводятся к функциям выше.
