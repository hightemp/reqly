use std::collections::BTreeMap;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::stream;
use serde_json::{Value, json};
use tower_http::compression::CompressionLayer;

pub fn router() -> Router {
    let compressed = Router::new()
        .route(
            "/compressed/gzip",
            get(|| async { axum::Json(json!({ "encoding": "gzip" })) }),
        )
        .route(
            "/compressed/br",
            get(|| async { axum::Json(json!({ "encoding": "br" })) }),
        )
        .layer(CompressionLayer::new());

    Router::new()
        .route("/echo", any(echo))
        .route("/status/{code}", any(status))
        .route("/redirect/{n}", any(redirect))
        .route("/redirect-to", any(redirect_to))
        .route("/cookies", get(cookies))
        .route("/cookies/set", get(set_cookies))
        .route("/cookies/redirect", get(set_cookies_and_redirect))
        .route("/delay/{ms}", any(delay))
        .route("/bytes/{n}", get(bytes))
        .route("/stream/{n}", get(stream_lines))
        .route("/basic-auth/{user}/{pass}", get(basic_auth))
        .route("/bearer/{token}", get(bearer))
        .merge(compressed)
}

async fn echo(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> axum::Json<Value> {
    let query: Vec<(String, String)> = uri
        .query()
        .map(|q| {
            q.split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    (k.to_string(), v.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let headers: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    axum::Json(json!({
        "method": method.as_str(),
        "path": uri.path(),
        "query": query,
        "headers": headers,
        "body": String::from_utf8_lossy(&body),
        "bodyBase64": STANDARD.encode(&body),
        "bodySize": body.len(),
    }))
}

async fn status(Path(code): Path<u16>) -> Response {
    StatusCode::from_u16(code)
        .unwrap_or(StatusCode::BAD_REQUEST)
        .into_response()
}

/// Classic `302 Found` chain: `/redirect/n` → `/redirect/n-1` → … → `/echo`.
async fn redirect(Path(n): Path<u32>) -> Response {
    let location = if n <= 1 {
        "/echo".to_string()
    } else {
        format!("/redirect/{}", n - 1)
    };
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}

/// `302` to an arbitrary `?url=` (absolute or relative); `?status=` overrides the code.
async fn redirect_to(Query(q): Query<BTreeMap<String, String>>) -> Response {
    let status = q.get("status").and_then(|s| s.parse().ok()).unwrap_or(302);
    let location = q.get("url").cloned().unwrap_or_else(|| "/echo".into());
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::FOUND),
        [(header::LOCATION, location)],
    )
        .into_response()
}

async fn cookies(headers: HeaderMap) -> axum::Json<BTreeMap<String, String>> {
    let jar = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    axum::Json(jar)
}

async fn set_cookies(Query(pairs): Query<Vec<(String, String)>>) -> Response {
    let mut res = axum::Json(json!({ "set": pairs.len() })).into_response();
    for (k, v) in pairs {
        let cookie = format!("{k}={v}; Path=/");
        res.headers_mut()
            .append(header::SET_COOKIE, cookie.parse().expect("cookie header"));
    }
    res
}

/// Sets cookies on a 302 hop, so clients must carry them through redirects.
async fn set_cookies_and_redirect(Query(pairs): Query<Vec<(String, String)>>) -> Response {
    let mut res = (StatusCode::FOUND, [(header::LOCATION, "/cookies")]).into_response();
    for (k, v) in pairs {
        let cookie = format!("{k}={v}; Path=/");
        res.headers_mut()
            .append(header::SET_COOKIE, cookie.parse().expect("cookie header"));
    }
    res
}

/// Pretends to be a forward proxy: echoes the absolute request target.
pub fn proxy_router() -> Router {
    Router::new().fallback(|method: Method, uri: Uri, headers: HeaderMap| async move {
        let auth = headers
            .get(header::PROXY_AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        axum::Json(json!({
            "proxied": true,
            "method": method.as_str(),
            "uri": uri.to_string(),
            "proxyAuthorization": auth,
        }))
    })
}

async fn delay(Path(ms): Path<u64>) -> axum::Json<Value> {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    axum::Json(json!({ "delayMs": ms }))
}

async fn bytes(Path(n): Path<usize>) -> Response {
    let body: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
    ([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response()
}

async fn stream_lines(Path(n): Path<usize>) -> Response {
    let chunks =
        (0..n).map(|i| Ok::<_, std::io::Error>(Bytes::from(format!("{{\"line\":{i}}}\n"))));
    Body::from_stream(stream::iter(chunks)).into_response()
}

fn unauthorized(challenge: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
    )
        .into_response()
}

async fn basic_auth(Path((user, pass)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let expected = format!("Basic {}", STANDARD.encode(format!("{user}:{pass}")));
    match headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if v == expected => axum::Json(json!({ "user": user })).into_response(),
        _ => unauthorized("Basic realm=\"reqly\""),
    }
}

async fn bearer(Path(token): Path<String>, headers: HeaderMap) -> Response {
    match headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if v == format!("Bearer {token}") => {
            axum::Json(json!({ "authenticated": true })).into_response()
        }
        _ => unauthorized("Bearer"),
    }
}
