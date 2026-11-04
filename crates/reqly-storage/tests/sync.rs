//! Sync records over SQLite (ADR-013).

use reqly_core::model::{EnvironmentKind, FolderData, HttpRequest, NodeData};
use reqly_core::{FixedClock, Id};
use reqly_storage::Storage;
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::sync::{InProcess, SyncClient, SyncServer};
use reqly_storage::tree::{Placement, TreeRepo};
use reqly_sync::{Kind, LocalStore, Resolution, ServerStore, resolve, round};
use serde_json::{Map, json};

fn request(url: &str) -> NodeData {
    NodeData::Request(HttpRequest {
        method: "GET".into(),
        url: url.into(),
        ..Default::default()
    })
}

struct Fx {
    ws: Id,
    col: Id,
    folder: Id,
    req: Id,
}

async fn fill(storage: &Storage, clock: &FixedClock) -> Fx {
    let tree = TreeRepo::new(storage, clock);
    let ws = tree.create_workspace("W").await.unwrap().meta.id;
    let col = tree.create_collection(ws, "C").await.unwrap().meta.id;
    let folder = tree
        .create_node(
            col,
            None,
            "F",
            NodeData::Folder(FolderData::default()),
            Placement::End,
        )
        .await
        .unwrap()
        .meta
        .id;
    let req = tree
        .create_node(
            col,
            Some(folder),
            "R",
            request("https://api.test/a"),
            Placement::End,
        )
        .await
        .unwrap()
        .meta
        .id;
    Fx {
        ws,
        col,
        folder,
        req,
    }
}

#[tokio::test]
async fn every_write_takes_the_next_change_sequence() {
    let storage = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1_000);
    let fx = fill(&storage, &clock).await;
    let server = SyncServer::new(&storage, &clock, true);

    let all = server.changed(fx.ws, 0).await.unwrap();
    let kinds: Vec<Kind> = all.iter().map(|(_, r)| r.kind).collect();
    assert!(kinds.contains(&Kind::Workspace) && kinds.contains(&Kind::Collection));
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Node).count(), 2);
    assert!(
        all.windows(2).all(|w| w[0].0 < w[1].0),
        "ordered by sequence"
    );

    let last = all.last().unwrap().0;
    TreeRepo::new(&storage, &clock)
        .update_node(fx.req, Some("Renamed"), None, None)
        .await
        .unwrap();
    let newer = server.changed(fx.ws, last).await.unwrap();
    assert_eq!(newer.len(), 1);
    assert_eq!(newer[0].1.id, fx.req);
    assert_eq!(newer[0].1.fields["name"], "Renamed");
    assert_eq!(newer[0].1.fields["parentId"], json!(fx.folder.to_string()));
    assert_eq!(newer[0].1.fields["collectionId"], json!(fx.col.to_string()));
    assert_eq!(newer[0].1.fields["data"]["url"], "https://api.test/a");
    assert!(server.changed(Id::new(), 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn records_copy_to_another_database() {
    let clock = FixedClock::new(1_000);
    let a = Storage::in_memory().await.unwrap();
    let fx = fill(&a, &clock).await;
    let b = Storage::in_memory().await.unwrap();
    let (from, to) = (
        SyncServer::new(&a, &clock, true),
        SyncServer::new(&b, &clock, true),
    );
    let mut records: Vec<_> = from
        .changed(fx.ws, 0)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    // Children before parents: the store needs dependency order.
    records.reverse();
    reqly_sync::record::dependency_order(&mut records);
    for r in &records {
        to.put(r).await.unwrap();
    }
    for r in &records {
        assert_eq!(to.get(r.kind, r.id).await.unwrap().as_ref(), Some(r));
    }
    let node = TreeRepo::new(&b, &clock).get_node(fx.req).await.unwrap();
    assert_eq!(node.parent_id, Some(fx.folder));
    assert_eq!(node.name, "R");
}

#[tokio::test]
async fn secrets_stay_out_unless_asked() {
    let storage = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1_000);
    let fx = fill(&storage, &clock).await;
    let envs = EnvironmentRepo::new(&storage, &clock);
    let env = envs
        .create(fx.ws, Some(fx.col), EnvironmentKind::Sub, "Prod")
        .await
        .unwrap();
    let mut data = Map::new();
    data.insert("host".into(), json!("api.test"));
    data.insert("token".into(), json!("s3cret"));
    envs.update(
        env.meta.id,
        None,
        None,
        Some(data),
        Some(vec!["token".into()]),
    )
    .await
    .unwrap();

    let plain = SyncServer::new(&storage, &clock, false);
    let mut record = plain
        .get(Kind::Environment, env.meta.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.fields["data"], json!({ "host": "api.test" }));
    // Writing it back without the secret keeps the stored value.
    record.fields["data"]["host"] = json!("api2.test");
    plain.put(&record).await.unwrap();
    let stored = envs.get(env.meta.id).await.unwrap();
    assert_eq!(stored.data["host"], "api2.test");
    assert_eq!(stored.data["token"], "s3cret");

    let with = SyncServer::new(&storage, &clock, true);
    let mut record = with
        .get(Kind::Environment, env.meta.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.fields["data"]["token"], "s3cret");
    record.fields["data"]["token"] = json!("rotated");
    with.put(&record).await.unwrap();
    assert_eq!(
        envs.get(env.meta.id).await.unwrap().data["token"],
        "rotated"
    );
    let raw: String = sqlx::query_scalar("SELECT data FROM environments WHERE id = ?")
        .bind(env.meta.id.to_string())
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert!(!raw.contains("rotated"), "encrypted at rest");
}

#[tokio::test]
async fn records_cannot_change_workspace_or_dangle() {
    let storage = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1_000);
    let fx = fill(&storage, &clock).await;
    let other = TreeRepo::new(&storage, &clock)
        .create_workspace("Other")
        .await
        .unwrap()
        .meta
        .id;
    let server = SyncServer::new(&storage, &clock, true);

    let mut node = server.get(Kind::Node, fx.req).await.unwrap().unwrap();
    node.workspace_id = other;
    assert_eq!(server.put(&node).await.unwrap_err().code, "sync.invalid");

    // A parent that is gone puts the node at the root.
    let mut node = server.get(Kind::Node, fx.req).await.unwrap().unwrap();
    node.id = Id::new();
    node.fields["parentId"] = json!(Id::new().to_string());
    server.put(&node).await.unwrap();
    let stored = server.get(Kind::Node, node.id).await.unwrap().unwrap();
    assert_eq!(stored.fields["parentId"], json!(null));

    let mut orphan = node.clone();
    orphan.id = Id::new();
    orphan.fields["collectionId"] = json!(Id::new().to_string());
    assert_eq!(
        server.put(&orphan).await.unwrap_err().code,
        "sync.missing_parent"
    );
}

