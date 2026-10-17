//! WebSocket, SSE and GraphQL subscription connections through the registry.

use std::time::Duration;

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

async fn node(ctx: &AppContext, data: Value) -> Value {
    let ws = call(ctx, "workspace.ensureDefault", json!({}))
        .await
        .unwrap()[0]["id"]
        .clone();
    let collection = call(ctx, "collection.list", json!({ "workspaceId": ws }))
        .await
        .unwrap()[0]["id"]
        .clone();
    call(
        ctx,
        "node.create",
        json!({ "collectionId": collection, "parentId": null, "name": "s", "data": data }),
    )
    .await
    .unwrap()["id"]
        .clone()
}

/// Waits until the connection log satisfies `pred`.
async fn wait_log(ctx: &AppContext, conn: &Value, pred: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let log = call(ctx, "stream.log", json!({ "connectionId": conn }))
            .await
            .unwrap();
        let log = log.as_array().unwrap().clone();
        if pred(&log) {
            return log;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out: {log:#?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn received(log: &[Value]) -> Vec<String> {
    log.iter()
        .filter(|m| m["direction"] == "received")
        .map(|m| m["data"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn websocket_send_receive_and_disconnect() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "webSocket", "url": server.url("/ws/echo"),
                "headers": [{ "name": "X-Who", "value": "{{ who }}" }],
                "auth": { "type": "bearer", "token": "tok", "prefix": "" },
                "variables": { "who": "reqly" }, "subprotocols": ["echo"] }),
    )
    .await;
    // `request.send` is for HTTP.
    assert_eq!(
        call(&ctx, "request.send", json!({ "requestId": req }))
            .await
            .unwrap_err()
            .code,
        "request.use_connect"
    );

    let mut events = ctx.events.subscribe();
    let info = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(info["kind"], "webSocket");
    assert_eq!(info["status"], "open");
    assert_eq!(info["protocol"], "echo");
    let conn = info["connectionId"].clone();

    let sent = call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "text", "body": "hi {{ who }}" }),
    )
    .await
    .unwrap();
    assert_eq!(sent["data"], "hi reqly");
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "json", "body": "{ \"a\": 1 }" }),
    )
    .await
    .unwrap();
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "binary", "body": "AAEC" }),
    )
    .await
    .unwrap();
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "text", "body": "headers" }),
    )
    .await
    .unwrap();
    let log = wait_log(&ctx, &conn, |l| received(l).len() >= 4).await;
    let got = received(&log);
    assert_eq!(&got[..3], ["hi reqly", "{\"a\":1}", "AAEC"]);
    let headers: Value = serde_json::from_str(&got[3]).unwrap();
    assert_eq!(headers["x-who"], "reqly");
    assert_eq!(headers["authorization"], "Bearer tok");
    let bad = call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "json", "body": "{" }),
    )
    .await
    .unwrap_err();
    assert_eq!(bad.code, "stream.invalid_json");

    call(&ctx, "stream.disconnect", json!({ "connectionId": conn }))
        .await
        .unwrap();
    wait_log(&ctx, &conn, |l| l.iter().any(|m| m["kind"] == "close")).await;
    let mut topics = Vec::new();
    while let Ok(e) = events.try_recv() {
        topics.push(e.topic);
    }
    assert!(topics.iter().any(|t| t == "stream.message"));
    assert!(topics.iter().any(|t| t == "stream.status"));
}

#[tokio::test]
async fn websocket_auto_reconnect_after_abnormal_close() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "webSocket", "url": server.url("/ws/echo"), "autoReconnect": true, "reconnectDelayMs": 50 }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "text", "body": "close:4000:bye" }),
    )
    .await
    .unwrap();
    wait_log(&ctx, &conn, |l| {
        l.iter().filter(|m| m["kind"] == "open").count() == 2
    })
    .await;
    // Working again.
    call(
        &ctx,
        "stream.send",
        json!({ "connectionId": conn, "format": "text", "body": "again" }),
    )
    .await
    .unwrap();
    wait_log(&ctx, &conn, |l| received(l).contains(&"again".to_string())).await;
    call(&ctx, "stream.disconnect", json!({ "connectionId": conn }))
        .await
        .unwrap();
}

