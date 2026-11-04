//! Desktop apps sync through a running server (ADR-013).

use std::net::SocketAddr;

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, dispatch_as, registry};
use reqly_server::{AppState, AuthConfig, AuthMode, router};
use reqly_storage::users::UserRepo;
use serde_json::{Value, json};

async fn start_server() -> String {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Multi,
        ..AuthConfig::default()
    };
    UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .create("ada", "Ada", "ada-password", false)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

struct Desktop {
    ctx: AppContext,
    registry: reqly_api::Registry<AppContext>,
}

impl Desktop {
    async fn new() -> Self {
        let ctx = AppContext::for_tests().await;
        Self {
            ctx,
            registry: registry(),
        }
    }

    async fn call(&self, name: &str, input: Value) -> Value {
        dispatch_as(
            &self.registry,
            &self.ctx,
            Caller::Local,
            name,
            input.clone(),
        )
        .await
        .unwrap_or_else(|e| panic!("{name} {input}: {e:?}"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_desktops_share_a_workspace() {
    let url = start_server().await;
    let a = Desktop::new().await;
    let b = Desktop::new().await;

    let wrong = dispatch_as(
        &a.registry,
        &a.ctx,
        Caller::Local,
        "sync.connect",
        json!({
            "url": url, "username": "ada", "password": "nope"
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(wrong.code, "auth.invalid");

    let status = a
        .call(
            "sync.connect",
            json!({ "url": url, "username": "ada", "password": "ada-password" }),
        )
        .await;
    assert_eq!(status["user"], "Ada (ada)");
    assert_eq!(status["url"], url);

    // A uploads a local workspace.
    let ws = a
        .call("workspace.create", json!({ "name": "Shared" }))
        .await["id"]
        .clone();
    let col = a
        .call(
            "collection.create",
            json!({ "workspaceId": ws, "name": "API" }),
        )
        .await["id"]
        .clone();
    let req = a
        .call(
            "node.create",
            json!({
                "collectionId": col, "name": "List users",
                "data": { "kind": "request", "method": "GET", "url": "https://api.test/users" }
            }),
        )
        .await["id"]
        .clone();
    let listed = a.call("sync.workspaces", json!({})).await;
    let shared = listed["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == ws)
        .unwrap();
    assert_eq!(
        (shared["local"].clone(), shared["remote"].clone()),
        (json!(true), json!(false))
    );
    a.call("sync.follow", json!({ "workspaceId": ws })).await;
    let status = a.call("sync.now", json!({})).await;
    assert_eq!(status["lastError"], Value::Null, "{status}");
    assert_eq!(status["pending"], 0);
    assert_eq!(
        status["workspaces"],
        json!([ws]),
        "still synced after the upload"
    );

    // B signs in with a token made for A's account and downloads it.
    let status = b
        .call(
            "sync.connect",
            json!({ "url": url, "username": "ada", "password": "ada-password" }),
        )
        .await;
    assert_eq!(status["workspaces"], json!([]));
    let listed = b.call("sync.workspaces", json!({})).await;
    let shared = listed["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == ws)
        .unwrap();
    assert_eq!(
        (shared["local"].clone(), shared["remote"].clone()),
        (json!(false), json!(true))
    );
    b.call("sync.follow", json!({ "workspaceId": ws })).await;
    b.call("sync.now", json!({})).await;
    let node = b.call("node.get", json!({ "id": req })).await;
    assert_eq!(node["name"], "List users");

    // Edits on both sides meet.
    b.call("node.update", json!({ "id": req, "name": "All users" }))
        .await;
    a.call(
        "node.update",
        json!({ "id": req, "baseVersion": 1, "fields": { "url": "https://api.test/v2/users" } }),
    )
    .await;
    b.call("sync.now", json!({})).await;
    a.call("sync.now", json!({})).await;
    b.call("sync.now", json!({})).await;
    for d in [&a, &b] {
        let node = d.call("node.get", json!({ "id": req })).await;
        assert_eq!(node["name"], "All users");
        assert_eq!(node["data"]["url"], "https://api.test/v2/users");
    }

    let log = a.call("sync.log", json!({})).await;
    assert!(
        log.as_array()
            .unwrap()
            .iter()
            .any(|e| e["pushed"].as_u64() > Some(0))
    );

    // Disconnecting keeps the data and stops syncing.
    let status = b.call("sync.disconnect", json!({})).await;
    assert_eq!(status["url"], Value::Null);
    assert_eq!(
        b.call("node.get", json!({ "id": req })).await["name"],
        "All users"
    );
}
