//! Two clients and a server with random offline edits converge (T11.1).

use proptest::prelude::*;
use reqly_core::Id;
use reqly_sync::memory::{MemoryClient, MemoryServer};
use reqly_sync::{Kind, LocalStore, Record, Resolution, ServerStore, resolve, round};
use serde_json::{Map, json};

const ENTITIES: usize = 4;

#[derive(Debug, Clone)]
enum Op {
    /// Creates the node, or renames it.
    Rename {
        client: usize,
        entity: usize,
        name: u8,
    },
    /// Inserts a character into the URL (a text field).
    Type {
        client: usize,
        entity: usize,
        at: usize,
        ch: char,
    },
    /// Moves the node to another parent (or the root).
    Move {
        client: usize,
        entity: usize,
        parent: Option<usize>,
    },
    Delete {
        client: usize,
        entity: usize,
    },
    Restore {
        client: usize,
        entity: usize,
    },
    Purge {
        client: usize,
        entity: usize,
    },
    Sync {
        client: usize,
        keep_local: bool,
    },
}

fn op() -> impl Strategy<Value = Op> {
    let c = 0..2usize;
    let e = 0..ENTITIES;
    prop_oneof![
        3 => (c.clone(), e.clone(), 0..4u8).prop_map(|(client, entity, name)| Op::Rename { client, entity, name }),
        4 => (c.clone(), e.clone(), 0..12usize, prop::sample::select(vec!['a', 'b', '/', '?', 'é']))
            .prop_map(|(client, entity, at, ch)| Op::Type { client, entity, at, ch }),
        1 => (c.clone(), e.clone(), prop::option::of(0..ENTITIES)).prop_map(|(client, entity, parent)| Op::Move { client, entity, parent }),
        1 => (c.clone(), e.clone()).prop_map(|(client, entity)| Op::Delete { client, entity }),
        1 => (c.clone(), e.clone()).prop_map(|(client, entity)| Op::Restore { client, entity }),
        1 => (c.clone(), e.clone()).prop_map(|(client, entity)| Op::Purge { client, entity }),
        3 => (c, any::<bool>()).prop_map(|(client, keep_local)| Op::Sync { client, keep_local }),
    ]
}

struct World {
    workspace: Id,
    ids: Vec<Id>,
    server: MemoryServer,
    clients: Vec<MemoryClient>,
}

impl World {
    async fn new() -> Self {
        let workspace = Id::new();
        let server = MemoryServer::default();
        server
            .put(&Record {
                kind: Kind::Workspace,
                id: workspace,
                workspace_id: workspace,
                deleted: false,
                fields: json!({ "name": "W" }).as_object().unwrap().clone(),
            })
            .await
            .unwrap();
        let clients = vec![MemoryClient::default(), MemoryClient::default()];
        for c in &clients {
            c.follow(workspace);
        }
        Self {
            workspace,
            ids: (0..ENTITIES).map(|_| Id::new()).collect(),
            server,
            clients,
        }
    }

    fn edit(&self, client: usize, entity: usize, f: impl FnOnce(&mut Record)) {
        let c = &self.clients[client];
        let id = self.ids[entity];
        let mut record = c.get(Kind::Node, id).unwrap_or_else(|| Record {
            kind: Kind::Node,
            id,
            workspace_id: self.workspace,
            deleted: false,
            fields: json!({ "name": "", "parentId": null, "data": { "url": "" } })
                .as_object()
                .unwrap()
                .clone(),
        });
        f(&mut record);
        c.put(record);
    }

    async fn sync(&self, client: usize, keep_local: bool) {
        let c = &self.clients[client];
        round(c, &self.server, false).await.unwrap();
        for conflict in c.conflicts().await.unwrap() {
            let how = if keep_local {
                Resolution::KeepLocal
            } else {
                Resolution::TakeRemote
            };
            resolve(
                c,
                &self.server,
                conflict.local.kind,
                conflict.local.id,
                how,
                false,
            )
            .await
            .unwrap();
        }
    }