#[tokio::test]
async fn sse_events_and_resume_with_last_event_id() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "request", "url": server.url("/sse/events?n=2"), "eventStream": true }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    let log = wait_log(&ctx, &conn, |l| received(l).len() >= 2).await;
    let events: Vec<&Value> = log.iter().filter(|m| m["kind"] == "event").collect();
    assert_eq!(events[0]["event"], "odd");
    assert_eq!(events[0]["eventId"], "1");
    assert_eq!(events[0]["data"], "line 1\nsecond");
    assert_eq!(events[1]["event"], "even");
    call(&ctx, "stream.disconnect", json!({ "connectionId": conn }))
        .await
        .unwrap();

    // Two events per connection, resumed with Last-Event-ID, until 204.
    let req = node(
        &ctx,
        json!({ "kind": "request", "url": server.url("/sse/resume"), "eventStream": true }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    let log = wait_log(&ctx, &conn, |l| {
        l.iter()
            .any(|m| m["data"].as_str().is_some_and(|d| d.contains("204")))
    })
    .await;
    assert_eq!(received(&log), ["1", "2", "3", "4"]);
    let list = call(&ctx, "stream.list", json!({})).await.unwrap();
    let status = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["connectionId"] == conn)
        .unwrap()["status"]
        .clone();
    // Closed (possibly still finishing).
    assert!(status == "closed" || status == "connecting", "{status}");
}

#[tokio::test]
async fn sse_rejects_non_streams() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "request", "url": server.url("/echo"), "eventStream": true }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    let log = wait_log(&ctx, &conn, |l| l.iter().any(|m| m["kind"] == "error")).await;
    assert!(
        log.iter()
            .any(|m| m["data"].as_str().unwrap().contains("200 application/json")),
        "{log:#?}"
    );
}

#[tokio::test]
async fn sse_disconnect_stops_a_running_stream() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "request", "url": server.url("/sse/slow"), "eventStream": true }),
    )
    .await;
    let conn = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap()["connectionId"]
        .clone();
    wait_log(&ctx, &conn, |l| received(l).len() >= 2).await;
    call(&ctx, "stream.disconnect", json!({ "connectionId": conn }))
        .await
        .unwrap();
    let log = wait_log(&ctx, &conn, |l| {
        l.iter().any(|m| m["data"] == "disconnected")
    })
    .await;
    let count = received(&log).len();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let later = call(&ctx, "stream.log", json!({ "connectionId": conn }))
        .await
        .unwrap();
    assert_eq!(
        received(later.as_array().unwrap()).len(),
        count,
        "no events after disconnecting"
    );
}

#[tokio::test]
async fn graphql_subscription_streams_and_queries_do_not() {
    let server = TestServer::start().await;
    let ctx = AppContext::for_tests().await;
    let req = node(
        &ctx,
        json!({ "kind": "request", "method": "POST", "url": server.url("/graphql/ws"),
                "body": { "kind": "graphql", "graphql": { "query": "subscription($n: Int!) { counter(to: $n) }", "variables": "{\"n\": {{ n }}}" } },
                "variables": { "n": 2 } }),
    )
    .await;
    let info = call(&ctx, "stream.connect", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(info["kind"], "graphqlSubscription");
    let conn = info["connectionId"].clone();
    let log = wait_log(&ctx, &conn, |l| {
        l.iter().any(|m| m["data"] == "the subscription completed")
    })
    .await;
    let results: Vec<Value> = received(&log)
        .iter()
        .map(|d| serde_json::from_str(d).unwrap())
        .collect();
    assert_eq!(
        results,
        [
            json!({ "data": { "counter": 1 } }),
            json!({ "data": { "counter": 2 } })
        ]
    );

    let query = node(
        &ctx,
        json!({ "kind": "request", "method": "POST", "url": server.url("/graphql"),
                "body": { "kind": "graphql", "graphql": { "query": "{ hello }" } } }),
    )
    .await;
    assert_eq!(
        call(&ctx, "stream.connect", json!({ "requestId": query }))
            .await
            .unwrap_err()
            .code,
        "stream.not_streaming"
    );
}
