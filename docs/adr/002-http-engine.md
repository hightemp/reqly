# ADR-002: HTTP engine

- Status: accepted
- Date: 2026-10-17
- Spike: `spikes/http-engine` (`cargo test -p spike-http-engine -- --nocapture`)

## Context

Requirements (PRD §5.5, §7.5–7.7): a timeline with the raw request and
response, phase timings (DNS, connect, TLS, TTFB), HTTP/1.1 and HTTP/2, HTTP,
HTTPS and SOCKS5 proxies, client certificates, custom CAs, Digest, NTLM, AWS
SigV4, cancellation, and builds for Linux, Windows, macOS and Android.

## Options

1. **libcurl** (`curl` 0.4 / `curl-sys`, libcurl 8.22, static build with vendored OpenSSL)
2. **reqwest 0.13 / hyper** (rustls)

## Spike results

Both implementations ran the same test suite against `reqly-testserver`.

| Criterion | libcurl | reqwest |
|---|---|---|
| Basic request, headers, body | ✅ | ✅ |
| Timeline (raw headers, TLS handshake, ALPN, redirects) | ✅ `CURLOPT_DEBUGFUNCTION`, output matches Insomnia | ❌ needs a custom connector and a hyper wrapper |
| DNS / connect / TLS / TTFB timings | ✅ `*_TIME` | ⚠️ only TTFB and total |
| Redirect count | ✅ | ⚠️ manually via a policy |
| HTTP/2 via ALPN | ✅ (`nghttp2`) | ✅ |
| mTLS from PEM strings, CA from PEM | ✅ `*_BLOB` | ✅ |
| Cancelling in flight | ✅ via the progress callback, < 1 s | ✅ drop the future |
| Digest / NTLM | ✅ built in (`ntlm` feature) | ❌ third-party crates or our own code |
| AWS SigV4 | ⚠️ `CURLOPT_AWS_SIGV4` is not in the safe API; we sign with `aws-sigv4` ourselves | same |
| HTTP / HTTPS / SOCKS4/5 proxies with auth, `no_proxy` | ✅ | ✅ (SOCKS4a / HTTPS proxies only partly) |
| HTTP/3 | ❌ not in the static build (needs ngtcp2/quiche) | ⚠️ experimental |
| Probe binary size (Linux x86_64, release, stripped) | 7.0 MB | 12.7 MB |
| Cross-compiling for Android aarch64 (NDK 28) | ✅ | ✅ |
| Windows / macOS | not checked locally; covered by the CI matrix once `reqly-http` exists | — |

## Decision

**libcurl** behind an `HttpEngine` trait in the `reqly-http` crate:

- `curl` features: `ssl`, `static-curl`, `static-ssl`, `http2`, `ntlm`. No
  system dependencies at runtime.
- `Easy2` runs in tokio's blocking pool (`spawn_blocking`). Cancellation uses
  an atomic flag checked by the progress/xferinfo callback. With many
  parallel requests (Runner, stage 9) we can move to `Multi` without changing
  the trait.
- **Proxies are always set explicitly.** By default libcurl reads
  `http_proxy` and `https_proxy` from the environment. The engine always sets
  `CURLOPT_PROXY` and `CURLOPT_NOPROXY` from Reqly's settings; the "system
  proxy" mode reads the environment or the OS on purpose.
- AWS SigV4, OAuth and other signatures are computed in Rust before the
  request reaches the engine.
- HTTP/3 is postponed (stage 4, optional). If needed, a separate libcurl
  build with ngtcp2 will be evaluated first.

## Consequences

- An Insomnia-like timeline and timings come almost for free.
- C dependencies (libcurl, OpenSSL, nghttp2, zlib) are built from source,
  which makes clean builds slower. The Android build is confirmed.
- `reqwest` stays in test dev-dependencies and possibly in internal clients
  (OAuth token endpoint), but is not the engine for user requests.
