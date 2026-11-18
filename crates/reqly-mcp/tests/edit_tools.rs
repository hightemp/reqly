//! Edit and environment tools (TASKS T13.4).

mod common;

use std::sync::Arc;

use common::Fx;
use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform};
use reqly_core::Id;
use reqly_mcp::HIDDEN;
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use serde_json::{Value, json};

async fn topics(fx: &Fx, f: impl std::future::Future<Output = Value>) -> (Value, Vec<String>) {
    let mut events = fx.ctx.events.subscribe();
    let out = f.await;
    let mut topics = Vec::new();
    while let Ok(e) = events.try_recv() {
        topics.push(e.topic);
    }
    (out, topics)
}

#[tokio::test]
async fn create_requests_and_folders() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let (folder, topics_seen) = topics(
        &fx,
        fx.tool(
            "create_folder",
            json!({ "collectionId": col["id"], "name": "Users" }),
        ),
    )
    .await;
    assert_eq!(folder["name"], "Users");
    assert_eq!(folder["path"], "Users");
    assert!(
        topics_seen.contains(&"tree.changed".to_string()),
        "{topics_seen:?}"
    );

    let req = fx
        .tool(
            "create_request",
            json!({
                "collectionId": col["id"],
                "parentId": folder["id"],
                "name": "Create user",
                "method": "POST",
                "url": "{{base_url}}/users",
                "headers": [{ "name": "Content-Type", "value": "application/json" }],
                "params": [{ "name": "notify", "value": "true" }],
                "body": { "kind": "json", "text": "{\"name\":\"Ada\"}" },
                "auth": { "type": "bearer", "token": "{{token}}" },
                "description": "Creates a user."
            }),
        )
        .await;
    assert_eq!(req["path"], "Users/Create user");
    assert_eq!(req["parentId"], folder["id"]);
    let data = &req["data"];
    assert_eq!(data["method"], "POST");
    assert_eq!(data["headers"][0]["name"], "Content-Type");
    assert_eq!(data["params"][0]["value"], "true");
    assert_eq!(data["body"]["kind"], "json");
    assert_eq!(
        data["auth"]["prefix"], "Bearer",
        "auth defaults are filled in"
    );
    assert_eq!(data["auth"]["token"], "{{token}}");

    // Minimal: a GET at the root, placed first.
    let first = fx
        .tool(
            "create_request",
            json!({ "collectionId": col["id"], "name": "Ping", "url": "https://x/ping", "position": "start" }),
        )
        .await;
    assert_eq!(first["data"]["method"], "GET");
    let tree = fx
        .tool("get_tree", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(tree["items"][0]["name"], "Ping");

    let err = fx
        .tool_err(
            "create_request",
            json!({ "collectionId": col["id"], "name": "Bad", "auth": { "type": "oauth1" } }),
        )
        .await;
    assert!(err.contains("missing field"), "{err}");
}

#[tokio::test]
async fn update_changes_only_given_fields_and_keeps_masked_secrets() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let req = fx
        .node(
            &col,
            None,
            "Me",
            json!({
                "kind": "request", "method": "GET", "url": "https://x/me",
                "auth": { "type": "basic", "username": "ada", "password": "pa55" },
                "headers": [{ "name": "X-Api-Key", "value": "k-1" }, { "name": "Accept", "value": "*/*" }]
            }),
        )
        .await;

    // The agent read the request (secrets masked) and sends parts back changed.
    let read = fx.tool("get_request", json!({ "id": req["id"] })).await;
    let mut headers = read["data"]["headers"].clone();
    headers[1]["value"] = json!("application/json");
    let (out, seen) = topics(
        &fx,
        fx.tool(
            "update_request",
            json!({
                "id": req["id"],
                "url": "https://x/v2/me",
                "headers": headers,
                "auth": read["data"]["auth"],
            }),
        ),
    )
    .await;
    assert_eq!(out["data"]["url"], "https://x/v2/me");
    assert_eq!(out["data"]["method"], "GET", "untouched fields stay");
    assert!(seen.contains(&"tree.changed".to_string()));

    let node = fx.cmd("node.get", json!({ "id": req["id"] })).await;
    assert_eq!(
        node["data"]["auth"]["password"], "pa55",
        "masked value kept"
    );
    assert_eq!(
        node["data"]["headers"][0]["value"], "k-1",
        "masked value kept"
    );
    assert_eq!(node["data"]["headers"][1]["value"], "application/json");

    // A real new secret is written.
    fx.tool(
        "update_request",
        json!({ "id": req["id"], "auth": { "type": "basic", "username": "ada", "password": "new" } }),
    )
    .await;
    let node = fx.cmd("node.get", json!({ "id": req["id"] })).await;
    assert_eq!(node["data"]["auth"]["password"], "new");

    let err = fx
        .tool_err("update_request", json!({ "id": req["id"], "method": 5 }))
        .await;
    assert!(err.contains("invalid"), "{err}");
}

