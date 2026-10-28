//! Who is calling a command and whether they may (ADR-010).
//!
//! Desktop and the single-user server modes call as [`Caller::Local`] with
//! full access. In the `multi` server mode every call runs as a
//! [`Caller::User`]: [`authorize`] derives the workspaces the call touches
//! from the ids in its input and checks the caller's role there against the
//! [`Need`] of the command.

use std::collections::BTreeSet;
use std::future::Future;

use reqly_core::{CoreError, CoreResult, Id};
use reqly_storage::Storage;
use reqly_storage::settings::SettingsRepo;
use reqly_storage::users::{MemberRepo, Role, workspaces_of_ids};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Caller {
    /// The desktop app or a single-user server: full access.
    #[default]
    Local,
    User {
        id: Id,
        admin: bool,
    },
}

tokio::task_local! {
    static CALLER: Caller;
}

/// The caller of the command running on this task.
pub fn current() -> Caller {
    CALLER.try_with(|c| *c).unwrap_or_default()
}

/// Runs `f` (a command) as `caller`.
pub async fn scope<F: Future>(caller: Caller, f: F) -> F::Output {
    CALLER.scope(caller, f).await
}

impl Caller {
    pub fn user_id(self) -> Option<Id> {
        match self {
            Caller::Local => None,
            Caller::User { id, .. } => Some(id),
        }
    }
}

/// What a command needs from its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// No workspace: any signed-in user. Commands that list or create
    /// workspaces, or own handles (streams, OAuth flows), check the caller
    /// themselves.
    Global,
    /// A signed-in user's own account.
    Account,
    /// Viewer in every workspace the input refers to.
    Read,
    /// Editor.
    Write,
    /// Owner.
    Manage,
    /// Server administrator.
    Admin,
}

impl Need {
    fn role(self) -> Option<Role> {
        match self {
            Need::Read => Some(Role::Viewer),
            Need::Write => Some(Role::Editor),
            Need::Manage => Some(Role::Owner),
            _ => None,
        }
    }
}

/// The access rule of every command. A command missing here needs
/// [`Need::Admin`]; a test keeps this table and the registry in step.
pub const RULES: &[(&str, Need)] = {
    use Need::*;
    &[
        ("app.info", Global),
        ("app.ping", Global),
        ("settings.get", Global),
        ("settings.set", Global),
        ("template.functions", Global),
        ("curl.parse", Global),
        ("codegen.targets", Global),
        ("import.preview", Global),
        ("import.fromUrl", Global),
        ("blob.upload", Global),
        ("file.writeText", Global),
        ("workspace.list", Global),
        ("workspace.ensureDefault", Global),
        ("workspace.create", Global),
        ("oauth2.complete", Global),
        ("oauth2.devicePoll", Global),
        ("stream.send", Global),
        ("stream.disconnect", Global),
        ("stream.list", Global),
        ("stream.log", Global),
        ("stream.clearLog", Global),
        ("stream.end", Global),
        ("runner.cancel", Global),
        ("runner.report", Global),
        ("account.me", Account),
        ("account.update", Account),
        ("account.changePassword", Account),
        ("account.tokens", Account),
        ("account.createToken", Account),
        ("account.deleteToken", Account),
        ("invites.accept", Account),
        // Reading and using a workspace.
        ("collection.list", Read),
        ("collection.get", Read),
        ("tree.list", Read),
        ("node.get", Read),
        ("trash.list", Read),
        ("search.nodes", Read),
        ("request.send", Read),
        ("runner.run", Read),
        ("request.cancel", Read),
        ("request.toCurl", Read),
        ("response.list", Read),
        ("response.get", Read),
        ("response.body", Read),
        ("response.filter", Read),
        ("response.saveBody", Read),
        ("history.workspace", Read),
        ("history.sharing", Read),
        ("cookies.list", Read),
        ("environment.list", Read),
        // The active environment is a setting of the caller.
        ("environment.setActive", Read),
        ("environment.setActiveGlobal", Read),
        ("template.preview", Read),
        ("template.variables", Read),
        ("template.prompts", Read),
        ("codegen.generate", Read),
        ("export.run", Read),
        ("oauth2.status", Read),
        ("oauth2.fetch", Read),
        ("oauth2.clear", Read),
        ("oauth2.authorize", Read),
        ("oauth2.deviceStart", Read),
        ("graphql.introspect", Read),
        ("graphql.schemaGet", Read),
        ("grpc.describe", Read),
        ("stream.connect", Read),
        ("variables.runtime", Read),
        ("members.list", Read),
        ("members.leave", Read),
        ("collab.join", Read),
        ("presence.set", Read),
        ("collab.awareness", Read),
        // Changing a workspace.
        ("collection.create", Write),
        ("collection.rename", Write),
        ("collection.updateSettings", Write),
        ("collection.updateScripts", Write),
        ("collection.delete", Write),
        ("node.create", Write),
        ("node.update", Write),
        ("collab.update", Write),
        ("tree.move", Write),
        ("tree.copy", Write),
        ("tree.duplicate", Write),
        ("tree.delete", Write),
        ("tree.restore", Write),
        ("trash.purge", Write),
        ("environment.create", Write),
        ("environment.update", Write),
        ("environment.duplicate", Write),
        ("environment.delete", Write),
        ("import.apply", Write),
        ("graphql.schemaSet", Write),
        ("graphql.schemaClear", Write),
        ("cookies.upsert", Write),
        ("cookies.delete", Write),
        ("cookies.clear", Write),
        ("response.delete", Write),
        ("history.clear", Write),
        ("variables.clearRuntime", Write),
        // Owning a workspace.
        ("workspace.rename", Manage),
        ("workspace.delete", Manage),
        ("members.set", Manage),
        ("members.remove", Manage),
        ("history.setSharing", Manage),
        ("members.invite", Manage),
        ("members.invites", Manage),
        ("members.revokeInvite", Manage),
        // Administration.
        ("maintenance.cleanup", Admin),
        ("admin.users", Admin),
        ("admin.createUser", Admin),
        ("admin.updateUser", Admin),
        ("admin.deleteUser", Admin),
        ("admin.workspaces", Admin),
        ("admin.server", Admin),
        ("admin.invite", Admin),
        ("admin.invites", Admin),
        ("admin.revokeInvite", Admin),
    ]
};

