//! Server authentication (PRD §7.17).
//!
//! `none`: open (warns when bound to a non-loopback address).
//! `token`: a shared secret from the config. The browser exchanges it for a
//! session cookie (HttpOnly, SameSite=Strict); scripts may send it as
//! `Authorization: Bearer`. Sessions live in SQLite, so restarts keep users
//! logged in. Cookie-authenticated requests must come from the same origin.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use reqly_core::CoreError;
use reqly_storage::sessions::SessionRepo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppState;

pub const SESSION_COOKIE: &str = "reqly_session";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    None,
    Token,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    /// Shared secret for `token` mode.
    pub token: Option<String>,
    pub session_days: u32,
    /// Add `Secure` to the cookie (enable behind HTTPS).
    pub secure_cookie: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::None,
            token: None,
            session_days: 30,
            secure_cookie: false,
        }
    }
}

/// Failed login attempts per client within a window.
#[derive(Default)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
}

const MAX_ATTEMPTS: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);

impl LoginLimiter {
    fn check_and_count(&self, client: &str) -> bool {
        let mut map = self.attempts.lock().expect("limiter lock");
        let entry = map.entry(client.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed() > WINDOW {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
        entry.0 <= MAX_ATTEMPTS
    }
}

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(CoreError::new(code, message))).into_response()
}

fn digest(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// Constant-time token comparison (on digests, so lengths do not leak).
fn token_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (digest(expected), digest(given));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Same-origin check for cookie-authenticated requests (CSRF protection).
fn origin_ok(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        // Non-browser clients and same-origin GETs often omit Origin.
        return true;
    };
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    origin.split_once("://").map(|(_, rest)| rest) == Some(host)
}

async fn session_valid(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = cookie_value(headers, SESSION_COOKIE) else {
        return false;
    };
    SessionRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .is_valid(&token)
        .await
        .unwrap_or(false)
}

/// Guards everything under `/api/v1` except `/api/v1/auth/*`.
pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Inside `nest("/api/v1")` the path is relative to the prefix.
    let path = request.uri().path();
    let public = path.starts_with("/auth/") || path.starts_with("/api/v1/auth/");
    if state.auth.mode == AuthMode::None || public {
        return next.run(request).await;
    }
    let headers = request.headers();
    if let (Some(expected), Some(given)) = (state.auth.token.as_deref(), bearer(headers)) {
        if token_matches(expected, given) {
            return next.run(request).await;
        }
        return error(StatusCode::UNAUTHORIZED, "auth.invalid", "invalid token");
    }
    if !session_valid(&state, headers).await {
        return error(StatusCode::UNAUTHORIZED, "auth.required", "login required");
    }
    if !origin_ok(headers) {
        return error(
            StatusCode::FORBIDDEN,
            "auth.origin",
            "cross-origin request refused",
        );
    }
    next.run(request).await
}

#[derive(Serialize)]
struct Status {
    mode: AuthMode,
    authenticated: bool,
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> Json<Status> {
    let authenticated = state.auth.mode == AuthMode::None || session_valid(&state, &headers).await;
    Json(Status {
        mode: state.auth.mode,
        authenticated,
    })
}

#[derive(Deserialize)]
struct LoginInput {
    token: String,
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS randomness");
    hex::encode(bytes)
}

async fn login(State(state): State<AppState>, request: Request) -> Response {
    let client = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(a)| a.ip().to_string())
        .unwrap_or_else(|| "local".into());
    let body = match axum::body::to_bytes(request.into_body(), 64 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "auth.invalid_input",
                "invalid body",
            );
        }
    };
    let Ok(input) = serde_json::from_slice::<LoginInput>(&body) else {
        return error(
            StatusCode::BAD_REQUEST,
            "auth.invalid_input",
            "expected {\"token\": …}",
        );
    };
    if !state.login_limiter.check_and_count(&client) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "auth.rate_limited",
            "too many attempts, try again later",
        );
    }
    let Some(expected) = state.auth.token.as_deref() else {
        return error(
            StatusCode::BAD_REQUEST,
            "auth.disabled",
            "token login is not enabled",
        );
    };
    if !token_matches(expected, &input.token) {
        return error(StatusCode::UNAUTHORIZED, "auth.invalid", "invalid token");
    }
    let session = random_token();
    let ttl_ms = i64::from(state.auth.session_days) * 86_400_000;
    if let Err(e) = SessionRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .create(&session, None, ttl_ms)
        .await
    {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.code, &e.message);
    }
    let cookie = format!(
        "{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        ttl_ms / 1000,
        if state.auth.secure_cookie {
            "; Secure"
        } else {
            ""
        }
    );
    (
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        let _ = SessionRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
            .delete(&token)
            .await;
    }
    let clear = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    (
        [(header::SET_COOKIE, clear)],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/login", post(login))
        .route("/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_tokens() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abc", "abd"));
        assert!(!token_matches("abc", ""));
    }

    #[test]
    fn origin_check() {
        let mut h = HeaderMap::new();
        assert!(origin_ok(&h));
        h.insert(header::HOST, "a:1".parse().unwrap());
        h.insert(header::ORIGIN, "http://a:1".parse().unwrap());
        assert!(origin_ok(&h));
        h.insert(header::ORIGIN, "http://b:1".parse().unwrap());
        assert!(!origin_ok(&h));
    }
}
