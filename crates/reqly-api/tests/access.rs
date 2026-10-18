//! Multi-user authorization (ADR-010): the role × command matrix.

use reqly_api::Registry;
use reqly_api::access::{Caller, Need, RULES, need};
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_core::{CoreError, Id};
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    registry: Registry<AppContext>,
    owner: Caller,
    editor: Caller,
    viewer: Caller,
    outsider: Caller,
    admin: Caller,
    ws: Value,
    col: Value,
    req: Value,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests_on(Platform::Server).await;
        let registry = registry();
        let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut callers = Vec::new();
        for (name, admin) in [
            ("owner", false),
            ("editor", false),
            ("viewer", false),
            ("outsider", false),
            ("admin", true),
        ] {
            let user = users.create(name, "", "password1", admin).await.unwrap();
            callers.push(Caller::User { id: user.id, admin });
        }
        let [owner, editor, viewer, outsider, admin] = callers[..] else {
            unreachable!()
        };
        let mut fx = Self {
            ctx,
            registry,
            owner,
            editor,
            viewer,
            outsider,
            admin,
            ws: Value::Null,
            col: Value::Null,
            req: Value::Null,
        };
        fx.ws = fx
            .ok(owner, "workspace.create", json!({ "name": "Team" }))
            .await["id"]
            .clone();
        let members = MemberRepo::new(&fx.ctx.storage, fx.ctx.clock.as_ref());
        let ws: Id = serde_json::from_value(fx.ws.clone()).unwrap();
        members
            .set(ws, editor.user_id().unwrap(), Role::Editor)
            .await
            .unwrap();
        members
            .set(ws, viewer.user_id().unwrap(), Role::Viewer)
            .await
            .unwrap();
        fx.col = fx
            .ok(
                owner,
                "collection.create",
                json!({ "workspaceId": fx.ws, "name": "C" }),
            )
            .await["id"]
            .clone();
        fx.req = fx
            .ok(
                owner,
                "node.create",
                json!({ "collectionId": fx.col, "parentId": null, "name": "R",
                        "data": { "kind": "request", "method": "GET", "url": "http://127.0.0.1:9/" } }),
            )
            .await["id"]
            .clone();
        fx
    }

    async fn call(&self, caller: Caller, name: &str, input: Value) -> Result<Value, CoreError> {
        dispatch_as(&self.registry, &self.ctx, caller, name, input).await
    }

    async fn ok(&self, caller: Caller, name: &str, input: Value) -> Value {
        self.call(caller, name, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {input}: {e:?}"))
    }

    async fn code(&self, caller: Caller, name: &str, input: Value) -> String {
        match self.call(caller, name, input).await {
            Ok(v) => panic!("{name} succeeded: {v}"),
            Err(e) => e.code,
        }
    }
}

#[test]
fn every_command_has_an_access_rule() {
    let registry = registry();
    let names: std::collections::BTreeSet<&str> = registry.names().collect();
    let ruled: std::collections::BTreeSet<&str> = RULES.iter().map(|(n, _)| *n).collect();
    let missing: Vec<_> = names.difference(&ruled).collect();
    let stale: Vec<_> = ruled.difference(&names).collect();
    assert!(missing.is_empty(), "commands without a rule: {missing:?}");
    assert!(stale.is_empty(), "rules for unknown commands: {stale:?}");
}

/// Every workspace command against every role, through the id lookup only
/// (no command runs): `{ "id": <request> }` points into the workspace.
#[tokio::test]
async fn role_command_matrix() {
    let fx = Fx::new().await;
    let input = json!({ "id": fx.req });
    for (name, rule) in RULES {
        for (caller, role) in [
            (fx.owner, Some(Role::Owner)),
            (fx.editor, Some(Role::Editor)),
            (fx.viewer, Some(Role::Viewer)),
            (fx.outsider, None),
        ] {
            let result = reqly_api::access::authorize(&fx.ctx.storage, caller, name, &input).await;
            let expected = match (rule, role) {
                (Need::Global | Need::Account, _) => Ok(()),
                (Need::Admin, _) => Err("access.denied"),
                (_, None) => Err("not_found"),
                (Need::Read, Some(_)) => Ok(()),
                (Need::Write, Some(r)) if r >= Role::Editor => Ok(()),
                (Need::Manage, Some(Role::Owner)) => Ok(()),
                _ => Err("access.denied"),
            };
            assert_eq!(
                result.map_err(|e| e.code).as_ref().map_err(String::as_str),
                expected.as_ref().map_err(|e| *e),
                "{name} as {role:?}"
            );
        }
        let admin = reqly_api::access::authorize(&fx.ctx.storage, fx.admin, name, &input).await;
        assert!(admin.is_ok(), "{name} as admin: {admin:?}");
        let local =
            reqly_api::access::authorize(&fx.ctx.storage, Caller::Local, name, &input).await;
        assert!(local.is_ok(), "{name} locally");
    }
}

