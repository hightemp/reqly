//! The MCP endpoint: JSON-RPC over `POST /mcp` (Streamable HTTP, ADR-014).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, registry};
use reqly_mcp::{
    Allowed, Annotations, BoxFuture, Denied, Gate, Group, McpState, OriginPolicy, Session, Tool,
    Tools, router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "secret-token";

/// Accepts `TOKEN`; `disabled` answers as if MCP were off; `allowed` limits tools.
struct TestGate {
    disabled: bool,
    allowed: Allowed,
}

impl Gate for TestGate {
    fn open<'a>(&'a self, token: Option<&'a str>) -> BoxFuture<'a, Result<Session, Denied>> {
        Box::pin(async move {
            if self.disabled {
                return Err(Denied::Disabled);
            }
            if token != Some(TOKEN) {
                return Err(Denied::Unauthorized);
            }
            Ok(Session {
                caller: Caller::Local,
                allowed: self.allowed.clone(),
            })
        })
    }
}

fn tools() -> Tools {
    let mut tools = Tools::new();
    tools.add(Tool::new(
        "app_info",
        Group::Read,
        "Version of the app.",
        json!({ "type": "object", "properties": {} }),
        Annotations::read_only(),
        |cx, _args| Box::pin(async move { cx.call("app.info", json!({})).await }),
    ));
    tools.add(Tool::new(
        "echo",
        Group::Edit,
        "Returns its arguments.",
        json!({ "type": "object", "properties": { "text": { "type": "string" } } }),
        Annotations::default(),
        |_cx, args| Box::pin(async move { Ok(args) }),
    ));
    tools.add(Tool::new(
        "fail",
        Group::Run,
        "Always fails.",
        json!({ "type": "object" }),
        Annotations::default(),
        |cx, _args| Box::pin(async move { cx.call("node.get", json!({ "id": "nope" })).await }),
    ));
    tools
}

async fn app_with(gate: TestGate, origin: OriginPolicy) -> axum::Router {
    app_and_ctx(gate, origin).await.0
}

async fn app_and_ctx(gate: TestGate, origin: OriginPolicy) -> (axum::Router, Arc<AppContext>) {
    let ctx = Arc::new(AppContext::for_tests().await);
    let state = McpState {
        ctx: ctx.clone(),
        registry: Arc::new(registry()),
        tools: Arc::new(tools()),
        gate: Arc::new(gate),
        origin,
    };
    (router(state), ctx)
}

async fn app() -> axum::Router {
    app_with(
        TestGate {
            disabled: false,
            allowed: Allowed::All,
        },
        OriginPolicy::Loopback,
    )
    .await
}

fn post() -> axum::http::request::Builder {
    Request::post("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {TOKEN}"))
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
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

async fn rpc(app: &axum::Router, message: Value) -> (StatusCode, Value) {
    let text = message.to_string();
    send(app, post().body(Body::from(text)).unwrap()).await
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[tokio::test]
async fn initialize_negotiates_the_protocol_version() {
    let app = app().await;
    let (status, body) = rpc(
        &app,
        request(
            1,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    let result = &body["result"];
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert!(result["capabilities"]["tools"].is_object());
    assert_eq!(result["serverInfo"]["name"], "reqly");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(result["instructions"].as_str().unwrap().contains("Reqly"));

    // An older supported revision is kept; an unknown one gets the latest.
    let (_, body) = rpc(
        &app,
        request(2, "initialize", json!({ "protocolVersion": "2025-03-26" })),
    )
    .await;
    assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
    let (_, body) = rpc(
        &app,
        request(3, "initialize", json!({ "protocolVersion": "1999-01-01" })),
    )
    .await;
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
}

#[tokio::test]
async fn notifications_and_client_responses_are_accepted_without_a_body() {
    let app = app().await;
    let (status, body) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body, Value::Null);
    let (status, _) = rpc(&app, json!({ "jsonrpc": "2.0", "id": 9, "result": {} })).await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn ping_and_unknown_methods() {
    let app = app().await;
    let (_, body) = rpc(&app, request(1, "ping", json!({}))).await;
    assert_eq!(body["result"], json!({}));
    let (status, body) = rpc(&app, request(2, "resources/list", json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["error"]["code"], -32601);
    assert_eq!(body["id"], 2);
}

#[tokio::test]
async fn invalid_messages() {
    let app = app().await;
    let (status, body) = send(&app, post().body(Body::from("{")).unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], -32700);
    assert_eq!(body["id"], Value::Null);

    let (status, body) = rpc(&app, json!({ "id": 1, "method": "ping" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], -32600);

    // Batches were removed in 2025-06-18.
    let (status, body) = rpc(&app, json!([request(1, "ping", json!({}))])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], -32600);
}

#[tokio::test]
async fn lists_tools_with_schemas_and_annotations() {
    let app = app().await;
    let (_, body) = rpc(&app, request(1, "tools/list", json!({}))).await;
    let tools = body["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["app_info", "echo", "fail"]);
    let info = &tools[0];
    assert_eq!(info["description"], "Version of the app.");
    assert_eq!(info["inputSchema"]["type"], "object");
    assert_eq!(info["annotations"]["readOnlyHint"], true);
    assert_eq!(info["annotations"]["destructiveHint"], false);
}

#[tokio::test]
async fn calls_tools_through_the_registry() {
    let app = app().await;
    let (_, body) = rpc(
        &app,
        request(
            1,
            "tools/call",
            json!({ "name": "app_info", "arguments": {} }),
        ),
    )
    .await;
    let result = &body["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    let text = result["content"][0]["text"].as_str().unwrap();
    assert_eq!(result["content"][0]["type"], "text");
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed, result["structuredContent"]);

    // Arguments default to an empty object; non-object results are wrapped.
    let (_, body) = rpc(&app, request(2, "tools/call", json!({ "name": "echo" }))).await;
    assert_eq!(body["result"]["structuredContent"], json!({}));
}

#[tokio::test]
async fn tool_failures_are_results_the_agent_can_read() {
    let app = app().await;
    let (status, body) = rpc(
        &app,
        request(1, "tools/call", json!({ "name": "fail", "arguments": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let result = &body["result"];
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("command.invalid_input"), "{text}");
}

#[tokio::test]
async fn unknown_or_disallowed_tools_are_rejected() {
    let app = app_with(
        TestGate {
            disabled: false,
            allowed: Allowed::Only(["app_info".to_string()].into()),
        },
        OriginPolicy::Loopback,
    )
    .await;
    let (_, body) = rpc(&app, request(1, "tools/list", json!({}))).await;
    let names: Vec<_> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].clone())
        .collect();
    assert_eq!(names, [json!("app_info")]);

    for name in ["echo", "nope"] {
        let (_, body) = rpc(
            &app,
            request(2, "tools/call", json!({ "name": name, "arguments": {} })),
        )
        .await;
        assert_eq!(body["error"]["code"], -32602, "{name}");
    }
}

#[tokio::test]
async fn requires_the_token() {
    let app = app().await;
    let msg = request(1, "ping", json!({})).to_string();
    for auth in [None, Some("Bearer wrong"), Some("Basic xyz")] {
        let mut req = Request::post("/mcp").header("content-type", "application/json");
        if let Some(a) = auth {
            req = req.header("authorization", a);
        }
        let res = app
            .clone()
            .oneshot(req.body(Body::from(msg.clone())).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{auth:?}");
        assert_eq!(res.headers()["www-authenticate"], "Bearer");
    }
}

#[tokio::test]
async fn disabled_mcp_is_not_found() {
    let app = app_with(
        TestGate {
            disabled: true,
            allowed: Allowed::All,
        },
        OriginPolicy::Loopback,
    )
    .await;
    let (status, _) = rpc(&app, request(1, "ping", json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_is_not_offered() {
    let app = app().await;
    let res = app
        .oneshot(
            Request::get("/mcp")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn rejects_an_unsupported_protocol_version_header() {
    let app = app().await;
    let msg = request(1, "ping", json!({})).to_string();
    let (status, _) = send(
        &app,
        post()
            .header("mcp-protocol-version", "1999-01-01")
            .body(Body::from(msg.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &app,
        post()
            .header("mcp-protocol-version", "2025-03-26")
            .body(Body::from(msg))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn status_with_origin(app: &axum::Router, origin: &str, host: &str) -> StatusCode {
    let msg = request(1, "ping", json!({})).to_string();
    let res = app
        .clone()
        .oneshot(
            post()
                .header("origin", origin)
                .header("host", host)
                .body(Body::from(msg))
                .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

#[tokio::test]
async fn loopback_policy_rejects_foreign_origins() {
    let app = app().await;
    for ok in [
        "http://localhost:3000",
        "http://127.0.0.1:7655",
        "http://[::1]",
        "https://localhost",
    ] {
        assert_eq!(
            status_with_origin(&app, ok, "127.0.0.1:7655").await,
            StatusCode::OK,
            "{ok}"
        );
    }
    for bad in [
        "https://evil.example",
        "http://localhost.evil.example",
        "http://127.0.0.1.evil.example",
        "null",
    ] {
        assert_eq!(
            status_with_origin(&app, bad, "127.0.0.1:7655").await,
            StatusCode::FORBIDDEN,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn same_host_policy_accepts_only_the_servers_own_origin() {
    let app = app_with(
        TestGate {
            disabled: false,
            allowed: Allowed::All,
        },
        OriginPolicy::SameHost,
    )
    .await;
    assert_eq!(
        status_with_origin(&app, "https://reqly.example.com", "reqly.example.com").await,
        StatusCode::OK
    );
    assert_eq!(
        status_with_origin(&app, "https://evil.example", "reqly.example.com").await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn tool_calls_are_logged_without_secrets() {
    let (app, ctx) = app_and_ctx(
        TestGate {
            disabled: false,
            allowed: Allowed::All,
        },
        OriginPolicy::Loopback,
    )
    .await;
    let mut events = ctx.events.subscribe();
    rpc(
        &app,
        request(
            1,
            "tools/call",
            json!({
                "name": "echo",
                "arguments": {
                    "text": "hello",
                    "values": { "api_key": "s3cr3t" },
                    "auth": { "password": "hunter2", "username": "ada" },
                    "long": "x".repeat(1000)
                }
            }),
        ),
    )
    .await;
    rpc(
        &app,
        request(2, "tools/call", json!({ "name": "fail", "arguments": {} })),
    )
    .await;
    // Listing tools is not a call.
    rpc(&app, request(3, "tools/list", json!({}))).await;

    let log = ctx.mcp_log.recent();
    assert_eq!(log.len(), 2);
    let (fail, echo) = (&log[0], &log[1]);
    assert_eq!(echo.tool, "echo");
    assert!(echo.ok);
    assert_eq!(echo.user, None);
    assert!(echo.arguments.contains("hello"), "{}", echo.arguments);
    assert!(echo.arguments.contains("ada"), "{}", echo.arguments);
    for secret in ["s3cr3t", "hunter2"] {
        assert!(!echo.arguments.contains(secret), "{}", echo.arguments);
    }
    assert!(
        echo.arguments.chars().count() <= 301,
        "{}",
        echo.arguments.len()
    );
    assert_eq!(fail.tool, "fail");
    assert!(!fail.ok);
    assert!(
        fail.error
            .as_deref()
            .unwrap()
            .contains("command.invalid_input")
    );

    let event = events.recv().await.unwrap();
    assert_eq!(event.topic, "mcp.call");
    assert_eq!(event.payload["tool"], "echo");
}
