//! Token auth mode: login, sessions, bearer tokens, CSRF and rate limiting.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use reqly_server::{AppState, AuthConfig, AuthMode, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "s3cret-token";

async fn app() -> axum::Router {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Token,
        token: Some(TOKEN.into()),
        ..AuthConfig::default()
    };
    router(state)
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Option<String>, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        cookie,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn cmd(cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::post("/api/v1/cmd/app.info").header("content-type", "application/json");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::from("{}")).unwrap()
}

fn login(token: &str) -> Request<Body> {
    Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "token": token }).to_string()))
        .unwrap()
}

fn session_cookie(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn commands_require_a_session() {
    let app = app().await;
    let (status, _, body) = send(&app, cmd(None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "auth.required");
}

#[tokio::test]
async fn status_reports_mode_without_auth() {
    let app = app().await;
    let (status, _, body) = send(
        &app,
        Request::get("/api/v1/auth/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "mode": "token", "authenticated": false }));
}

#[tokio::test]
async fn login_sets_a_secure_cookie_that_authorizes() {
    let app = app().await;
    let (status, cookie, _) = send(&app, login(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let set = cookie.unwrap();
    assert!(
        set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Path=/"),
        "{set}"
    );
    let cookie = session_cookie(&set);

    let (status, _, body) = send(&app, cmd(Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, _, st) = send(
        &app,
        Request::get("/api/v1/auth/status")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st["authenticated"], true);

    // Logout invalidates the session.
    let logout = Request::post("/api/v1/auth/logout")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, logout).await.0, StatusCode::OK);
    assert_eq!(
        send(&app, cmd(Some(&cookie))).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn wrong_token_is_rejected() {
    let app = app().await;
    let (status, cookie, body) = send(&app, login("nope")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none());
    assert_eq!(body["code"], "auth.invalid");
}

#[tokio::test]
async fn bearer_token_works_for_scripts() {
    let app = app().await;
    let req = Request::post("/api/v1/cmd/app.info")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(send(&app, req).await.0, StatusCode::OK);
}

#[tokio::test]
async fn cross_origin_posts_are_refused() {
    let app = app().await;
    let (_, cookie, _) = send(&app, login(TOKEN)).await;
    let cookie = session_cookie(&cookie.unwrap());
    let req = Request::post("/api/v1/cmd/app.info")
        .header(header::HOST, "reqly.local:8484")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::COOKIE, &cookie)
        .body(Body::from("{}"))
        .unwrap();
    let (status, _, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "auth.origin");

    let same = Request::post("/api/v1/cmd/app.info")
        .header(header::HOST, "reqly.local:8484")
        .header(header::ORIGIN, "http://reqly.local:8484")
        .header(header::COOKIE, &cookie)
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(send(&app, same).await.0, StatusCode::OK);
}

#[tokio::test]
async fn login_attempts_are_rate_limited() {
    let app = app().await;
    let mut last = StatusCode::OK;
    for _ in 0..12 {
        last = send(&app, login("wrong")).await.0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn sessions_survive_a_restart() {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Token,
        token: Some(TOKEN.into()),
        ..AuthConfig::default()
    };
    let first = router(state.clone());
    let (_, cookie, _) = send(&first, login(TOKEN)).await;
    let cookie = session_cookie(&cookie.unwrap());
    // A new router over the same storage = restarted server.
    let second = router(state);
    assert_eq!(send(&second, cmd(Some(&cookie))).await.0, StatusCode::OK);
}

#[tokio::test]
async fn static_files_and_health_stay_public() {
    let app = app().await;
    assert_eq!(
        send(&app, Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .0,
        StatusCode::OK
    );
    let res = app
        .clone()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn none_mode_needs_no_login() {
    let app = router(AppState::for_tests().await);
    assert_eq!(send(&app, cmd(None)).await.0, StatusCode::OK);
}
