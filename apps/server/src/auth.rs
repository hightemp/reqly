//! Server authentication (PRD §7.17).
//!
//! `none`: open (warns when bound to a non-loopback address).
//! `token`: a shared secret from the config. The browser exchanges it for a
//! session cookie (HttpOnly, SameSite=Strict); scripts may send it as
//! `Authorization: Bearer`. Sessions live in SQLite, so restarts keep users
//! logged in. Cookie-authenticated requests must come from the same origin.
//! `multi`: users log in with a password (or send a personal API token as
//! `Bearer`); every command runs as that user (ADR-010).

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
use reqly_api::access::Caller;
use reqly_core::CoreError;
use reqly_storage::invites::InviteRepo;
use reqly_storage::sessions::SessionRepo;
use reqly_storage::users::{User, UserRepo, random_token};
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
    Multi,
}

/// Who may create an account in `multi` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Registration {
    /// Anyone who can reach the server.
    Open,
    /// Only with an invitation.
    Invite,
    /// Only administrators create accounts.
    #[default]
    Admin,
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
    pub registration: Registration,
    /// The first administrator, created at start when there are no users.
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::None,
            token: None,
            session_days: 30,
            secure_cookie: false,
            registration: Registration::default(),
            admin_username: None,
            admin_password: None,
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

fn users(state: &AppState) -> UserRepo<'_> {
    UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
}

/// The user of a live session (multi mode), `Local` for a live session of
/// token mode, `None` without one.
async fn session_caller(state: &AppState, headers: &HeaderMap) -> Option<(Caller, Option<User>)> {
    let token = cookie_value(headers, SESSION_COOKIE)?;
    let user_id = SessionRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .lookup(&token)
        .await
        .ok()??;
    match (state.auth.mode, user_id) {
        (AuthMode::Multi, Some(id)) => {
            let user = users(state).get(id.parse().ok()?).await.ok()?;
            (!user.disabled).then(|| (caller_of(&user), Some(user)))
        }
        (AuthMode::Token, None) => Some((Caller::Local, None)),
        _ => None,
    }
}

fn caller_of(user: &User) -> Caller {
    Caller::User {
        id: user.id,
        admin: user.is_admin,
    }
}

/// Guards everything under `/api/v1` except `/api/v1/auth/*` and puts the
/// [`Caller`] into the request extensions.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // Inside `nest("/api/v1")` the path is relative to the prefix.
    let path = request.uri().path();
    let public = path.starts_with("/auth/") || path.starts_with("/api/v1/auth/");
    if state.auth.mode == AuthMode::None || public {
        return next.run(request).await;
    }
    let headers = request.headers();
    if let Some(given) = bearer(headers) {
        let caller = match state.auth.mode {
            AuthMode::Multi => users(&state)
                .user_for_token(given)
                .await
                .ok()
                .flatten()
                .map(|u| caller_of(&u)),
            _ => state
                .auth
                .token
                .as_deref()
                .filter(|expected| token_matches(expected, given))
                .map(|_| Caller::Local),
        };
        let Some(caller) = caller else {
            return error(StatusCode::UNAUTHORIZED, "auth.invalid", "invalid token");
        };
        request.extensions_mut().insert(caller);
        return next.run(request).await;
    }
    let Some((caller, _)) = session_caller(&state, headers).await else {
        return error(StatusCode::UNAUTHORIZED, "auth.required", "login required");
    };
    if !origin_ok(headers) {
        return error(
            StatusCode::FORBIDDEN,
            "auth.origin",
            "cross-origin request refused",
        );
    }
    request.extensions_mut().insert(caller);
    next.run(request).await
}

