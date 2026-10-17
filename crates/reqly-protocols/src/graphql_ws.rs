//! GraphQL subscriptions over WebSocket: the `graphql-transport-ws`
//! protocol (graphql-ws library) and the legacy `graphql-ws` protocol
//! (subscriptions-transport-ws), chosen by subprotocol negotiation.

use std::time::Duration;

use reqly_core::{CoreError, CoreResult};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::ws::{self, WsConfig, WsEvent, WsFrame, WsSender};

pub const TRANSPORT_WS: &str = "graphql-transport-ws";
pub const LEGACY_WS: &str = "graphql-ws";

#[derive(Debug, Clone, PartialEq)]
pub enum GqlEvent {
    /// The server accepted the connection and the subscription started.
    Started { protocol: String },
    /// One result (`{ data, errors }`).
    Next(Value),
    /// The operation failed (GraphQL errors).
    Error(Value),
    /// The server completed the subscription.
    Complete,
    /// The connection ended.
    Closed {
        code: Option<u16>,
        reason: String,
        error: Option<String>,
    },
}

pub struct Subscription {
    sender: WsSender,
    legacy: bool,
}

impl Subscription {
    /// Stops the subscription and closes the connection.
    pub fn stop(&self) {
        let stop = if self.legacy {
            json!({ "id": "1", "type": "stop" })
        } else {
            json!({ "id": "1", "type": "complete" })
        };
        let _ = self.sender.send(WsFrame::Text(stop.to_string()));
        self.sender.close(1000, "");
    }
}

fn gql_error(message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new("graphql.subscription", message.clone()).with_param("message", message)
}

/// Connects and subscribes. `payload` is `{query, variables?, operationName?}`.
pub async fn subscribe(
    mut config: WsConfig,
    init_payload: Option<Value>,
    payload: Value,
) -> CoreResult<(Subscription, mpsc::UnboundedReceiver<GqlEvent>)> {
    config.subprotocols = vec![TRANSPORT_WS.into(), LEGACY_WS.into()];
    let (handshake, sender, mut events) = ws::connect(config).await?;
    let protocol = handshake
        .protocol
        .clone()
        .unwrap_or_else(|| TRANSPORT_WS.into());
    let legacy = protocol == LEGACY_WS;

    let mut init = json!({ "type": "connection_init" });
    if let Some(p) = init_payload {
        init["payload"] = p;
    }
    sender.send(WsFrame::Text(init.to_string()))?;

    // Wait for connection_ack.
    let ack = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = events.recv().await {
            match event {
                WsEvent::Message(WsFrame::Text(text)) => {
                    let msg: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                    match msg["type"].as_str() {
                        Some("connection_ack") => return Ok(()),
                        Some("connection_error") => {
                            return Err(gql_error(format!(
                                "connection refused: {}",
                                msg["payload"]
                            )));
                        }
                        Some("ping") => {
                            let _ =
                                sender.send(WsFrame::Text(json!({ "type": "pong" }).to_string()));
                        }
                        _ => {}
                    }
                }
                WsEvent::Closed {
                    code,
                    reason,
                    error,
                } => {
                    return Err(gql_error(format!(
                        "the server closed the connection ({}) {reason}{}",
                        code.map(|c| c.to_string()).unwrap_or_default(),
                        error.map(|e| format!(": {e}")).unwrap_or_default()
                    )));
                }
                _ => {}
            }
        }
        Err(gql_error("the connection ended"))
    })
    .await
    .map_err(|_| gql_error("no connection_ack from the server"))?;
    ack?;

    let start = json!({ "id": "1", "type": if legacy { "start" } else { "subscribe" }, "payload": payload });
    sender.send(WsFrame::Text(start.to_string()))?;

    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(GqlEvent::Started {
        protocol: protocol.clone(),
    });
    let pong = sender.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let out = match event {
                WsEvent::Message(WsFrame::Text(text)) => {
                    let msg: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                    match msg["type"].as_str() {
                        Some("next" | "data") => GqlEvent::Next(msg["payload"].clone()),
                        Some("error") => GqlEvent::Error(msg["payload"].clone()),
                        Some("complete") => {
                            let _ = tx.send(GqlEvent::Complete);
                            pong.close(1000, "");
                            continue;
                        }
                        Some("ping") => {
                            let _ = pong.send(WsFrame::Text(json!({ "type": "pong" }).to_string()));
                            continue;
                        }
                        _ => continue,
                    }
                }
                WsEvent::Closed {
                    code,
                    reason,
                    error,
                } => {
                    let _ = tx.send(GqlEvent::Closed {
                        code,
                        reason,
                        error,
                    });
                    return;
                }
                _ => continue,
            };
            if tx.send(out).is_err() {
                pong.close(1000, "");
                return;
            }
        }
    });
    Ok((Subscription { sender, legacy }, rx))
}
