//! `sync.pull` and `sync.push` within workspace roles (ADR-013).

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_core::Id;
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    registry: reqly_api::Registry<AppContext>,
    owner: Caller,
    viewer: Caller,
    outsider: Caller,
    ws: Id,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests_on(Platform::Server).await;
        let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut callers = Vec::new();
        for name in ["owner", "viewer", "outsider"] {
            let user = users.create(name, "", "password1", false).await.unwrap();
            callers.push(Caller::User {
                id: user.id,
                admin: false,
            });
        }
        let [owner, viewer, outsider] = callers[..] else {
            unreachable!()
        };
        let registry = registry();
        let ws = dispatch_as(
            &registry,
            &ctx,
            owner,
            "workspace.create",
            json!({ "name": "Team" }),
        )
        .await
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        dispatch_as(
            &registry,
            &ctx,
            owner,
            "collection.create",
            json!({ "workspaceId": ws, "name": "API" }),
        )
        .await
        .unwrap();
        MemberRepo::new(&ctx.storage, ctx.clock.as_ref())
            .set(ws, viewer.user_id().unwrap(), Role::Viewer)
            .await
            .unwrap();
        Self {
            ctx,
            registry,
            owner,
            viewer,
            outsider,
            ws,
        }
    }

    async fn call(&self, caller: Caller, name: &str, input: Value) -> Value {
        dispatch_as(&self.registry, &self.ctx, caller, name, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {input}: {e:?}"))
    }

    async fn pull(&self, caller: Caller) -> Value {
        self.call(
            caller,
            "sync.pull",
            json!({ "cursors": [{ "workspaceId": self.ws, "seq": 0 }] }),
        )
        .await
    }
}

#[tokio::test]
async fn members_pull_their_workspaces_only() {
    let fx = Fx::new().await;
    let out = fx.pull(fx.viewer).await;
    let kinds: Vec<&str> = out["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"workspace") && kinds.contains(&"collection"),
        "{kinds:?}"
    );
    assert!(out["cursors"][0]["seq"].as_u64().unwrap() > 0);

    let out = fx.pull(fx.outsider).await;
    assert_eq!(out["records"], json!([]));
    assert_eq!(out["unavailable"], json!([fx.ws]));
}

#[tokio::test]
async fn pushes_need_the_editor_role() {
    let fx = Fx::new().await;
    let mut collection = fx.pull(fx.owner).await["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "collection")
        .unwrap()
        .clone();
    let base = collection.clone();
    collection["fields"]["name"] = json!("Renamed");
    let push = json!({ "changes": [{ "record": collection, "base": base }] });

    for caller in [fx.viewer, fx.outsider] {
        let out = fx.call(caller, "sync.push", push.clone()).await;
        assert_eq!(out["results"][0]["status"], "rejected");
        assert_eq!(out["results"][0]["code"], "access.denied");
    }
    let out = fx.call(fx.owner, "sync.push", push).await;
    assert_eq!(out["results"][0]["status"], "applied");
    let cols = fx
        .call(
            fx.viewer,
            "collection.list",
            json!({ "workspaceId": fx.ws }),
        )
        .await;
    assert_eq!(cols[0]["name"], "Renamed");
}

#[tokio::test]
async fn a_pushed_new_workspace_belongs_to_its_author() {
    let fx = Fx::new().await;
    let ws = Id::new();
    let col = Id::new();
    let push = json!({ "changes": [
        { "record": { "kind": "collection", "id": col, "workspaceId": ws, "deleted": false,
            "fields": { "name": "Mine", "sortKey": "V", "settings": {}, "scripts": {} } }, "base": null },
        { "record": { "kind": "workspace", "id": ws, "workspaceId": ws, "deleted": false,
            "fields": { "name": "Uploaded", "sortKey": "V" } }, "base": null },
    ] });
    let out = fx.call(fx.outsider, "sync.push", push).await;
    assert!(
        out["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "applied"),
        "{out}"
    );
    let role = MemberRepo::new(&fx.ctx.storage, fx.ctx.clock.as_ref())
        .role(ws, fx.outsider.user_id().unwrap())
        .await
        .unwrap();
    assert_eq!(role, Some(Role::Owner));
    let list = fx.call(fx.outsider, "workspace.list", json!({})).await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|w| w["name"] == "Uploaded")
    );
    // Nobody else sees it.
    let out = fx
        .call(
            fx.owner,
            "sync.pull",
            json!({ "cursors": [{ "workspaceId": ws, "seq": 0 }] }),
        )
        .await;
    assert_eq!(out["unavailable"], json!([ws]));
}

#[tokio::test]
async fn client_commands_need_a_connection() {
    let ctx = AppContext::for_tests().await;
    let registry = registry();
    let status = dispatch_as(&registry, &ctx, Caller::Local, "sync.status", json!({}))
        .await
        .unwrap();
    assert_eq!(status["url"], Value::Null);
    let err = dispatch_as(&registry, &ctx, Caller::Local, "sync.now", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code, "sync.not_connected");
    let err = dispatch_as(
        &registry,
        &ctx,
        Caller::Local,
        "sync.connect",
        json!({ "url": "ftp://x" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "sync.url");
}
