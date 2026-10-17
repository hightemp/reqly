use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use reqly_core::CoreError;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/cmd/{name}", post(command))
        .route("/ws", get(websocket))
        .route("/{*rest}", any(|| async { StatusCode::NOT_FOUND }))
}

struct ApiError(CoreError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "command.unknown" | "not_found" => StatusCode::NOT_FOUND,
            "command.invalid_input" => StatusCode::BAD_REQUEST,
            "internal" | "storage.error" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        (status, axum::Json(self.0)).into_response()
    }
}

async fn command(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Bytes,
) -> Result<axum::Json<Value>, ApiError> {
    let input: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).map_err(|e| {
            ApiError(
                CoreError::new("command.invalid_input", e.to_string())
                    .with_param("name", name.as_str()),
            )
        })?
    };
    let output = state
        .registry
        .dispatch(&state.ctx, &name, input)
        .await
        .map_err(ApiError)?;
    Ok(axum::Json(output))
}

async fn websocket(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| push_events(state, socket))
}

/// Pushes every core event to the socket until either side closes.
async fn push_events(state: AppState, mut socket: WebSocket) {
    let mut events = state.ctx.events.subscribe();
    let hello = json!({ "type": "hello", "version": env!("CARGO_PKG_VERSION") });
    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    let msg = json!({ "type": "event", "event": event });
                    if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    // The client must resync state; tell it how much it missed.
                    let msg = json!({ "type": "lagged", "skipped": skipped });
                    if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}
