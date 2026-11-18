//! The Streamable HTTP endpoint: `POST /mcp`, one JSON-RPC message per request.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use reqly_api::commands::AppContext;
use reqly_api::registry::Registry;
use serde_json::Value;

use crate::protocol::{self, INVALID_REQUEST, PARSE_ERROR, Reply, SUPPORTED_VERSIONS};
use crate::tools::{ToolContext, Tools};
use crate::{Denied, Gate};

/// Which browser origins may call the endpoint (DNS rebinding protection).
/// Requests without `Origin` (agents, not browsers) always pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginPolicy {
    /// Desktop: only `localhost`, `127.0.0.1` and `[::1]` origins.
    Loopback,
    /// Server: only the origin of the server itself (its `Host`).
    SameHost,
}

#[derive(Clone)]
pub struct McpState {
    pub ctx: Arc<AppContext>,
    pub registry: Arc<Registry<AppContext>>,
    pub tools: Arc<Tools>,
    pub gate: Arc<dyn Gate>,
    pub origin: OriginPolicy,
}

/// The `/mcp` route, ready to be merged into an app's router.
pub fn router(state: McpState) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(message).get(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .with_state(state)
}

fn json(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
}

async fn message(State(state): State<McpState>, headers: HeaderMap, body: Bytes) -> Response {
    if !origin_allowed(state.origin, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let session = match state.gate.open(token).await {
        Ok(s) => s,
        Err(Denied::Disabled) => return StatusCode::NOT_FOUND.into_response(),
        Err(Denied::Unauthorized) => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
            )
                .into_response();
        }
    };
    if let Some(v) = headers.get("mcp-protocol-version")
        && !v.to_str().is_ok_and(|v| SUPPORTED_VERSIONS.contains(&v))
    {
        return json(
            StatusCode::BAD_REQUEST,
            protocol::error(
                Value::Null,
                INVALID_REQUEST,
                "unsupported MCP-Protocol-Version",
            ),
        );
    }
    let message: Value = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => {
            return json(
                StatusCode::BAD_REQUEST,
                protocol::error(Value::Null, PARSE_ERROR, &e.to_string()),
            );
        }
    };
    let cx = ToolContext {
        ctx: state.ctx.clone(),
        registry: state.registry.clone(),
        caller: session.caller,
    };
    match protocol::handle(&state.tools, cx, &session, message).await {
        Reply::Message(m) => json(StatusCode::OK, m),
        Reply::Invalid(m) => json(StatusCode::BAD_REQUEST, m),
        Reply::Accepted => StatusCode::ACCEPTED.into_response(),
    }
}

fn origin_allowed(policy: OriginPolicy, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(host) = origin.to_str().ok().and_then(origin_host) else {
        return false;
    };
    match policy {
        OriginPolicy::Loopback => {
            let name = strip_port(host);
            matches!(name, "localhost" | "127.0.0.1" | "[::1]")
        }
        OriginPolicy::SameHost => headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| h.eq_ignore_ascii_case(host)),
    }
}

/// `host[:port]` of an `http(s)://host[:port]` origin.
fn origin_host(origin: &str) -> Option<&str> {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

fn strip_port(host: &str) -> &str {
    if host.starts_with('[') {
        return host.find(']').map_or(host, |i| &host[..=i]);
    }
    host.rsplit_once(':').map_or(host, |(name, _)| name)
}