    async fn apply(&self, op: Op) {
        match op {
            Op::Rename {
                client,
                entity,
                name,
            } => self.edit(client, entity, |r| {
                r.fields.insert("name".into(), json!(format!("n{name}")));
            }),
            Op::Type {
                client,
                entity,
                at,
                ch,
            } => self.edit(client, entity, |r| {
                let data = r.fields.get_mut("data").unwrap().as_object_mut().unwrap();
                let mut url: Vec<char> = data["url"].as_str().unwrap().chars().collect();
                url.insert(at.min(url.len()), ch);
                data.insert("url".into(), json!(url.into_iter().collect::<String>()));
            }),
            Op::Move {
                client,
                entity,
                parent,
            } => {
                let parent = parent
                    .filter(|&p| p != entity)
                    .map(|p| self.ids[p].to_string());
                self.edit(client, entity, |r| {
                    r.fields.insert("parentId".into(), json!(parent));
                })
            }
            Op::Delete { client, entity } => self.edit(client, entity, |r| r.deleted = true),
            Op::Restore { client, entity } => self.edit(client, entity, |r| r.deleted = false),
            Op::Purge { client, entity } => {
                let c = &self.clients[client];
                if c.get(Kind::Node, self.ids[entity])
                    .is_some_and(|r| r.deleted)
                {
                    c.purge(Kind::Node, self.ids[entity]);
                }
            }
            Op::Sync { client, keep_local } => self.sync(client, keep_local).await,
        }
    }

    /// Syncs everybody until nothing is left to send.
    async fn settle(&self) {
        for _ in 0..6 {
            for i in 0..self.clients.len() {
                self.sync(i, false).await;
            }
            let mut quiet = true;
            for c in &self.clients {
                quiet &= c.changed().await.unwrap().is_empty()
                    && c.conflicts().await.unwrap().is_empty();
            }
            if quiet {
                break;
            }
        }
        for i in 0..self.clients.len() {
            self.sync(i, false).await;
        }
    }
}

/// What a client shows: purged rows are gone, deleted ones are in the trash.
fn visible(records: Vec<Record>) -> Vec<(Id, bool, Map<String, serde_json::Value>)> {
    records
        .into_iter()
        .map(|r| (r.id, r.deleted, r.fields))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn offline_edits_converge(ops in prop::collection::vec(op(), 1..40)) {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let world = World::new().await;
            for op in ops {
                world.apply(op).await;
            }
            world.settle().await;
            let server = world.server.records(world.workspace);
            for (i, c) in world.clients.iter().enumerate() {
                assert!(c.changed().await.unwrap().is_empty(), "client {i} has unsent changes");
                assert!(c.conflicts().await.unwrap().is_empty(), "client {i} has conflicts");
                // A purged row stays purged locally; everything else matches.
                let local = visible(c.records(world.workspace));
                let expected: Vec<_> = visible(server.clone())
                    .into_iter()
                    .filter(|(id, deleted, _)| !*deleted || local.iter().any(|(l, _, _)| l == id))
                    .collect();
                assert_eq!(local, expected, "client {i} differs from the server");
            }
        });
    }
}

#[tokio::test]
async fn text_edits_on_both_clients_merge() {
    let world = World::new().await;
    world.edit(0, 0, |r| {
        r.fields["data"]["url"] = json!("https://api.test/users");
    });
    world.sync(0, false).await;
    world.sync(1, false).await;
    world.edit(0, 0, |r| {
        r.fields["data"]["url"] = json!("https://api.test/v2/users")
    });
    world.edit(1, 0, |r| {
        r.fields["data"]["url"] = json!("https://api.test/users?page=2")
    });
    world.settle().await;
    for c in &world.clients {
        let r = c.get(Kind::Node, world.ids[0]).unwrap();
        assert_eq!(r.fields["data"]["url"], "https://api.test/v2/users?page=2");
    }
}

#[tokio::test]
async fn delete_against_change_waits_for_the_user() {
    let world = World::new().await;
    world.edit(0, 0, |r| r.fields["name"] = json!("Users"));
    world.sync(0, false).await;
    world.sync(1, false).await;
    world.edit(0, 0, |r| r.deleted = true);
    world.edit(1, 0, |r| r.fields["name"] = json!("All users"));
    round(&world.clients[0], &world.server, false)
        .await
        .unwrap();
    let report = round(&world.clients[1], &world.server, false)
        .await
        .unwrap();
    assert_eq!(report.conflicts, 1);
    // Nothing is lost until the user decides; the server keeps the deletion.
    assert_eq!(
        world.clients[1]
            .get(Kind::Node, world.ids[0])
            .unwrap()
            .fields["name"],
        "All users"
    );
    assert!(
        world
            .server
            .get(Kind::Node, world.ids[0])
            .await
            .unwrap()
            .unwrap()
            .deleted
    );
    resolve(
        &world.clients[1],
        &world.server,
        Kind::Node,
        world.ids[0],
        Resolution::KeepLocal,
        false,
    )
    .await
    .unwrap();
    world.settle().await;
    for c in &world.clients {
        let r = c.get(Kind::Node, world.ids[0]).unwrap();
        assert!(!r.deleted);
        assert_eq!(r.fields["name"], "All users");
    }
}