#[tokio::test]
async fn two_desktops_sync_through_a_server() {
    let clock = FixedClock::new(1_000);
    let server_db = Storage::in_memory().await.unwrap();
    let server = InProcess {
        storage: &server_db,
        clock: &clock,
    };
    let (a_db, b_db) = (
        Storage::in_memory().await.unwrap(),
        Storage::in_memory().await.unwrap(),
    );
    let (a, b) = (
        SyncClient::new(&a_db, &clock, false),
        SyncClient::new(&b_db, &clock, false),
    );

    // A uploads a local workspace.
    let fx = fill(&a_db, &clock).await;
    a.follow(fx.ws).await.unwrap();
    let report = round(&a, &server, false).await.unwrap();
    assert_eq!(report.pushed, 4, "workspace, collection, folder, request");
    assert!(a.changed().await.unwrap().is_empty());

    // B downloads it.
    b.follow(fx.ws).await.unwrap();
    round(&b, &server, false).await.unwrap();
    let tree_b = TreeRepo::new(&b_db, &clock);
    assert_eq!(tree_b.get_node(fx.req).await.unwrap().name, "R");
    assert!(
        b.changed().await.unwrap().is_empty(),
        "nothing to send back"
    );

    // Offline edits on both sides merge.
    let tree_a = TreeRepo::new(&a_db, &clock);
    tree_a
        .update_node(fx.req, Some("Get user"), None, None)
        .await
        .unwrap();
    tree_b
        .update_node(
            fx.req,
            None,
            Some(request("https://api.test/a?page=2")),
            None,
        )
        .await
        .unwrap();
    round(&a, &server, false).await.unwrap();
    round(&b, &server, false).await.unwrap();
    round(&a, &server, false).await.unwrap();
    for tree in [&tree_a, &tree_b] {
        let node = tree.get_node(fx.req).await.unwrap();
        assert_eq!(node.name, "Get user");
        assert_eq!(
            node.data.method_and_url().1,
            Some("https://api.test/a?page=2")
        );
    }

    // Deleted on A, changed on B: B decides.
    tree_a.delete_nodes(&[fx.req]).await.unwrap();
    tree_b
        .update_node(fx.req, Some("Get one user"), None, None)
        .await
        .unwrap();
    round(&a, &server, false).await.unwrap();
    let report = round(&b, &server, false).await.unwrap();
    assert_eq!(report.conflicts, 1);
    let conflict = &b.conflicts().await.unwrap()[0];
    assert!(conflict.remote.deleted);
    assert_eq!(conflict.local.fields["name"], "Get one user");
    resolve(
        &b,
        &server,
        Kind::Node,
        fx.req,
        Resolution::KeepLocal,
        false,
    )
    .await
    .unwrap();
    round(&a, &server, false).await.unwrap();
    let node = tree_a.get_node(fx.req).await.unwrap();
    assert!(node.meta.deleted_at.is_none());
    assert_eq!(node.name, "Get one user");
    assert!(a.changed().await.unwrap().is_empty() && b.changed().await.unwrap().is_empty());
}
