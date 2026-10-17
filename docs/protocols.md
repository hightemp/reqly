# Protocols

Besides plain HTTP, a request can be GraphQL, a server-sent event stream, a
WebSocket or a gRPC call. Create them from the **Create** menu, the tree's
context menu or the command palette. All of them use variables, template
tags, folder headers and auth like HTTP requests.

## GraphQL

A GraphQL request is an HTTP request with a GraphQL body: the query, the
variables (JSON, templates allowed) and, for documents with several
operations, the operation to run. `POST` sends
`{"query", "variables", "operationName"}` as JSON; `GET` sends them as query
parameters.

- **Schema:** the schema button above the editor fetches it by introspection
  (with the request's URL, headers and auth) or loads it from a file (SDL
  `.graphql` or an introspection `.json`). It is cached per request.
- With a schema, the editor completes fields and arguments and marks
  invalid queries; **Docs** opens an explorer of the types.
- **Prettify** reformats the query.
- **Subscriptions:** if the operation is a subscription, the request
  connects over WebSocket instead of sending (`graphql-transport-ws`, falling
  back to the legacy `graphql-ws`). Use the endpoint's WebSocket URL
  (`ws://` / `wss://` or `http(s)://`, converted automatically). Headers are
  sent with the handshake and in `connection_init`.

## Server-sent events

An event stream request keeps the connection open and shows each event
(type, id and data) as it arrives. If the server closes the stream, it
reconnects after the server's `retry` delay (3 s by default) with
`Last-Event-ID`; a `204` answer or a response that is not
`text/event-stream` stops it. SSE runs on the same engine as HTTP, so proxy,
certificate and HTTP version settings apply.

## WebSocket

Connect, then send text, JSON (validated before sending) or binary (base64)
messages with **Send** or Ctrl+Enter. Messages can be saved and reused.
Settings: subprotocols, automatic reconnection after an abnormal close and
its delay, TLS verification and timeouts. Auth and headers apply to the
handshake.

## gRPC

- **Address:** `host:port` or `grpc://host:port` for plaintext,
  `grpcs://host:port` for TLS.
- **Services:** add `.proto` files (with the files they import, named as in
  their `import` statements) in the **Proto** tab, or turn on **server
  reflection**. On desktop you can also list import directories.
- Pick the method; **Fill example** writes a sample message with every
  field, and the editor completes field names of the request type.
  Messages are proto3 JSON.
- **Unary** and **server streaming** calls send the message when invoked.
  For **client streaming** and **bidirectional** calls, invoke first, then
  send messages one by one and **End stream**.
- Metadata (`-bin` keys take base64), auth, TLS settings and the request
  timeout (as the deadline) apply. The log shows response messages, headers,
  the final status and trailers.

## Connections

WebSocket, SSE, GraphQL subscription and gRPC requests show a connection log
instead of a response: sent and received messages with times and sizes,
lifecycle entries (connected, closed, reconnecting, errors), filters by
text, direction and SSE event type, a viewer for the selected message
(JSON is formatted) and the handshake headers. A request has one connection
at a time; connecting again replaces it. The log keeps the last 5000
messages and is not stored in the history.

## Server mode

On the server, WebSocket and gRPC targets are checked against the network
policy like HTTP requests, and the connection goes to the checked address.
Local `.proto` files and import paths are not available; add the files to the
request instead.

## Import and export

Insomnia GraphQL, WebSocket and gRPC requests are imported (the gRPC
`.proto` file when it is in the export) and exported; Postman and Bruno
GraphQL bodies become GraphQL requests. Postman collections cannot hold
WebSocket or gRPC requests, so they are left out of Postman exports.
