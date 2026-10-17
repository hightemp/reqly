//! Workspace, collection and tree commands.

use std::collections::BTreeSet;

use reqly_core::model::{Collection, Node, NodeData, NodeSummary, RequestSettings, Workspace};
use reqly_core::{CoreResult, Id, Version};
use reqly_storage::tree::{Placement, TreeRepo};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<WorkspaceList>()
        .register::<WorkspaceEnsureDefault>()
        .register::<WorkspaceCreate>()
        .register::<WorkspaceRename>()
        .register::<WorkspaceDelete>()
        .register::<CollectionList>()
        .register::<CollectionGet>()
        .register::<CollectionCreate>()
        .register::<CollectionRename>()
        .register::<CollectionUpdateSettings>()
        .register::<CollectionDelete>()
        .register::<TreeList>()
        .register::<NodeGet>()
        .register::<NodeCreate>()
        .register::<NodeUpdate>()
        .register::<TreeMove>()
        .register::<TreeCopy>()
        .register::<TreeDuplicate>()
        .register::<TreeDelete>()
        .register::<TreeRestore>()
        .register::<TrashList>()
        .register::<TrashPurge>();
}

fn repo(ctx: &AppContext) -> TreeRepo<'_> {
    TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
}

// ----- events ---------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspacesChanged {}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionsChanged {
    pub workspace_id: Id,
}

/// Something in these collections' trees changed; clients reload them.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TreeChanged {
    pub collection_ids: Vec<Id>,
    /// Nodes whose content changed (open editors may need to reload).
    pub node_ids: Vec<Id>,
}

fn workspaces_changed(ctx: &AppContext) {
    ctx.events
        .publish(Event::new("workspaces.changed", WorkspacesChanged {}));
}

fn collections_changed(ctx: &AppContext, workspace_id: Id) {
    ctx.events.publish(Event::new(
        "collections.changed",
        CollectionsChanged { workspace_id },
    ));
}

fn tree_changed(ctx: &AppContext, collections: impl IntoIterator<Item = Id>, nodes: Vec<Id>) {
    let collection_ids: Vec<Id> = collections
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ctx.events.publish(Event::new(
        "tree.changed",
        TreeChanged {
            collection_ids,
            node_ids: nodes,
        },
    ));
}

async fn collections_of(ctx: &AppContext, ids: &[Id]) -> CoreResult<Vec<Id>> {
    let mut out = Vec::new();
    for &id in ids {
        out.push(repo(ctx).get_node(id).await?.collection_id);
    }
    Ok(out)
}

// ----- shared input types ---------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum ApiPlacement {
    Start,
    #[default]
    End,
    Before {
        id: Id,
    },
    After {
        id: Id,
    },
}

