# ADR-006: JavaScript engine for scripts

- Status: accepted
- Date: 2026-10-18

## Context

Stage 6 runs pre-request and after-response scripts (PRD §7.15) with
Postman (`pm.*`) and Insomnia (`insomnia.*`) compatibility. Scripts are
untrusted (imported collections), so the engine needs hard time and memory
limits and no ambient access to files or the network. It must build for
Linux, Windows, macOS and Android and stay small. PRD §5.6 compared
rquickjs (QuickJS), Boa and deno_core (V8) and recommended rquickjs, to be
confirmed by a spike.

## Spike (rquickjs 0.14, release build, x86_64 Linux)

| Check | Result |
| --- | --- |
| Time limit (`while (true) {}`, interrupt handler, 200 ms) | interrupted after 200 ms; the runtime stays usable |
| Memory limit (32 MB, growing arrays) | `out of memory` error after 23 ms |
| Async host function (`await` of a tokio future) | works (`Async` wrapper) |
| Ambient APIs | `require`, `process`, `fetch`, `setTimeout` are undefined |
| New runtime + context + trivial script | 0.1 ms |
| chai + lodash + crypto-js + moment from source, per run | ~37 ms |
| The same libraries from precompiled bytecode | 0.6–1.8 ms each (≈5 ms for all four) |
| Compiling the four libraries to bytecode (once per process) | ~35 ms |
| Binary size (stripped, with tokio, vs tokio alone) | +1.3 MB |
| `aarch64-linux-android` (NDK 28, API 24) | builds with the `bindgen` feature (no bundled Android bindings) |

Boa was not measured: it has no hard memory limit (only loop and
recursion limits), which rules it out for untrusted scripts. deno_core adds
tens of MB and has no prebuilt Android V8.

## Decision

rquickjs:

- one runtime and context per script run, with a memory limit (64 MB) and an
  interrupt handler for the time limit (5 s by default, PRD §7.15); time spent
  waiting for `sendRequest` does not count;
- library modules (chai, lodash, crypto-js, moment) are compiled to bytecode
  once per process and loaded lazily on `require` or first use;
- `rquickjs` gets the `bindgen` feature only on Android
  (`[target.'cfg(target_os = "android")'.dependencies]`); Android builds need
  libclang and `BINDGEN_EXTRA_CLANG_ARGS_<target>=--sysroot=<ndk sysroot>`.

## Consequences

- Scripts run in the core (Rust), so desktop, server and the future CLI
  behave the same.
- QuickJS is slower than V8 on heavy computation, which request scripts do
  not need.
- The Android build (stage 8) must provide libclang to bindgen.
