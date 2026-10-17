# ADR-002: HTTP-движок

- Статус: принято
- Дата: 2026-10-17
- Спайк: `spikes/http-engine` (`cargo test -p spike-http-engine -- --nocapture`)

## Контекст

Нужны (PRD §5.5, §7.5–7.7): timeline с сырыми запросом и ответом, метрики фаз
(DNS, connect, TLS, TTFB), HTTP/1.1 и HTTP/2, прокси HTTP, HTTPS и SOCKS5,
клиентские сертификаты, кастомный CA, Digest, NTLM, AWS SigV4, отмена запроса,
сборка под Linux, Windows, macOS и Android.

## Варианты

1. **libcurl** (`curl` 0.4 / `curl-sys`, libcurl 8.22, статическая сборка с vendored OpenSSL)
2. **reqwest 0.13 / hyper** (rustls)

## Результаты спайка

Обе реализации проверены одним набором тестов против `reqly-testserver`.

| Критерий | libcurl | reqwest |
|---|---|---|
| Базовый запрос, заголовки, тело | ✅ | ✅ |
| Timeline (сырые заголовки, TLS-handshake, ALPN, редиректы) | ✅ `CURLOPT_DEBUGFUNCTION`, вывод совпадает с Insomnia | ❌ нужен собственный connector и обёртка над hyper |
| Фазы времени DNS / connect / TLS / TTFB | ✅ `*_TIME` | ⚠️ только TTFB и total |
| Число редиректов | ✅ | ⚠️ вручную через policy |
| HTTP/2 через ALPN | ✅ (`nghttp2`) | ✅ |
| mTLS из PEM-строк, CA из PEM | ✅ `*_BLOB` | ✅ |
| Отмена в полёте | ✅ через progress-callback, < 1 с | ✅ drop future |
| Digest / NTLM | ✅ встроены (`ntlm` feature) | ❌ сторонние crates или своя реализация |
| AWS SigV4 | ⚠️ `CURLOPT_AWS_SIGV4` нет в safe-API, используем `aws-sigv4` и подписываем сами | то же |
| Прокси HTTP / HTTPS / SOCKS4/5 + auth, `no_proxy` | ✅ | ✅ (кроме SOCKS4a/HTTPS-proxy частично) |
| HTTP/3 | ❌ в статической сборке нет (нужен ngtcp2/quiche) | ⚠️ экспериментально |
| Размер probe-бинарника (Linux x86_64, release, strip) | 7.0 МБ | 12.7 МБ |
| Кросс-компиляция Android aarch64 (NDK 28) | ✅ | ✅ |
| Windows / macOS | не проверялось локально; проверяется матрицей CI при появлении `reqly-http` | — |

## Решение

**libcurl** через трейт `HttpEngine` в будущем crate `reqly-http`:

- Фичи `curl`: `ssl`, `static-curl`, `static-ssl`, `http2`, `ntlm`. Без системных
  зависимостей в рантайме.
- `Easy2` выполняется в blocking-пуле tokio (`spawn_blocking`). Отмена работает
  через атомарный флаг, который проверяет progress/xferinfo-callback. При большом
  числе параллельных запросов (Runner, этап 9) можно перейти на `Multi`, не меняя
  трейт.
- **Прокси задаются только явно.** libcurl по умолчанию читает `http_proxy` и
  `https_proxy` из окружения. Движок всегда выставляет `CURLOPT_PROXY` и
  `CURLOPT_NOPROXY` из настроек Reqly; режим «системный прокси» читает окружение
  или ОС осознанно.
- AWS SigV4, OAuth и прочие подписи считаются в Rust до передачи запроса в движок.
- HTTP/3 откладывается (этап 4, «опционально»). Если понадобится, сначала
  оценим отдельную сборку libcurl с ngtcp2.

## Последствия

- Timeline и метрики «как в Insomnia» достаются почти бесплатно.
- C-зависимости (libcurl, OpenSSL, nghttp2, zlib) собираются из исходников, что
  увеличивает время чистой сборки. Под Android сборка подтверждена.
- `reqwest` остаётся в dev-зависимостях тестов и, возможно, во внутренних
  клиентах (OAuth token endpoint), но не используется как движок пользовательских
  запросов.
