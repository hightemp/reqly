use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use reqly_server::{AppState, Config, router};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn state() -> AppState {
    AppState::for_tests().await
}

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

fn cmd(name: &str, input: Value) -> Request<Body> {
    Request::post(format!("/api/v1/cmd/{name}"))
        .header("content-type", "application/json")
        .body(Body::from(input.to_string()))
        .unwrap()
}

#[tokio::test]
async fn healthz_is_ok() {
    let (status, body) = call(
        router(state().await),
        Request::get("/healthz").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn command_dispatch_returns_output() {
    let (status, body) = call(router(state().await), cmd("app.info", json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["platform"], "server");
}

#[tokio::test]
async fn unknown_command_is_404_with_error_body() {
    let (status, body) = call(router(state().await), cmd("nope", json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "command.unknown");
}

#[tokio::test]
async fn invalid_input_is_400() {
    let (status, body) = call(router(state().await), cmd("app.ping", json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "command.invalid_input");
}

#[tokio::test]
async fn malformed_json_is_400() {
    let req = Request::post("/api/v1/cmd/app.ping")
        .header("content-type", "application/json")
        .body(Body::from("{oops"))
        .unwrap();
    let (status, body) = call(router(state().await), req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "command.invalid_input");
}

#[tokio::test]
async fn serves_spa_index_for_unknown_paths() {
    let app = router(state().await);
    let res = app
        .oneshot(
            Request::get("/some/client/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let res = res.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn unknown_api_path_is_404_not_index() {
    let app = router(state().await);
    let res = app
        .oneshot(Request::get("/api/v1/nope").body(Body::empty()).unwrap())
        .await;
    assert_eq!(res.unwrap().status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn websocket_receives_events() {
    let state = state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router(state.clone())).into_future());

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/ws"))
        .await
        .unwrap();
    let hello: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(hello["type"], "hello");

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    client
        .post(format!("http://{addr}/api/v1/cmd/app.ping"))
        .json(&json!({ "message": "hi" }))
        .send()
        .await
        .unwrap();

    let msg: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(msg["type"], "event");
    assert_eq!(msg["event"]["topic"], "app.pinged");
    assert_eq!(msg["event"]["payload"], "hi");
    ws.close(None).await.unwrap();
}

#[test]
fn config_defaults() {
    let cfg = Config::load(None, |_| None).unwrap();
    assert_eq!(cfg.bind, "127.0.0.1:8484");
    assert_eq!(cfg.data_dir, std::path::PathBuf::from(".reqly-data"));
}

#[test]
fn config_file_then_env_override() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("reqly.toml");
    std::fs::write(
        &file,
        "bind = \"0.0.0.0:9000\"\ndata_dir = \"/var/lib/reqly\"\n",
    )
    .unwrap();

    let cfg = Config::load(Some(&file), |_| None).unwrap();
    assert_eq!(cfg.bind, "0.0.0.0:9000");
    assert_eq!(cfg.data_dir, std::path::PathBuf::from("/var/lib/reqly"));

    let cfg = Config::load(Some(&file), |k| {
        (k == "REQLY_BIND").then(|| "[::]:1".into())
    })
    .unwrap();
    assert_eq!(cfg.bind, "[::]:1");
    assert_eq!(cfg.data_dir, std::path::PathBuf::from("/var/lib/reqly"));
}

#[test]
fn config_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("reqly.toml");
    std::fs::write(&file, "bnd = \"x\"\n").unwrap();
    assert!(Config::load(Some(&file), |_| None).is_err());
}
