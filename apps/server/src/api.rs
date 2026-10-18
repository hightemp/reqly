use axum::Extension;
use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use std::collections::HashSet;

use reqly_api::access::{Caller, event_visible};
use reqly_api::commands::dispatch_as;
use reqly_core::{CoreError, Id};
use reqly_storage::users::MemberRepo;
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
            "access.denied" => StatusCode::FORBIDDEN,
            "command.invalid_input" => StatusCode::BAD_REQUEST,
            "internal" | "storage.error" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        (status, axum::Json(self.0)).into_response()
    }
}

async fn command(
    State(state): State<AppState>,
    caller: Option<Extension<Caller>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Result<axum::Json<Value>, ApiError> {
    let caller = caller.map(|Extension(c)| c).unwrap_or_default();
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
    let output = dispatch_as(&state.registry, &state.ctx, caller, &name, input)
        .await
        .map_err(ApiError)?;
    Ok(axum::Json(output))
}

#[derive(serde::Deserialize)]
struct WsQuery {
    /// The browser tab, for presence (`presence.set`).
    client: Option<String>,
}

async fn websocket(
    State(state): State<AppState>,
    caller: Option<Extension<Caller>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let caller = caller.map(|Extension(c)| c).unwrap_or_default();
    ws.on_upgrade(move |socket| async move {
        push_events(state.clone(), caller, socket).await;
        // The tab is gone: so is its presence.
        if let Some(client) = query.client {
            state.ctx.presence.leave(&state.ctx, &client);
        }
    })
}

/// The workspaces of a user, for filtering events.
async fn memberships(state: &AppState, caller: Caller) -> HashSet<Id> {
    let Some(user) = caller.user_id() else {
        return HashSet::new();
    };
    MemberRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .workspaces_of(user)
        .await
        .map(|m| m.into_keys().collect())
        .unwrap_or_default()
}

/// Pushes the core events the caller may see to the socket until either
/// side closes (ADR-010).
async fn push_events(state: AppState, caller: Caller, mut socket: WebSocket) {
    let mut events = state.ctx.events.subscribe();
    let mut member_of = memberships(&state, caller).await;
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
                    if event.topic == "workspaces.changed" {
                        member_of = memberships(&state, caller).await;
                    }
                    if !event_visible(&state.ctx.storage, caller, &member_of, &event).await {
                        continue;
                    }
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
