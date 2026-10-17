//! A tiny OAuth 2.0 provider for tests. Client: `reqly` / `secret`;
//! resource owner: `alice` / `pw`. `?expires_in=N` on the token endpoint
//! sets the lifetime of issued tokens.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use axum::Form;
use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Default)]
struct State {
    codes: HashMap<String, (String, Option<String>, String)>, // code → (redirect_uri, challenge, scope)
    tokens: HashMap<String, String>,                          // access token → scope
    refresh: HashMap<String, String>,                         // refresh token → scope
    devices: HashMap<String, (String, bool)>,                 // device_code → (user_code, approved)
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn next(prefix: &str) -> String {
    format!("{prefix}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn error(status: StatusCode, code: &str) -> Response {
    (
        status,
        axum::Json(
            json!({ "error": code, "error_description": format!("{code} (test provider)") }),
        ),
    )
        .into_response()
}

fn issue(scope: &str, refresh: bool, expires_in: i64) -> Response {
    let access = next("at");
    let mut state = STATE.lock().unwrap();
    state.tokens.insert(access.clone(), scope.to_string());
    let mut body = json!({ "access_token": access, "token_type": "Bearer", "expires_in": expires_in, "scope": scope });
    if refresh {
        let rt = next("rt");
        state.refresh.insert(rt.clone(), scope.to_string());
        body["refresh_token"] = json!(rt);
    }
    axum::Json(body).into_response()
}

pub async fn authorize(Query(q): Query<HashMap<String, String>>) -> Response {
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    if get("client_id") != "reqly" {
        return error(StatusCode::BAD_REQUEST, "unauthorized_client");
    }
    let redirect = get("redirect_uri");
    let state = get("state");
    let sep = if redirect.contains('?') { '&' } else { '?' };
    match get("response_type").as_str() {
        "code" => {
            let code = next("code");
            let challenge = q.get("code_challenge").cloned();
            if challenge.is_some() && get("code_challenge_method") != "S256" {
                return error(StatusCode::BAD_REQUEST, "invalid_request");
            }
            STATE
                .lock()
                .unwrap()
                .codes
                .insert(code.clone(), (redirect.clone(), challenge, get("scope")));
            Redirect::to(&format!("{redirect}{sep}code={code}&state={state}")).into_response()
        }
        "token" => {
            let access = next("at");
            STATE
                .lock()
                .unwrap()
                .tokens
                .insert(access.clone(), get("scope"));
            Redirect::to(&format!(
                "{redirect}#access_token={access}&token_type=Bearer&expires_in=3600&state={state}"
            ))
            .into_response()
        }
        _ => error(StatusCode::BAD_REQUEST, "unsupported_response_type"),
    }
}

fn client_ok(headers: &HeaderMap, form: &HashMap<String, String>) -> bool {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b| STANDARD.decode(b).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    match basic {
        Some(creds) => creds == "reqly:secret",
        None => {
            form.get("client_id").map(String::as_str) == Some("reqly")
                && form.get("client_secret").map(String::as_str) == Some("secret")
        }
    }
}

pub async fn token(
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let get = |k: &str| form.get(k).cloned().unwrap_or_default();
    let expires_in: i64 = q
        .get("expires_in")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600);
    let grant = get("grant_type");
    // Public clients (PKCE, device) may omit the secret.
    let public = matches!(
        grant.as_str(),
        "authorization_code" | "urn:ietf:params:oauth:grant-type:device_code"
    ) && get("client_id") == "reqly"
        && !headers.contains_key(header::AUTHORIZATION);
    if !public && !client_ok(&headers, &form) {
        return error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    match grant.as_str() {
        "client_credentials" => issue(&get("scope"), false, expires_in),
        "password" => {
            if get("username") != "alice" || get("password") != "pw" {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            issue(&get("scope"), true, expires_in)
        }
        "authorization_code" => {
            let Some((redirect, challenge, scope)) =
                STATE.lock().unwrap().codes.remove(&get("code"))
            else {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            if redirect != get("redirect_uri") {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            if let Some(challenge) = challenge {
                let computed =
                    URL_SAFE_NO_PAD.encode(Sha256::digest(get("code_verifier").as_bytes()));
                if computed != challenge {
                    return error(StatusCode::BAD_REQUEST, "invalid_grant");
                }
            } else if public {
                return error(StatusCode::UNAUTHORIZED, "invalid_client");
            }
            issue(&scope, true, expires_in)
        }
        "refresh_token" => {
            let Some(scope) = STATE.lock().unwrap().refresh.remove(&get("refresh_token")) else {
                return error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            issue(&scope, true, expires_in)
        }
        "urn:ietf:params:oauth:grant-type:device_code" => {
            let device = get("device_code");
            let approved = STATE.lock().unwrap().devices.get(&device).map(|d| d.1);
            match approved {
                Some(true) => {
                    STATE.lock().unwrap().devices.remove(&device);
                    issue("device", false, expires_in)
                }
                Some(false) => error(StatusCode::BAD_REQUEST, "authorization_pending"),
                None => error(StatusCode::BAD_REQUEST, "expired_token"),
            }
        }
        _ => error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    }
}

pub async fn device(Form(form): Form<HashMap<String, String>>) -> Response {
    if form.get("client_id").map(String::as_str) != Some("reqly") {
        return error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let device_code = next("dc");
    let user_code = next("UC").to_uppercase();
    STATE
        .lock()
        .unwrap()
        .devices
        .insert(device_code.clone(), (user_code.clone(), false));
    axum::Json(json!({
        "device_code": device_code,
        "user_code": user_code,
        "verification_uri": "https://example.test/device",
        "verification_uri_complete": format!("https://example.test/device?user_code={user_code}"),
        "interval": 1,
        "expires_in": 300,
    }))
    .into_response()
}

pub async fn approve(Query(q): Query<HashMap<String, String>>) -> Response {
    let code = q.get("user_code").cloned().unwrap_or_default();
    let mut state = STATE.lock().unwrap();
    for (user_code, approved) in state.devices.values_mut() {
        if *user_code == code {
            *approved = true;
            return StatusCode::NO_CONTENT.into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

pub async fn resource(headers: HeaderMap) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    match STATE.lock().unwrap().tokens.get(token) {
        Some(scope) => {
            axum::Json(json!({ "ok": true, "scope": scope, "token": token })).into_response()
        }
        None => error(StatusCode::UNAUTHORIZED, "invalid_token"),
    }
}