impl From<ApiPlacement> for Placement {
    fn from(p: ApiPlacement) -> Self {
        match p {
            ApiPlacement::Start => Placement::Start,
            ApiPlacement::End => Placement::End,
            ApiPlacement::Before { id } => Placement::Before(id),
            ApiPlacement::After { id } => Placement::After(id),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct IdInput {
    pub id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct IdsInput {
    pub ids: Vec<Id>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RenameInput {
    pub id: Id,
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct NameInput {
    pub name: String,
}

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

// ----- workspaces -----------------------------------------------------------

command!(
    WorkspaceList,
    "workspace.list",
    Empty,
    Vec<Workspace>,
    |ctx, _input| { repo(ctx).list_workspaces().await }
);

command!(
    /// Returns workspaces, creating "My Workspace" with one collection on first run.
    WorkspaceEnsureDefault,
    "workspace.ensureDefault",
    Empty,
    Vec<Workspace>,
    |ctx, _input| {
        let repo = repo(ctx);
        let existing = repo.list_workspaces().await?;
        if !existing.is_empty() {
            return Ok(existing);
        }
        let ws = repo.create_workspace("My Workspace").await?;
        repo.create_collection(ws.meta.id, "My Collection").await?;
        workspaces_changed(ctx);
        repo.list_workspaces().await
    }
);

command!(
    WorkspaceCreate,
    "workspace.create",
    NameInput,
    Workspace,
    |ctx, input| {
        let ws = repo(ctx).create_workspace(&input.name).await?;
        workspaces_changed(ctx);
        Ok(ws)
    }
);

command!(
    WorkspaceRename,
    "workspace.rename",
    RenameInput,
    Empty,
    |ctx, input| {
        repo(ctx).rename_workspace(input.id, &input.name).await?;
        workspaces_changed(ctx);
        Ok(Empty {})
    }
);

command!(
    WorkspaceDelete,
    "workspace.delete",
    IdInput,
    Empty,
    |ctx, input| {
        repo(ctx).delete_workspace(input.id).await?;
        workspaces_changed(ctx);
        Ok(Empty {})
    }
);

// ----- collections ----------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspaceIdInput {
    pub workspace_id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionCreateInput {
    pub workspace_id: Id,
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CollectionSettingsInput {
    pub id: Id,
    pub settings: RequestSettings,
}

command!(
    CollectionList,
    "collection.list",
    WorkspaceIdInput,
    Vec<Collection>,
    |ctx, input| { repo(ctx).list_collections(input.workspace_id).await }
);

command!(
    CollectionGet,
    "collection.get",
    IdInput,
    Collection,
    |ctx, input| { repo(ctx).get_collection(input.id).await }
);

command!(
    CollectionCreate,
    "collection.create",
    CollectionCreateInput,
    Collection,
    |ctx, input| {
        let col = repo(ctx)
            .create_collection(input.workspace_id, &input.name)
            .await?;
        collections_changed(ctx, input.workspace_id);
        Ok(col)
    }
);

command!(
    CollectionRename,
    "collection.rename",
    RenameInput,
    Empty,
    |ctx, input| {
        let repo = repo(ctx);
        repo.rename_collection(input.id, &input.name).await?;
        collections_changed(ctx, repo.get_collection(input.id).await?.workspace_id);
        Ok(Empty {})
    }
);

command!(
    CollectionUpdateSettings,
    "collection.updateSettings",
    CollectionSettingsInput,
    Empty,
    |ctx, input| {
        repo(ctx)
            .update_collection_settings(input.id, &input.settings)
            .await?;
        Ok(Empty {})
    }
);

command!(
    CollectionDelete,
    "collection.delete",
    IdInput,
    Empty,
    |ctx, input| {
        let repo = repo(ctx);
        let ws = repo.get_collection(input.id).await?.workspace_id;
        repo.delete_collection(input.id).await?;
        collections_changed(ctx, ws);
        Ok(Empty {})
    }
);

// ----- tree -----------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionIdInput {
    pub collection_id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeCreateInput {
    pub collection_id: Id,
    pub parent_id: Option<Id>,
    pub name: String,
    pub data: NodeData,
    #[serde(default)]
    pub placement: ApiPlacement,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeUpdateInput {
    pub id: Id,
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub data: Option<NodeData>,
    /// Reject the update if the node changed since this version.
    #[ts(optional)]
    pub expected_version: Option<Version>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TreeTargetInput {
    pub ids: Vec<Id>,
    pub collection_id: Id,
    pub parent_id: Option<Id>,
    #[serde(default)]
    pub placement: ApiPlacement,
}

command!(
    TreeList,
    "tree.list",
    CollectionIdInput,
    Vec<NodeSummary>,
    |ctx, input| { repo(ctx).list_tree(input.collection_id).await }
);

command!(NodeGet, "node.get", IdInput, Node, |ctx, input| {
    repo(ctx).get_node(input.id).await
});

command!(
    NodeCreate,
    "node.create",
    NodeCreateInput,
    Node,
    |ctx, input| {
        let node = repo(ctx)
            .create_node(
                input.collection_id,
                input.parent_id,
                &input.name,
                input.data,
                input.placement.into(),
            )
            .await?;
        tree_changed(ctx, [node.collection_id], vec![node.meta.id]);
        Ok(node)
    }
);

command!(
    NodeUpdate,
    "node.update",
    NodeUpdateInput,
    Node,
    |ctx, input| {
        let node = repo(ctx)
            .update_node(
                input.id,
                input.name.as_deref(),
                input.data,
                input.expected_version,
            )
            .await?;
        tree_changed(ctx, [node.collection_id], vec![node.meta.id]);
        Ok(node)
    }
);

command!(
    TreeMove,
    "tree.move",
    TreeTargetInput,
    Vec<Node>,
    |ctx, input| {
        let mut cols = collections_of(ctx, &input.ids).await?;
        let moved = repo(ctx)
            .move_nodes(
                &input.ids,
                input.collection_id,
                input.parent_id,
                input.placement.into(),
            )
            .await?;
        cols.push(input.collection_id);
        tree_changed(ctx, cols, input.ids);
        Ok(moved)
    }
);

command!(
    TreeCopy,
    "tree.copy",
    TreeTargetInput,
    Vec<Node>,
    |ctx, input| {
        let copies = repo(ctx)
            .copy_nodes(
                &input.ids,
                input.collection_id,
                input.parent_id,
                input.placement.into(),
            )
            .await?;
        tree_changed(
            ctx,
            [input.collection_id],
            copies.iter().map(|n| n.meta.id).collect(),
        );
        Ok(copies)
    }
);

command!(
    TreeDuplicate,
    "tree.duplicate",
    IdsInput,
    Vec<Node>,
    |ctx, input| {
        let copies = repo(ctx).duplicate_nodes(&input.ids).await?;
        tree_changed(
            ctx,
            copies.iter().map(|n| n.collection_id),
            copies.iter().map(|n| n.meta.id).collect(),
        );
        Ok(copies)
    }
);

command!(TreeDelete, "tree.delete", IdsInput, Empty, |ctx, input| {
    let cols = collections_of(ctx, &input.ids).await?;
    repo(ctx).delete_nodes(&input.ids).await?;
    tree_changed(ctx, cols, input.ids);
    Ok(Empty {})
});

command!(
    TreeRestore,
    "tree.restore",
    IdsInput,
    Empty,
    |ctx, input| {
        let cols = collections_of(ctx, &input.ids).await?;
        repo(ctx).restore_nodes(&input.ids).await?;
        tree_changed(ctx, cols, input.ids);
        Ok(Empty {})
    }
);

command!(
    TrashList,
    "trash.list",
    CollectionIdInput,
    Vec<Node>,
    |ctx, input| { repo(ctx).list_trash(input.collection_id).await }
);

command!(TrashPurge, "trash.purge", IdsInput, Empty, |ctx, input| {
    let cols = collections_of(ctx, &input.ids).await?;
    repo(ctx).purge_nodes(&input.ids).await?;
    tree_changed(ctx, cols, Vec::new());
    Ok(Empty {})
});

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::{AppContext, registry};

    async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
        registry()
            .dispatch(ctx, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[tokio::test]
    async fn ensure_default_bootstraps_once() {
        let ctx = AppContext::for_tests().await;
        let first = call(&ctx, "workspace.ensureDefault", json!({})).await;
        let second = call(&ctx, "workspace.ensureDefault", json!({})).await;
        assert_eq!(first, second);
        let ws = first[0]["id"].clone();
        let cols = call(&ctx, "collection.list", json!({ "workspaceId": ws })).await;
        assert_eq!(cols[0]["name"], "My Collection");
    }

    #[tokio::test]
    async fn tree_round_trip_over_json_with_events() {
        let ctx = AppContext::for_tests().await;
        let mut events = ctx.events.subscribe();
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let col =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();

        let folder = call(
            &ctx,
            "node.create",
            json!({ "collectionId": col, "parentId": null, "name": "API", "data": { "kind": "folder" } }),
        )
        .await;
        let req = call(
            &ctx,
            "node.create",
            json!({
                "collectionId": col, "parentId": folder["id"], "name": "List",
                "data": { "kind": "request", "method": "GET", "url": "https://x" },
                "placement": { "type": "start" }
            }),
        )
        .await;
        assert_eq!(req["data"]["method"], "GET");

        let tree = call(&ctx, "tree.list", json!({ "collectionId": col })).await;
        assert_eq!(tree.as_array().unwrap().len(), 2);

        let updated = call(
            &ctx,
            "node.update",
            json!({ "id": req["id"], "name": "List users", "expectedVersion": 1 }),
        )
        .await;
        assert_eq!(updated["version"], 2);

        call(&ctx, "tree.delete", json!({ "ids": [folder["id"]] })).await;
        assert_eq!(
            call(&ctx, "tree.list", json!({ "collectionId": col })).await,
            json!([])
        );
        assert_eq!(
            call(&ctx, "trash.list", json!({ "collectionId": col })).await[0]["name"],
            "API"
        );

        let mut topics = Vec::new();
        while let Ok(e) = events.try_recv() {
            topics.push(e.topic);
        }
        assert!(
            topics.iter().filter(|t| *t == "tree.changed").count() >= 4,
            "{topics:?}"
        );
    }

    #[tokio::test]
    async fn version_conflict_surfaces_as_error_code() {
        let ctx = AppContext::for_tests().await;
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let col =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();
        let node = call(
            &ctx,
            "node.create",
            json!({ "collectionId": col, "parentId": null, "name": "r", "data": { "kind": "request" } }),
        )
        .await;
        let err = registry()
            .dispatch(
                &ctx,
                "node.update",
                json!({ "id": node["id"], "name": "x", "expectedVersion": 7 }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, "tree.version_conflict");
    }
}