#[derive(Serialize)]
struct Status {
    mode: AuthMode,
    authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    registration: Option<Registration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<User>,
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> Json<Status> {
    let session = match state.auth.mode {
        AuthMode::None => Some((Caller::Local, None)),
        _ => session_caller(&state, &headers).await,
    };
    let multi = state.auth.mode == AuthMode::Multi;
    Json(Status {
        mode: state.auth.mode,
        authenticated: session.is_some(),
        registration: multi.then_some(state.auth.registration),
        user: session.and_then(|(_, user)| user),
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LoginInput {
    Token { token: String },
    Password { username: String, password: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterInput {
    username: String,
    password: String,
    #[serde(default)]
    display_name: String,
    /// An invitation token: needed when registration is by invitation.
    #[serde(default)]
    invite: Option<String>,
}

fn client_of(request: &Request) -> String {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(a)| a.ip().to_string())
        .unwrap_or_else(|| "local".into())
}

async fn json_body<T: serde::de::DeserializeOwned>(request: Request) -> Option<T> {
    let body = axum::body::to_bytes(request.into_body(), 64 * 1024)
        .await
        .ok()?;
    serde_json::from_slice(&body).ok()
}

fn invalid_input(hint: &str) -> Response {
    error(StatusCode::BAD_REQUEST, "auth.invalid_input", hint)
}

fn core_error(e: CoreError) -> Response {
    let status = match e.code.as_str() {
        "auth.invalid" => StatusCode::UNAUTHORIZED,
        "auth.locked" => StatusCode::TOO_MANY_REQUESTS,
        "auth.disabled" => StatusCode::FORBIDDEN,
        "storage.error" | "internal" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(e)).into_response()
}

/// Starts a session and answers with its cookie.
async fn start_session(state: &AppState, user_id: Option<String>) -> Response {
    let session = random_token();
    let ttl_ms = i64::from(state.auth.session_days) * 86_400_000;
    if let Err(e) = SessionRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .create(&session, user_id.as_deref(), ttl_ms)
        .await
    {
        return core_error(e);
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

async fn login(State(state): State<AppState>, request: Request) -> Response {
    let client = client_of(&request);
    let Some(input) = json_body::<LoginInput>(request).await else {
        return invalid_input("expected {\"token\"} or {\"username\", \"password\"}");
    };
    if !state.login_limiter.check_and_count(&client) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "auth.rate_limited",
            "too many attempts, try again later",
        );
    }
    match (state.auth.mode, input) {
        (AuthMode::Token, LoginInput::Token { token }) => {
            let Some(expected) = state.auth.token.as_deref() else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "auth.disabled",
                    "token login is not enabled",
                );
            };
            if !token_matches(expected, &token) {
                return error(StatusCode::UNAUTHORIZED, "auth.invalid", "invalid token");
            }
            start_session(&state, None).await
        }
        (AuthMode::Multi, LoginInput::Password { username, password }) => {
            match users(&state).verify_login(&username, &password).await {
                Ok(user) => start_session(&state, Some(user.id.to_string())).await,
                Err(e) => core_error(e),
            }
        }
        _ => error(
            StatusCode::BAD_REQUEST,
            "auth.disabled",
            "this login method is not enabled",
        ),
    }
}

async fn register(State(state): State<AppState>, request: Request) -> Response {
    let client = client_of(&request);
    let Some(input) = json_body::<RegisterInput>(request).await else {
        return invalid_input("expected {\"username\", \"password\"}");
    };
    let allowed = match state.auth.registration {
        Registration::Open => true,
        Registration::Invite => input.invite.is_some(),
        Registration::Admin => false,
    };
    if state.auth.mode != AuthMode::Multi || !allowed {
        return error(
            StatusCode::FORBIDDEN,
            "auth.registration_closed",
            "registration is closed",
        );
    }
    let invites = InviteRepo::new(&state.ctx.storage, state.ctx.clock.as_ref());
    if let Some(token) = &input.invite
        && let Err(e) = invites.preview(token).await
    {
        return core_error(e);
    }
    if !state.login_limiter.check_and_count(&client) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "auth.rate_limited",
            "too many attempts, try again later",
        );
    }
    match users(&state)
        .create(&input.username, &input.display_name, &input.password, false)
        .await
    {
        Ok(user) => {
            if let Some(token) = &input.invite
                && let Err(e) = invites.accept(token, user.id).await
            {
                // Used by someone else meanwhile: the account stays.
                tracing::info!(error = %e, "invitation not accepted");
            }
            start_session(&state, Some(user.id.to_string())).await
        }
        Err(e) => core_error(e),
    }
}

/// What an invitation offers (public, for the sign-up screen).
async fn invite_preview(
    State(state): State<AppState>,
    axum::extract::Path(token): axum::extract::Path<String>,
) -> Response {
    match InviteRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .preview(&token)
        .await
    {
        Ok(preview) => Json(preview).into_response(),
        Err(e) if e.code == "invite.invalid" => (StatusCode::NOT_FOUND, Json(e)).into_response(),
        Err(e) => core_error(e),
    }
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
        .route("/register", post(register))
        .route("/invite/{token}", get(invite_preview))
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
