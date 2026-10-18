//! Concurrent writers on a database file in WAL mode, as on a busy server.

use std::sync::Arc;

use reqly_core::SystemClock;
use reqly_core::model::{FolderData, NodeData};
use reqly_storage::tree::{Placement, TreeRepo};
use reqly_storage::{OpenOptions, Storage};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_wait_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        Storage::open(&dir.path().join("reqly.db"), OpenOptions::default())
            .await
            .unwrap(),
    );
    let tree = TreeRepo::new(&storage, &SystemClock);
    let ws = tree.create_workspace("W").await.unwrap().meta.id;
    let col = tree.create_collection(ws, "C").await.unwrap().meta.id;
    let node = tree
        .create_node(
            col,
            None,
            "R",
            NodeData::Folder(FolderData::default()),
            Placement::End,
        )
        .await
        .unwrap()
        .meta
        .id;

    let mut tasks = Vec::new();
    for t in 0..16 {
        let storage = storage.clone();
        tasks.push(tokio::spawn(async move {
            let tree = TreeRepo::new(&storage, &SystemClock);
            for i in 0..20 {
                tree.update_node(node, Some(&format!("{t}-{i}")), None, None)
                    .await?;
                tree.create_node(
                    col,
                    None,
                    "N",
                    NodeData::Folder(FolderData::default()),
                    Placement::End,
                )
                .await?;
            }
            Ok::<_, reqly_core::CoreError>(())
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(tree.list_tree(col).await.unwrap().len(), 1 + 16 * 20);
}
