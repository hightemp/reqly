# Protocols

Create GraphQL, WebSocket, SSE and gRPC requests from **Create** or the
tree's context menu. They use variables, folder headers and auth like HTTP
requests.

## GraphQL

Write the query and its variables (JSON). The schema button loads the schema
by introspection or from a file; with it the editor completes fields and
**Docs** shows the types. Subscriptions connect over WebSocket.

## Server-sent events

An event stream request keeps the connection open and shows each event as
it arrives. It reconnects when the server closes the stream.

## WebSocket

**Connect**, then send text, JSON or binary messages (`Ctrl+Enter`). Save
messages you send often. Subprotocols, reconnection and timeouts are in the
settings.

## gRPC

- Address: `host:port` (plaintext) or `grpcs://host:port` (TLS).
- Add `.proto` files in the **Proto** tab or turn on server reflection.
- Pick the method; **Fill example** writes a sample message.
- Unary and server streaming calls send when invoked. For client streaming
  and bidirectional calls, invoke, send messages one by one, then **End
  stream**.

## The connection log

Streaming requests show a log instead of a response: messages sent and
received with times and sizes, connection events, filters and a viewer for
the selected message.
