//! The desktop listener follows the settings (TASKS T13.6).

use std::sync::Arc;
use std::time::Duration;

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_mcp::serve_local;
use serde_json::{Value, json};

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn set(ctx: &AppContext, enabled: bool, port: u16) -> Value {
    dispatch_as(
        &registry(),
        ctx,
        Caller::Local,
        "mcp.setConfig",
        json!({ "enabled": enabled, "port": port, "tools": {} }),
    )
    .await
    .unwrap()
}

async fn status(ctx: &AppContext) -> Value {
    dispatch_as(&registry(), ctx, Caller::Local, "mcp.config", json!({}))
        .await
        .unwrap()
}

/// Waits until `mcp.config` reports a listener state matching `done`.
async fn wait_for(ctx: &AppContext, done: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..100 {
        let s = status(ctx).await;
        if done(&s["listen"]) {
            return s["listen"].clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("listener state not reached: {}", status(ctx).await);
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

async fn ping(port: u16, token: &str) -> Result<u16, reqwest::Error> {
    let res = client()
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .bearer_auth(token)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))
        .send()
        .await?;
    Ok(res.status().as_u16())
}

#[tokio::test]
async fn starts_moves_and_stops_with_the_settings() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Desktop).await);
    let task = serve_local(ctx.clone());
    let listen = wait_for(&ctx, |l| l.is_object()).await;
    assert_eq!(listen["endpoint"], Value::Null, "off by default");

    let port = free_port();
    let token = set(&ctx, true, port).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let listen = wait_for(&ctx, |l| !l["endpoint"].is_null()).await;
    assert_eq!(listen["endpoint"], format!("http://127.0.0.1:{port}/mcp"));
    assert_eq!(listen["error"], Value::Null);
    assert_eq!(ping(port, &token).await.unwrap(), 200);
    assert_eq!(ping(port, "wrong").await.unwrap(), 401);

    let moved = free_port();
    set(&ctx, true, moved).await;
    wait_for(&ctx, |l| {
        l["endpoint"] == format!("http://127.0.0.1:{moved}/mcp")
    })
    .await;
    assert_eq!(ping(moved, &token).await.unwrap(), 200);
    assert!(ping(port, &token).await.is_err(), "the old port is closed");

    set(&ctx, false, moved).await;
    wait_for(&ctx, |l| l["endpoint"].is_null()).await;
    assert!(ping(moved, &token).await.is_err(), "closed when disabled");
    task.abort();
}

#[tokio::test]
async fn a_busy_port_is_reported() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Desktop).await);
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut events = ctx.events.subscribe();
    let task = serve_local(ctx.clone());
    set(&ctx, true, port).await;
    let listen = wait_for(&ctx, |l| !l["error"].is_null()).await;
    assert_eq!(listen["endpoint"], Value::Null);
    assert!(
        listen["error"]
            .as_str()
            .unwrap()
            .contains(&port.to_string())
    );
    let mut topics = Vec::new();
    while let Ok(e) = events.try_recv() {
        topics.push(e.topic);
    }
    assert!(
        topics.contains(&"mcp.listenChanged".to_string()),
        "{topics:?}"
    );

    // Freed and saved again: it starts.
    drop(busy);
    set(&ctx, true, port).await;
    wait_for(&ctx, |l| !l["endpoint"].is_null()).await;
    task.abort();
}

#[tokio::test]
async fn never_listens_on_android() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Mobile).await);
    let task = serve_local(ctx.clone());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(status(&ctx).await["listen"]["endpoint"], Value::Null);
    task.abort();
}