pub fn need(command: &str) -> Need {
    RULES
        .iter()
        .find(|(name, _)| *name == command)
        .map_or(Need::Admin, |(_, need)| *need)
}

/// Input fields that hold handles or users, not workspace entities.
const NOT_ENTITIES: &[&str] = &[
    "connectionId",
    "flowId",
    "userId",
    "tokenId",
    "inviteId",
    "docId",
    "clientId",
    "runId",
];

fn is_id_field(key: &str) -> bool {
    !NOT_ENTITIES.contains(&key)
        && (matches!(key, "id" | "ids") || key.ends_with("Id") || key.ends_with("Ids"))
}

/// Entity ids in a command input: its top-level id fields and those of an
/// export `scope`. Nested request data (headers carry ids of their own) is
/// not looked at. `Err` when an id field holds something else.
pub fn input_ids(input: &Value) -> CoreResult<BTreeSet<Id>> {
    let bad = || CoreError::new("not_found", "not found");
    let mut out = BTreeSet::new();
    let Some(fields) = input.as_object() else {
        return Ok(out);
    };
    let scope = fields.get("scope").and_then(Value::as_object);
    for (key, value) in fields.iter().chain(scope.into_iter().flatten()) {
        if !is_id_field(key) {
            continue;
        }
        let values = match value {
            Value::Null => continue,
            Value::Array(items) => items.iter().collect(),
            one => vec![one],
        };
        for v in values {
            let id = v.as_str().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
            out.insert(id);
        }
    }
    Ok(out)
}

fn denied(need: Need) -> CoreError {
    CoreError::new("access.denied", "you are not allowed to do this").with_param(
        "need",
        match need {
            Need::Read => "viewer",
            Need::Write => "editor",
            Need::Manage => "owner",
            Need::Admin => "admin",
            Need::Global | Need::Account => "user",
        },
    )
}

/// The caller's role in a workspace; administrators own every workspace.
pub async fn role_in(
    storage: &Storage,
    caller: Caller,
    workspace_id: Id,
) -> CoreResult<Option<Role>> {
    match caller {
        Caller::Local | Caller::User { admin: true, .. } => Ok(Some(Role::Owner)),
        Caller::User { id, .. } => {
            MemberRepo::new(storage, &reqly_core::SystemClock)
                .role(workspace_id, id)
                .await
        }
    }
}

