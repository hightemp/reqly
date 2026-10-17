# ADR-006: Request protocols

- Status: accepted
- Date: 2026-10-17

## Context

Stage 5 adds GraphQL, WebSocket, server-sent events (SSE) and gRPC
(PRD §7.7). Requests of every protocol must live in the same tree, inherit
folder headers and auth, use templates, import and export, and work the same
on desktop and server.

## Options

1. One node kind per protocol (`graphql`, `sse`, `webSocket`, `grpc`).
2. Protocols as variations of an HTTP request where the wire is HTTP, and
   separate node kinds only where it is not.

## Decision

Option 2:

- **GraphQL** is an HTTP body kind (`body.kind = graphql` with query,
  variables and operation name). It is sent as JSON (or query parameters for
  `GET`), so history, cookies, codegen, cURL and HAR keep working.
  Subscriptions (detected from the operation) run over WebSocket with
  `graphql-transport-ws`, falling back to the legacy `graphql-ws`.
- **SSE** is an HTTP request with `eventStream: true`. It runs on the libcurl
  engine (proxies, certificates, HTTP versions) with a streaming body sink,
  a WHATWG parser and reconnects with `Last-Event-ID`.
- **WebSocket** and **gRPC** are node kinds (`NodeData::WebSocket`,
  `NodeData::Grpc`). Their handshake / metadata are converted to an HTTP
  request for preparation, so templates, folder headers, auth (including
  signing schemes) and prompts apply unchanged.
- Long-lived exchanges are **connections** (`stream.*` commands): one per
  request, a bounded message log in the core and `stream.message` /
  `stream.status` events, which reach the web UI over the existing WebSocket.
- The tree stores a `protocol` column for badges (GQL, SSE, WS, gRPC).
- Protocol clients live in `reqly-protocols` (tokio-tungstenite + rustls,
  tonic with a dynamic codec, prost-reflect, protox), separate from the HTTP
  engine.

## Consequences

- WebSocket and gRPC do not go through libcurl: proxies configured for HTTP do
  not apply to them yet. On the server the network policy checks their target
  and the connection is made to the checked address.
- `.proto` sources are stored inline in the request (or as uploaded blobs on
  the server), so collections stay portable; import paths are desktop-only.
- Connection logs are not part of the response history.
