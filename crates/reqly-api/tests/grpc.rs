//! gRPC requests through the command registry.

use std::time::Duration;

use reqly_api::commands::{AppContext, Platform, registry};
use reqly_testserver::grpc::{GrpcTestServer, TEST_PROTO, TYPES_PROTO};
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

async fn grpc_node(ctx: &AppContext, data: Value) -> Value {
    let ws = call(ctx, "workspace.ensureDefault", json!({}))
        .await
        .unwrap()[0]["id"]
        .clone();
    let collection = call(ctx, "collection.list", json!({ "workspaceId": ws }))
        .await
        .unwrap()[0]["id"]
        .clone();
    let mut data = data;
    data["kind"] = json!("grpc");
    call(
        ctx,
        "node.create",
        json!({ "collectionId": collection, "parentId": null, "name": "g", "data": data }),
    )
    .await
    .unwrap()["id"]
        .clone()
}

fn inline() -> Value {
    json!({ "inline": [ { "name": "test.proto", "text": TEST_PROTO }, { "name": "common/types.proto", "text": TYPES_PROTO } ] })
}

async fn finished(ctx: &AppContext, conn: &Value) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let log = call(ctx, "stream.log", json!({ "connectionId": conn }))
            .await
            .unwrap();
        let log = log.as_array().unwrap().clone();
        if log
            .iter()
            .any(|m| m["kind"] == "close" || m["kind"] == "error")
        {
            return log;
        }
        assert!(tokio::time::Instant::now() < deadline, "{log:#?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn received(log: &[Value]) -> Vec<Value> {
    log.iter()
        .filter(|m| m["direction"] == "received")
        .map(|m| serde_json::from_str(m["data"].as_str().unwrap()).unwrap())
        .collect()
}

#[tokio::test]
async fn describe_from_files_and_reflection() {
    let server = GrpcTestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let from_files = grpc_node(&ctx, json!({ "url": server.url(), "proto": inline() })).await;
    let services = call(&ctx, "grpc.describe", json!({ "requestId": from_files }))
        .await
        .unwrap();
    assert_eq!(services[0]["name"], "reqly.test.v1.TestService");
    assert_eq!(services[0]["methods"][3]["clientStreaming"], true);

    let reflection = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": { "reflection": true } }),
    )
    .await;
    let services = call(&ctx, "grpc.describe", json!({ "requestId": reflection }))
        .await
        .unwrap();
    assert_eq!(services.as_array().unwrap().len(), 1);

    let none = grpc_node(&ctx, json!({ "url": server.url() })).await;
    assert_eq!(
        call(&ctx, "grpc.describe", json!({ "requestId": none }))
            .await
            .unwrap_err()
            .code,
        "grpc.no_proto"
    );
}

#[tokio::test]
async fn unary_with_templates_metadata_and_auth() {
    let server = GrpcTestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = grpc_node(
        &ctx,
        json!({ "url": "{{ host }}", "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Echo",
                "message": "{ \"message\": \"{{ greeting }}\" }",
                "metadata": [{ "name": "x-echo", "value": "{{ greeting }}!" }],
                "auth": { "type": "bearer", "token": "t", "prefix": "" },
                "variables": { "host": server.url(), "greeting": "hello" } }),
    )
    .await;
    let info = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(info["kind"], "grpc");
    let log = finished(&ctx, &info["connectionId"]).await;
    let replies = received(&log);
    assert_eq!(replies.len(), 1, "{log:#?}");
    assert_eq!(replies[0]["message"], "hello");
    assert_eq!(replies[0]["metadata"], "hello!");
    assert!(log.iter().any(|m| m["data"] == "status OK (0)"));
    let list = call(&ctx, "stream.list", json!({})).await.unwrap();
    let headers = &list[0]["headers"];
    assert!(
        headers
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "x-served-by"),
        "{headers}"
    );
}

#[tokio::test]
async fn errors_and_server_streaming() {
    let server = GrpcTestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let failing = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Echo",
                "message": "{ \"failCode\": 7 }" }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": failing }))
        .await
        .unwrap()["connectionId"]
        .clone();
    let log = finished(&ctx, &conn).await;
    assert!(
        log.iter().any(|m| m["data"]
            .as_str()
            .unwrap()
            .starts_with("status PERMISSION_DENIED (7)")),
        "{log:#?}"
    );

    let count = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Count",
                "message": "{ \"to\": 3 }" }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": count }))
        .await
        .unwrap()["connectionId"]
        .clone();
    let log = finished(&ctx, &conn).await;
    let values: Vec<Value> = received(&log)
        .into_iter()
        .map(|v| v["value"].clone())
        .collect();
    assert_eq!(values, [json!("1"), json!("2"), json!("3")]);

    let bad = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Echo", "message": "{ \"x\": 1 }" }),
    )
    .await;
    assert_eq!(
        call(&ctx, "stream.connect", json!({ "requestId": bad }))
            .await
            .unwrap_err()
            .code,
        "grpc.invalid_message"
    );
}

#[tokio::test]
async fn client_streaming_with_send_and_end() {
    let server = GrpcTestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Sum" }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    for n in [1, 2, 3] {
        call(&ctx, "stream.send", json!({ "connectionId": conn, "format": "json", "body": format!("{{\"value\": {n}}}") }))
            .await
            .unwrap();
    }
    call(&ctx, "stream.end", json!({ "connectionId": conn }))
        .await
        .unwrap();
    let log = finished(&ctx, &conn).await;
    assert_eq!(received(&log)[0]["sum"], "6");
    let late = call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "json", "body": "{}" }),
    )
    .await
    .unwrap_err();
    assert_eq!(late.code, "stream.not_open");

    // Bidi: answers arrive while sending.
    let chat = grpc_node(
        &ctx,
        json!({ "url": server.url(), "proto": inline(), "service": "reqly.test.v1.TestService", "method": "Chat" }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": chat }))
        .await
        .unwrap()["connectionId"]
        .clone();
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "json", "body": "{\"text\": \"hey\"}" }),
    )
    .await
    .unwrap();
    call(&ctx, "stream.end", json!({ "connectionId": conn }))
        .await
        .unwrap();
    let log = finished(&ctx, &conn).await;
    assert_eq!(received(&log)[0]["text"], "you said: hey");
}

#[tokio::test]
async fn server_refuses_local_proto_files() {
    let mut ctx = AppContext::for_tests().await;
    ctx.platform = Platform::Server;
    let req = grpc_node(&ctx, json!({ "url": "127.0.0.1:9", "proto": { "files": [{ "type": "path", "path": "/etc/x.proto" }] } })).await;
    assert_eq!(
        call(&ctx, "grpc.describe", json!({ "requestId": req }))
            .await
            .unwrap_err()
            .code,
        "http.not_allowed"
    );
}
