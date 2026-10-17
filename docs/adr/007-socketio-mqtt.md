# ADR-007: Socket.IO and MQTT

- Status: proposed (PRD Q4 is open; this is the recommendation)
- Date: 2026-10-17

## Context

PRD §7.7 lists Socket.IO and MQTT as optional, to be decided after stage 5
(question Q4). Insomnia supports Socket.IO; Postman supports Socket.IO and
MQTT.

## Findings

- **Socket.IO** runs on Engine.IO over WebSocket (or HTTP long polling).
  With the stage 5 WebSocket client it needs: the Engine.IO v4 handshake
  (`0{…}` open packet, ping/pong), Socket.IO packet encoding (`40` connect,
  `42["event", …]` events, acknowledgements, namespaces) and binary
  attachments. Most of the UI (log, composer, saved messages) is shared with
  WebSocket; the composer needs an event name and arguments. Estimate: small
  (days), mostly protocol framing and tests against a Socket.IO server.
  Raw WebSocket requests to `/socket.io/?EIO=4&transport=websocket` already
  work for simple cases today.
- **MQTT** is a different model: a broker connection with subscriptions to
  topic filters, QoS 0–2, retained messages, last will, MQTT 3.1.1 and 5
  properties, TCP/TLS or WebSocket transports. It needs a client library
  (`rumqttc`), its own request kind and a subscription manager in the UI.
  Estimate: medium (a week or more). It is mainly used for IoT, outside the
  core HTTP/API audience.

## Recommendation

1. Add **Socket.IO** as a WebSocket variant in a later minor release (or the
   start of stage 6), reusing the connection log and composer.
2. Do not build **MQTT** into the core now; revisit it with the plugin system
   (PRD backlog) or if users ask for it.

## Consequences

- Q4 stays open until the product decision; TASKS T5.5.2 is done when this
  ADR is accepted or changed.