#[tokio::test]
async fn rename_move_duplicate_and_delete() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let a = fx.node(&col, None, "A", json!({ "kind": "folder" })).await;
    let b = fx.node(&col, None, "B", json!({ "kind": "folder" })).await;
    let req = fx
        .node(
            &col,
            Some(&a),
            "Req",
            json!({ "kind": "request", "method": "GET", "url": "https://x" }),
        )
        .await;

    let out = fx
        .tool(
            "rename_item",
            json!({ "id": req["id"], "name": "Get things" }),
        )
        .await;
    assert_eq!(out["name"], "Get things");

    let out = fx
        .tool("move_item", json!({ "id": req["id"], "parentId": b["id"] }))
        .await;
    assert_eq!(out["path"], "B/Get things");
    let out = fx
        .tool(
            "move_item",
            json!({ "id": req["id"], "parentId": null, "position": "start" }),
        )
        .await;
    assert_eq!(out["path"], "Get things");
    let tree = fx
        .tool("get_tree", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(tree["items"][0]["name"], "Get things");

    let copy = fx.tool("duplicate_item", json!({ "id": req["id"] })).await;
    assert_ne!(copy["id"], req["id"]);
    assert!(copy["name"].as_str().unwrap().starts_with("Get things"));

    let (out, seen) = topics(&fx, fx.tool("delete_item", json!({ "id": a["id"] }))).await;
    assert_eq!(out["deleted"], json!([a["id"]]));
    assert!(out["note"].as_str().unwrap().contains("trash"));
    assert!(seen.contains(&"tree.changed".to_string()));
    let trash = fx
        .cmd("trash.list", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(trash[0]["id"], a["id"]);
    // Restorable like any deletion.
    fx.cmd("tree.restore", json!({ "ids": [a["id"]] })).await;
    let tree = fx
        .tool("get_tree", json!({ "collectionId": col["id"] }))
        .await;
    assert!(
        tree["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == a["id"])
    );
}

#[tokio::test]
async fn environments_read_masked_and_write_secrets() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let created = fx
        .tool(
            "create_environment",
            json!({ "collectionId": col["id"], "name": "Staging", "variables": { "base_url": "https://staging" } }),
        )
        .await;
    assert_eq!(created["kind"], "sub");
    assert_eq!(created["variables"]["base_url"], "https://staging");

    let (_, seen) = topics(
        &fx,
        fx.tool(
            "set_variables",
            json!({
                "environmentId": created["id"],
                "values": { "token": "t-123", "retries": 3 },
                "secret": ["token"]
            }),
        ),
    )
    .await;
    assert!(
        seen.contains(&"environments.changed".to_string()),
        "{seen:?}"
    );

    let vars = fx
        .tool("get_variables", json!({ "environmentId": created["id"] }))
        .await;
    assert_eq!(vars["variables"]["base_url"], "https://staging");
    assert_eq!(vars["variables"]["retries"], 3);
    assert_eq!(vars["variables"]["token"], HIDDEN);
    assert_eq!(vars["secrets"], json!(["token"]));

    // Writing the masked value back keeps the secret; removing works.
    fx.tool(
        "set_variables",
        json!({ "environmentId": created["id"], "values": { "token": HIDDEN }, "remove": ["retries"] }),
    )
    .await;
    let env = fx
        .cmd("environment.get", json!({ "id": created["id"] }))
        .await;
    assert_eq!(env["data"]["token"], "t-123");
    assert!(env["data"].get("retries").is_none());

    // Activate it, then clear.
    fx.tool(
        "set_active_environment",
        json!({ "environmentId": created["id"] }),
    )
    .await;
    let list = fx
        .tool("list_environments", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(list["activeId"], created["id"]);
    fx.tool(
        "set_active_environment",
        json!({ "environmentId": null, "collectionId": col["id"] }),
    )
    .await;
    let list = fx
        .tool("list_environments", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(list["activeId"], Value::Null);

    // A global environment: no collection.
    let ws = fx.workspace().await;
    let global = fx
        .tool(
            "create_environment",
            json!({ "workspaceId": ws["id"], "name": "Shared" }),
        )
        .await;
    assert_eq!(global["kind"], "global");
    fx.tool(
        "set_active_environment",
        json!({ "environmentId": global["id"] }),
    )
    .await;
    let list = fx
        .tool("list_environments", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(list["activeGlobalId"], global["id"]);
}

#[tokio::test]
async fn viewers_cannot_edit_through_an_agent() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Server).await);
    let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let owner = users.create("owner", "", "password1", false).await.unwrap();
    let viewer = users
        .create("viewer", "", "password1", false)
        .await
        .unwrap();
    let as_owner = Fx::with(
        ctx.clone(),
        Caller::User {
            id: owner.id,
            admin: false,
        },
    );
    let ws = as_owner.tool("list_workspaces", json!({})).await;
    assert_eq!(ws["workspaces"], json!([]));
    let ws = reqly_api::commands::dispatch_as(
        &reqly_api::commands::registry(),
        &ctx,
        Caller::User {
            id: owner.id,
            admin: false,
        },
        "workspace.create",
        json!({ "name": "Team" }),
    )
    .await
    .unwrap();
    let ws_id: Id = serde_json::from_value(ws["id"].clone()).unwrap();
    MemberRepo::new(&ctx.storage, ctx.clock.as_ref())
        .set(ws_id, viewer.id, Role::Viewer)
        .await
        .unwrap();
    let col = reqly_api::commands::dispatch_as(
        &reqly_api::commands::registry(),
        &ctx,
        Caller::User {
            id: owner.id,
            admin: false,
        },
        "collection.create",
        json!({ "workspaceId": ws["id"], "name": "C" }),
    )
    .await
    .unwrap();

    let as_viewer = Fx::with(
        ctx.clone(),
        Caller::User {
            id: viewer.id,
            admin: false,
        },
    );
    let tree = as_viewer
        .tool("get_tree", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(tree["items"], json!([]));
    let err = as_viewer
        .tool_err(
            "create_request",
            json!({ "collectionId": col["id"], "name": "X" }),
        )
        .await;
    assert!(err.contains("access.denied"), "{err}");

    let created = as_owner
        .tool(
            "create_request",
            json!({ "collectionId": col["id"], "name": "X" }),
        )
        .await;
    assert_eq!(created["name"], "X");
}