/// Checks that `caller` may run `command` with `input`.
pub async fn authorize(
    storage: &Storage,
    caller: Caller,
    command: &str,
    input: &Value,
) -> CoreResult<()> {
    let Caller::User { admin, .. } = caller else {
        return Ok(());
    };
    let need = need(command);
    let Some(required) = need.role() else {
        return match need {
            Need::Admin if !admin => Err(denied(need)),
            _ => Ok(()),
        };
    };
    let ids = input_ids(input)?;
    if ids.is_empty() {
        // A workspace command without a workspace: refuse rather than guess.
        return Err(denied(need));
    }
    let ids: Vec<Id> = ids.into_iter().collect();
    let found = workspaces_of_ids(storage, &ids).await?;
    if found.len() < ids.len() {
        return Err(CoreError::new("not_found", "not found"));
    }
    let workspaces: BTreeSet<Id> = found.into_values().collect();
    for ws in workspaces {
        match role_in(storage, caller, ws).await? {
            // Not a member: the workspace does not exist for the caller.
            None => return Err(CoreError::new("not_found", "not found")),
            Some(role) if role < required => return Err(denied(need)),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Events about shared workspace content; everyone in the workspace gets
/// them. Other events go only to the user who caused them, except
/// `workspaces.changed`, which carries nothing and asks every client to
/// reload its list.
const SHARED_EVENTS: &[&str] = &[
    "tree.changed",
    "collections.changed",
    "environments.changed",
    "cookies.changed",
    "variables.runtimeChanged",
    "collab.update",
    "collab.awareness",
    "presence.changed",
];

/// Whether `caller` may receive `event`. `member_of` holds the caller's
/// workspaces (ignored for local callers and administrators).
pub async fn event_visible(
    storage: &Storage,
    caller: Caller,
    member_of: &std::collections::HashSet<Id>,
    event: &crate::Event,
) -> bool {
    let Caller::User { id, admin } = caller else {
        return true;
    };
    if event.topic == "workspaces.changed" {
        return true;
    }
    if !SHARED_EVENTS.contains(&event.topic.as_str()) {
        return event.actor == Some(id);
    }
    if admin {
        return true;
    }
    let Ok(ids) = input_ids(&event.payload) else {
        return false;
    };
    let ids: Vec<Id> = ids.into_iter().collect();
    match workspaces_of_ids(storage, &ids).await {
        Ok(found) => !found.is_empty() && found.values().all(|ws| member_of.contains(ws)),
        Err(_) => false,
    }
}

/// The settings key for the caller: users keep their own settings.
pub fn settings_key(key: &str) -> String {
    match current() {
        Caller::Local => key.to_string(),
        Caller::User { id, .. } => format!("user:{id}:{key}"),
    }
}

/// Reads a setting of the caller, falling back to the server-wide value.
pub async fn setting(storage: &Storage, key: &str) -> CoreResult<Option<Value>> {
    let repo = SettingsRepo::new(storage);
    let own = settings_key(key);
    if own != key
        && let Some(v) = repo.get(&own).await?
    {
        return Ok(Some(v));
    }
    repo.get(key).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn collects_ids_from_input() {
        let a = Id::new();
        let b = Id::new();
        let ids = input_ids(&json!({
            "id": a, "parentId": null, "ids": [b], "connectionId": "c1", "name": "x",
            "data": { "headers": [{ "id": Id::new() }] }
        }))
        .unwrap();
        assert_eq!(ids, BTreeSet::from([a, b]));
        let ids = input_ids(&json!({ "scope": { "kind": "nodes", "nodeIds": [a] } })).unwrap();
        assert_eq!(ids, BTreeSet::from([a]));
        assert!(input_ids(&json!({ "id": "not-an-id" })).is_err());
        assert!(input_ids(&json!({ "userId": "u" })).unwrap().is_empty());
    }

    #[test]
    fn unknown_commands_need_admin() {
        assert_eq!(need("node.get"), Need::Read);
        assert_eq!(need("something.new"), Need::Admin);
    }

    #[tokio::test]
    async fn caller_is_task_local() {
        assert_eq!(current(), Caller::Local);
        let user = Caller::User {
            id: Id::new(),
            admin: false,
        };
        assert_eq!(scope(user, async { current() }).await, user);
        let key = scope(user, async { settings_key("ui") }).await;
        assert_eq!(key, format!("user:{}:ui", user.user_id().unwrap()));
    }
}