#[tokio::test]
async fn real_commands_follow_roles() {
    let fx = Fx::new().await;
    let update = json!({ "id": fx.req, "name": "Renamed" });

    // Reading.
    for caller in [fx.owner, fx.editor, fx.viewer] {
        fx.ok(caller, "tree.list", json!({ "collectionId": fx.col }))
            .await;
        fx.ok(caller, "node.get", json!({ "id": fx.req })).await;
    }
    assert_eq!(
        fx.code(fx.outsider, "tree.list", json!({ "collectionId": fx.col }))
            .await,
        "not_found"
    );

    // Writing.
    assert_eq!(
        fx.code(fx.viewer, "node.update", update.clone()).await,
        "access.denied"
    );
    fx.ok(fx.editor, "node.update", update).await;
    assert_eq!(
        fx.code(
            fx.viewer,
            "tree.move",
            json!({ "ids": [fx.req], "collectionId": fx.col, "parentId": null })
        )
        .await,
        "access.denied"
    );

    // Owning.
    let rename = json!({ "id": fx.ws, "name": "Renamed" });
    assert_eq!(
        fx.code(fx.editor, "workspace.rename", rename.clone()).await,
        "access.denied"
    );
    fx.ok(fx.owner, "workspace.rename", rename).await;

    // Ids from two workspaces need a role in both.
    let other = fx
        .ok(fx.outsider, "workspace.create", json!({ "name": "Mine" }))
        .await["id"]
        .clone();
    assert_eq!(
        fx.code(
            fx.outsider,
            "tree.move",
            json!({ "ids": [fx.req], "collectionId": other, "parentId": null })
        )
        .await,
        "not_found"
    );
    assert_eq!(
        fx.code(fx.viewer, "node.get", json!({ "id": "garbage" }))
            .await,
        "not_found"
    );

    // Administration.
    assert_eq!(
        fx.code(fx.owner, "admin.users", json!({})).await,
        "access.denied"
    );
    let users = fx.ok(fx.admin, "admin.users", json!({})).await;
    assert_eq!(users.as_array().unwrap().len(), 5);
    assert_eq!(
        fx.code(fx.owner, "no.such", json!({})).await,
        "command.unknown"
    );
}

#[tokio::test]
async fn workspaces_are_listed_per_member() {
    let fx = Fx::new().await;
    let names = |v: Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|w| w["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        names(fx.ok(fx.viewer, "workspace.list", json!({})).await),
        ["Team"]
    );
    // A new user gets a workspace of their own.
    let own = fx
        .ok(fx.outsider, "workspace.ensureDefault", json!({}))
        .await;
    assert_eq!(names(own.clone()), ["My Workspace"]);
    let again = fx
        .ok(fx.outsider, "workspace.ensureDefault", json!({}))
        .await;
    assert_eq!(own, again);
    let me = fx.ok(fx.outsider, "account.me", json!({})).await;
    assert_eq!(me["user"]["username"], "outsider");
    assert_eq!(me["roles"][own[0]["id"].as_str().unwrap()], "owner");
}

#[tokio::test]
async fn members_are_managed_by_owners() {
    let fx = Fx::new().await;
    let members = fx
        .ok(
            fx.owner,
            "members.set",
            json!({ "workspaceId": fx.ws, "username": "outsider", "role": "viewer" }),
        )
        .await;
    assert_eq!(members.as_array().unwrap().len(), 4);
    fx.ok(fx.outsider, "tree.list", json!({ "collectionId": fx.col }))
        .await;
    assert_eq!(
        fx.code(
            fx.editor,
            "members.set",
            json!({ "workspaceId": fx.ws, "username": "outsider", "role": "owner" })
        )
        .await,
        "access.denied"
    );
    assert_eq!(
        fx.code(
            fx.owner,
            "members.set",
            json!({ "workspaceId": fx.ws, "username": "ghost", "role": "viewer" })
        )
        .await,
        "member.unknown_user"
    );
    fx.ok(
        fx.outsider,
        "members.leave",
        json!({ "workspaceId": fx.ws }),
    )
    .await;
    assert_eq!(
        fx.code(fx.outsider, "tree.list", json!({ "collectionId": fx.col }))
            .await,
        "not_found"
    );
    assert_eq!(
        fx.code(fx.owner, "members.leave", json!({ "workspaceId": fx.ws }))
            .await,
        "member.last_owner"
    );
    let viewer = fx.viewer.user_id().unwrap();
    fx.ok(
        fx.owner,
        "members.remove",
        json!({ "workspaceId": fx.ws, "userId": viewer }),
    )
    .await;
    assert_eq!(
        fx.code(fx.viewer, "node.get", json!({ "id": fx.req }))
            .await,
        "not_found"
    );
}

