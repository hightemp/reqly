//! Desktop ↔ server sync (ADR-013).
//!
//! The server answers `sync.pull` and `sync.push` within the caller's
//! workspace roles. The desktop app keeps a connection to a server and runs
//! sync rounds against it ([`client`]).

pub mod client;

use std::collections::{BTreeSet, HashSet};

use reqly_core::{CoreResult, Id};
use reqly_storage::sync::SyncServer;
use reqly_storage::tree::TreeRepo;
use reqly_storage::users::{MemberRepo, Role};
use reqly_sync::{Kind, PullInput, PullOutput, PushInput, PushOutput, PushResult, Record};

use super::AppContext;
use super::template::EnvironmentsChanged;
use crate::access::{self, Caller};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<SyncPull>().register::<SyncPush>();
    client::register(r);
}

/// Workspaces of `ids` where the caller has at least `need`.
async fn allowed(
    ctx: &AppContext,
    ids: impl IntoIterator<Item = Id>,
    need: Role,
) -> CoreResult<HashSet<Id>> {
    let caller = access::current();
    let mut out = HashSet::new();
    for id in ids {
        if access::role_in(&ctx.storage, caller, id)
            .await?
            .is_some_and(|r| r >= need)
        {
            out.insert(id);
        }
    }
    Ok(out)
}

/// Tells the UIs (and live documents) what a sync wrote.
pub(crate) async fn publish_changes(ctx: &AppContext, records: &[Record]) {
    if records.is_empty() {
        return;
    }
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let mut collections: BTreeSet<Id> = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut envs: BTreeSet<(Id, Option<Id>)> = BTreeSet::new();
    let mut workspaces_with_collections: BTreeSet<Id> = BTreeSet::new();
    let mut workspaces = false;
    for r in records {
        match r.kind {
            Kind::Workspace => workspaces = true,
            Kind::Collection => {
                workspaces_with_collections.insert(r.workspace_id);
                collections.insert(r.id);
            }
            Kind::Environment => {
                let collection = r
                    .fields
                    .get("collectionId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok());
                envs.insert((r.workspace_id, collection));
            }
            Kind::Node => {
                if let Ok(node) = tree.get_node(r.id).await {
                    collections.insert(node.collection_id);
                    ctx.collab.sync_from_node(ctx, &node);
                }
                nodes.push(r.id);
            }
        }
    }
    if workspaces {
        super::tree::workspaces_changed(ctx);
    }
    for ws in workspaces_with_collections {
        super::tree::collections_changed(ctx, ws);
    }
    if !collections.is_empty() {
        super::tree::tree_changed(ctx, collections, nodes);
    }
    for (workspace_id, collection_id) in envs {
        ctx.events.publish(Event::new(
            "environments.changed",
            EnvironmentsChanged {
                workspace_id,
                collection_id,
            },
        ));
    }
}

/// The server's changes in the caller's workspaces since the given
/// cursors; workspaces the caller cannot read come back as `unavailable`.
pub struct SyncPull;

impl Command<AppContext> for SyncPull {
    const NAME: &'static str = "sync.pull";
    type Input = PullInput;
    type Output = PullOutput;

    async fn run(ctx: &AppContext, input: PullInput) -> CoreResult<PullOutput> {
        let ids: Vec<Id> = input.cursors.iter().map(|c| c.workspace_id).collect();
        let readable = allowed(ctx, ids, Role::Viewer).await?;
        let store = SyncServer::new(&ctx.storage, ctx.clock.as_ref(), input.secrets);
        reqly_sync::server::pull(&store, &input, &|ws| readable.contains(&ws)).await
    }
}

/// Stores local changes of a client, merged with the server's records
/// (ADR-013). Needs the editor role; a new workspace belongs to the caller.
pub struct SyncPush;

impl Command<AppContext> for SyncPush {
    const NAME: &'static str = "sync.push";
    type Input = PushInput;
    type Output = PushOutput;

    async fn run(ctx: &AppContext, input: PushInput) -> CoreResult<PushOutput> {
        let existing = reqly_storage::sync::workspace_ids(&ctx.storage).await?;
        let mentioned: HashSet<Id> = input
            .changes
            .iter()
            .map(|c| c.record.workspace_id)
            .collect();
        let known: Vec<Id> = mentioned
            .iter()
            .copied()
            .filter(|w| existing.contains(w))
            .collect();
        let writable = allowed(ctx, known, Role::Editor).await?;
        // A workspace the server does not have yet comes with the push.
        let created: HashSet<Id> = input
            .changes
            .iter()
            .filter(|c| c.record.kind == Kind::Workspace && !existing.contains(&c.record.id))
            .map(|c| c.record.id)
            .collect();
        let store = SyncServer::new(&ctx.storage, ctx.clock.as_ref(), input.secrets);
        let out = reqly_sync::server::push(&store, input, &|r: &Record| {
            writable.contains(&r.workspace_id) || created.contains(&r.workspace_id)
        })
        .await?;

        let mut applied = Vec::new();
        let mut owned: HashSet<Id> = HashSet::new();
        for result in &out.results {
            if let PushResult::Applied { record } = result {
                if record.kind == Kind::Workspace && created.contains(&record.id) {
                    owned.insert(record.id);
                }
                applied.push(record.clone());
            }
        }
        if let Caller::User { id: user, .. } = access::current() {
            let members = MemberRepo::new(&ctx.storage, ctx.clock.as_ref());
            for ws in owned {
                members.set(ws, user, Role::Owner).await?;
            }
        }
        publish_changes(ctx, &applied).await;
        Ok(out)
    }
}
