//! End-to-end send flow through the command registry.

use std::time::Duration;

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    collection: Value,
    workspace: Value,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests().await;
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let col =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();
        Self {
            ctx,
            collection: col,
            workspace: ws,
        }
    }

    async fn request(&self, parent: Value, data: Value) -> Value {
        call(
            &self.ctx,
            "node.create",
            json!({ "collectionId": self.collection, "parentId": parent, "name": "r", "data": data }),
        )
        .await["id"]
            .clone()
    }

    async fn call(&self, name: &str, input: Value) -> Value {
        call(&self.ctx, name, input).await
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    registry()
        .dispatch(ctx, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn body_text(resp: &Value) -> String {
    use base64::Engine;
    let b64 = resp["base64"].as_str().unwrap();
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn sends_and_records_history() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let folder = call(
        &fx.ctx,
        "node.create",
        json!({
            "collectionId": fx.collection, "parentId": null, "name": "api",
            "data": { "kind": "folder", "headers": [{ "name": "X-Folder", "value": "yes" }] }
        }),
    )
    .await["id"]
        .clone();
    let req = fx
        .request(
            folder,
            json!({ "kind": "request", "method": "POST", "url": server.url("/echo"),
            "body": { "kind": "json", "text": "{\"a\":1}" } }),
        )
        .await;

    let mut events = fx.ctx.events.subscribe();
    let record = fx.call("request.send", json!({ "requestId": req })).await;
    assert_eq!(record["status"], 200);
    assert_eq!(record["requestSnapshot"]["method"], "POST");
    assert!(record["timeline"].as_array().unwrap().len() > 3);
    assert!(record["timings"]["totalMs"].as_f64().unwrap() > 0.0);
    assert_eq!(record["contentType"], "application/json");

    let chunk = fx
        .call(
            "response.body",
            json!({ "id": record["id"], "offset": 0, "limit": 1_000_000 }),
        )
        .await;
    let echoed: Value = serde_json::from_str(&body_text(&chunk)).unwrap();
    assert_eq!(echoed["body"], "{\"a\":1}");
    assert!(
        echoed["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "x-folder")
    );
    assert_eq!(chunk["total"], record["bodySize"]);

    let list = fx.call("response.list", json!({ "requestId": req })).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["method"], "POST");

    let mut topics = Vec::new();
    while let Ok(e) = events.try_recv() {
        topics.push(e.topic);
    }
    assert!(
        topics.contains(&"request.started".to_string()),
        "{topics:?}"
    );
    assert!(
        topics.contains(&"request.finished".to_string()),
        "{topics:?}"
    );
}

#[tokio::test]
async fn failures_are_recorded_with_error() {
    let fx = Fx::new().await;
    let req = fx
        .request(
            Value::Null,
            json!({ "kind": "request", "url": "http://127.0.0.1:9/" }),
        )
        .await;
    let record = fx.call("request.send", json!({ "requestId": req })).await;
    assert_eq!(record["status"], Value::Null);
    assert_eq!(record["error"]["code"], "http.connect");
    assert!(!record["timeline"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn cookies_persist_between_requests() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let set = fx
        .request(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/cookies/set?session=abc") }),
        )
        .await;
    let read = fx
        .request(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/cookies") }),
        )
        .await;
    fx.call("request.send", json!({ "requestId": set })).await;

    let jar = fx
        .call("cookies.list", json!({ "workspaceId": fx.workspace }))
        .await;
    assert_eq!(jar[0]["name"], "session");

    let record = fx.call("request.send", json!({ "requestId": read })).await;
    let chunk = fx
        .call(
            "response.body",
            json!({ "id": record["id"], "offset": 0, "limit": 100000 }),
        )
        .await;
    assert!(body_text(&chunk).contains("\"session\":\"abc\""));

    fx.call("cookies.clear", json!({ "workspaceId": fx.workspace }))
        .await;
    assert_eq!(
        fx.call("cookies.list", json!({ "workspaceId": fx.workspace }))
            .await,
        json!([])
    );
}

#[tokio::test]
async fn cancel_stops_a_running_request() {
    let server = TestServer::start().await;
    let fx = std::sync::Arc::new(Fx::new().await);
    let req = fx
        .request(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/delay/5000") }),
        )
        .await;
    let fx2 = fx.clone();
    let req2 = req.clone();
    let task = tokio::spawn(async move {
        registry()
            .dispatch(&fx2.ctx, "request.send", json!({ "requestId": req2 }))
            .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    fx.call("request.cancel", json!({ "requestId": req })).await;
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(err.code, "http.cancelled");
    assert_eq!(
        fx.call("response.list", json!({ "requestId": req })).await,
        json!([])
    );
}

#[tokio::test]
async fn history_limit_and_clear_for_folder() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    fx.call(
        "settings.set",
        json!({ "key": "history.limits", "value": { "perRequest": 2 } }),
    )
    .await;
    let folder = call(
        &fx.ctx,
        "node.create",
        json!({ "collectionId": fx.collection, "parentId": null, "name": "f", "data": { "kind": "folder" } }),
    )
    .await["id"]
        .clone();
    let req = fx
        .request(
            folder.clone(),
            json!({ "kind": "request", "url": server.url("/echo") }),
        )
        .await;
    for _ in 0..3 {
        fx.call("request.send", json!({ "requestId": req })).await;
    }
    assert_eq!(
        fx.call("response.list", json!({ "requestId": req }))
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );

    fx.call("history.clear", json!({ "ids": [folder] })).await;
    assert_eq!(
        fx.call("response.list", json!({ "requestId": req })).await,
        json!([])
    );
}

#[tokio::test]
async fn sending_a_folder_is_rejected() {
    let fx = Fx::new().await;
    let folder = call(
        &fx.ctx,
        "node.create",
        json!({ "collectionId": fx.collection, "parentId": null, "name": "f", "data": { "kind": "folder" } }),
    )
    .await["id"]
        .clone();
    let err = registry()
        .dispatch(&fx.ctx, "request.send", json!({ "requestId": folder }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "request.not_http");
}

#[tokio::test]
async fn uploaded_blob_is_sent_as_binary_body() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    use base64::Engine;
    let blob = fx
        .call("blob.upload", json!({ "name": "a.bin", "base64": base64::engine::general_purpose::STANDARD.encode("bin!") }))
        .await;
    let req = fx
        .request(Value::Null, json!({ "kind": "request", "method": "PUT", "url": server.url("/echo"),
            "body": { "kind": "binary", "file": { "type": "blob", "hash": blob["hash"], "name": "a.bin", "size": 4 } } }))
        .await;
    let record = fx.call("request.send", json!({ "requestId": req })).await;
    let chunk = fx
        .call(
            "response.body",
            json!({ "id": record["id"], "offset": 0, "limit": 100000 }),
        )
        .await;
    assert!(body_text(&chunk).contains("\"body\":\"bin!\""));
}

#[tokio::test]
async fn curl_import_and_export() {
    let fx = Fx::new().await;
    let parsed = fx
        .call(
            "curl.parse",
            json!({ "command": "curl -X POST 'http://h/x?a=1' -H 'X-A: 1' -d 'k=v'" }),
        )
        .await;
    let data = parsed[0]["request"].clone();
    assert_eq!(data["method"], "POST");
    assert_eq!(data["params"][0]["name"], "a");

    let mut node_data = data.clone();
    node_data["kind"] = json!("request");
    let id = fx.request(Value::Null, node_data).await;
    let out = fx.call("request.toCurl", json!({ "requestId": id })).await;
    let cmd = out["command"].as_str().unwrap();
    assert!(cmd.contains("--url 'http://h/x?a=1'"), "{cmd}");
    assert!(cmd.contains("--data-raw 'k=v'"), "{cmd}");
}

#[tokio::test]
async fn save_body_writes_file_and_is_refused_on_server() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let req = fx
        .request(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/bytes/10") }),
        )
        .await;
    let record = fx.call("request.send", json!({ "requestId": req })).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.bin");
    fx.call(
        "response.saveBody",
        json!({ "id": record["id"], "path": path }),
    )
    .await;
    assert_eq!(std::fs::read(&path).unwrap().len(), 10);

    let server_ctx = AppContext::for_tests_on(reqly_api::commands::Platform::Server).await;
    let err = registry()
        .dispatch(
            &server_ctx,
            "response.saveBody",
            json!({ "id": record["id"], "path": path }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "command.unavailable");
}