#[tokio::test]
async fn settings_and_active_environments_are_per_user() {
    let fx = Fx::new().await;
    fx.ok(
        fx.editor,
        "settings.set",
        json!({ "key": "ui.theme", "value": "dark" }),
    )
    .await;
    let theme = |caller| fx.ok(caller, "settings.get", json!({ "key": "ui.theme" }));
    assert_eq!(theme(fx.editor).await["value"], "dark");
    assert_eq!(theme(fx.viewer).await["value"], Value::Null);

    let env = fx
        .ok(
            fx.editor,
            "environment.create",
            json!({ "workspaceId": fx.ws, "collectionId": fx.col, "kind": "sub", "name": "Prod" }),
        )
        .await;
    fx.ok(
        fx.viewer,
        "environment.setActive",
        json!({ "collectionId": fx.col, "environmentId": env["id"] }),
    )
    .await;
    let active = |caller| {
        fx.ok(
            caller,
            "environment.list",
            json!({ "collectionId": fx.col }),
        )
    };
    assert_eq!(active(fx.viewer).await["activeId"], env["id"]);
    assert_eq!(active(fx.editor).await["activeId"], Value::Null);
}

#[tokio::test]
async fn accounts_and_tokens() {
    let fx = Fx::new().await;
    let created = fx
        .ok(fx.viewer, "account.createToken", json!({ "name": "cli" }))
        .await;
    assert!(created["secret"].as_str().unwrap().starts_with("rq_"));
    let tokens = fx.ok(fx.viewer, "account.tokens", json!({})).await;
    assert_eq!(tokens.as_array().unwrap().len(), 1);
    // Other users cannot delete it.
    assert_eq!(
        fx.code(
            fx.editor,
            "account.deleteToken",
            json!({ "tokenId": created["token"]["id"] })
        )
        .await,
        "not_found"
    );
    fx.ok(
        fx.viewer,
        "account.deleteToken",
        json!({ "tokenId": created["token"]["id"] }),
    )
    .await;

    assert_eq!(
        fx.code(
            fx.viewer,
            "account.changePassword",
            json!({ "current": "wrong", "new": "password2" })
        )
        .await,
        "auth.invalid"
    );
    fx.ok(
        fx.viewer,
        "account.changePassword",
        json!({ "current": "password1", "new": "password2" }),
    )
    .await;
    let me = fx
        .ok(fx.viewer, "account.update", json!({ "displayName": "Vee" }))
        .await;
    assert_eq!(me["displayName"], "Vee");
    assert_eq!(
        fx.code(Caller::Local, "account.tokens", json!({})).await,
        "account.unavailable"
    );
    assert_eq!(need("account.me"), Need::Account);
}

#[tokio::test]
async fn invitations_add_members() {
    let fx = Fx::new().await;
    let created = fx
        .ok(
            fx.owner,
            "members.invite",
            json!({ "workspaceId": fx.ws, "role": "editor" }),
        )
        .await;
    let token = created["token"].clone();
    assert_eq!(
        fx.code(
            fx.editor,
            "members.invite",
            json!({ "workspaceId": fx.ws, "role": "owner" })
        )
        .await,
        "access.denied"
    );
    let open = fx
        .ok(fx.owner, "members.invites", json!({ "workspaceId": fx.ws }))
        .await;
    assert_eq!(open.as_array().unwrap().len(), 1);

    fx.ok(fx.outsider, "invites.accept", json!({ "token": token }))
        .await;
    fx.ok(
        fx.outsider,
        "node.update",
        json!({ "id": fx.req, "name": "Mine" }),
    )
    .await;
    assert_eq!(
        fx.code(fx.viewer, "invites.accept", json!({ "token": token }))
            .await,
        "invite.invalid"
    );

    let other = fx
        .ok(
            fx.owner,
            "members.invite",
            json!({ "workspaceId": fx.ws, "role": "viewer" }),
        )
        .await;
    fx.ok(
        fx.owner,
        "members.revokeInvite",
        json!({ "workspaceId": fx.ws, "inviteId": other["invite"]["id"] }),
    )
    .await;

    assert_eq!(
        fx.code(fx.owner, "admin.invite", json!({})).await,
        "access.denied"
    );
    let signup = fx.ok(fx.admin, "admin.invite", json!({})).await;
    let listed = fx.ok(fx.admin, "admin.invites", json!({})).await;
    assert_eq!(listed[0]["id"], signup["invite"]["id"]);
}
