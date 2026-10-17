//! WebSocket and server-sent event endpoints.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::Query;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// `/ws/echo`: echoes text and binary frames. Text `close:<code>:<reason>`
/// makes the server close; `headers` answers the handshake headers as JSON.
/// Accepts the `echo` and `chat` subprotocols.
pub async fn ws_echo(ws: WebSocketUpgrade, headers: HeaderMap) -> Response {
    let handshake: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    ws.protocols(["echo", "chat"])
        .on_upgrade(move |socket| echo(socket, handshake))
}

async fn echo(mut socket: WebSocket, handshake: HashMap<String, String>) {
    while let Some(Ok(msg)) = socket.recv().await {
        let reply = match msg {
            Message::Text(text) => {
                if let Some(rest) = text.strip_prefix("close:") {
                    let (code, reason) = rest.split_once(':').unwrap_or((rest, ""));
                    let _ = socket
                        .send(Message::Close(Some(CloseFrame {
                            code: code.parse().unwrap_or(1000),
                            reason: reason.to_string().into(),
                        })))
                        .await;
                    return;
                }
                if text.as_str() == "headers" {
                    Message::Text(json!(handshake).to_string().into())
                } else {
                    Message::Text(text)
                }
            }
            Message::Binary(bytes) => Message::Binary(bytes),
            Message::Close(_) => return,
            _ => continue,
        };
        if socket.send(reply).await.is_err() {
            return;
        }
    }
}

/// `/ws/ticker?n=3`: sends `tick 1`..`tick n` after connecting, then closes (1000).
pub async fn ws_ticker(ws: WebSocketUpgrade, Query(q): Query<HashMap<String, String>>) -> Response {
    let n: u32 = q.get("n").and_then(|v| v.parse().ok()).unwrap_or(3);
    ws.on_upgrade(move |mut socket| async move {
        for i in 1..=n {
            if socket
                .send(Message::Text(format!("tick {i}").into()))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "done".into(),
            })))
            .await;
    })
}

fn sse_response(body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

/// `/sse/events?n=3`: `n` events (id, event type, multi-line data) and a retry hint.
pub async fn sse_events(Query(q): Query<HashMap<String, String>>) -> Response {
    let n: u32 = q.get("n").and_then(|v| v.parse().ok()).unwrap_or(3);
    let mut body = String::from(": comment\nretry: 50\n\n");
    for i in 1..=n {
        let kind = if i % 2 == 0 { "even" } else { "odd" };
        body.push_str(&format!(
            "id: {i}\nevent: {kind}\ndata: line {i}\ndata: second\n\n"
        ));
    }
    sse_response(body)
}

/// `/sse/resume`: two events per connection, continuing after `Last-Event-ID`;
/// stops (204) after event 4.
pub async fn sse_resume(headers: HeaderMap) -> Response {
    let last: u32 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if last >= 4 {
        return StatusCode::NO_CONTENT.into_response();
    }
    let mut body = String::from("retry: 20\n\n");
    for i in last + 1..=last + 2 {
        body.push_str(&format!("id: {i}\ndata: {i}\n\n"));
    }
    sse_response(body)
}

/// `/sse/slow`: one event every 50 ms until the client disconnects.
pub async fn sse_slow() -> Response {
    let stream = futures::stream::unfold(1u64, |i| async move {
        if i > 1 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Some((
            Ok::<_, Infallible>(Bytes::from(format!("id: {i}\ndata: tick {i}\n\n"))),
            i + 1,
        ))
    });
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(stream),
    )
        .into_response()
}
