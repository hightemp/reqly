use reqly_core::model::{FolderData, HttpRequest, KeyValue, NodeData, NodeKind};
use reqly_core::{FixedClock, Id};
use reqly_storage::Storage;
use reqly_storage::tree::{Placement, TreeRepo};

fn folder() -> NodeData {
    NodeData::Folder(FolderData::default())
}

fn request(method: &str, url: &str) -> NodeData {
    NodeData::Request(HttpRequest {
        method: method.into(),
        url: url.into(),
        headers: vec![KeyValue::new("Accept", "*/*")],
        ..Default::default()
    })
}

struct Fx {
    storage: Storage,
    clock: FixedClock,
}

impl Fx {
    async fn new() -> Self {
        Self {
            storage: Storage::in_memory().await.unwrap(),
            clock: FixedClock::new(1_000),
        }
    }

    fn repo(&self) -> TreeRepo<'_> {
        TreeRepo::new(&self.storage, &self.clock)
    }

    async fn collection(&self) -> Id {
        let ws = self.repo().create_workspace("WS").await.unwrap();
        self.repo()
            .create_collection(ws.meta.id, "Col")
            .await
            .unwrap()
            .meta
            .id
    }

    /// Names of children of `parent` in order.
    async fn children(&self, col: Id, parent: Option<Id>) -> Vec<String> {
        self.repo()
            .list_tree(col)
            .await
            .unwrap()
            .into_iter()
            .filter(|n| n.parent_id == parent)
            .map(|n| n.name)
            .collect()
    }

    async fn add(&self, col: Id, parent: Option<Id>, name: &str, data: NodeData) -> Id {
        self.repo()
            .create_node(col, parent, name, data, Placement::End)
            .await
            .unwrap()
            .meta
            .id
    }
}

#[tokio::test]
async fn workspaces_and_collections_crud() {
    let fx = Fx::new().await;
    let repo = fx.repo();
    let a = repo.create_workspace("A").await.unwrap();
    repo.create_workspace("B").await.unwrap();
    assert_eq!(
        repo.list_workspaces()
            .await
            .unwrap()
            .iter()
            .map(|w| w.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    repo.rename_workspace(a.meta.id, "A2").await.unwrap();
    let c = repo.create_collection(a.meta.id, "API").await.unwrap();
    assert_eq!(
        repo.list_collections(a.meta.id).await.unwrap()[0].name,
        "API"
    );
    repo.rename_collection(c.meta.id, "API v2").await.unwrap();
    assert_eq!(repo.get_collection(c.meta.id).await.unwrap().name, "API v2");
    repo.delete_collection(c.meta.id).await.unwrap();
    assert!(repo.list_collections(a.meta.id).await.unwrap().is_empty());
    repo.delete_workspace(a.meta.id).await.unwrap();
    assert_eq!(repo.list_workspaces().await.unwrap().len(), 1);
}

#[tokio::test]
async fn create_get_and_list_with_placement() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let api = fx.add(col, None, "api", folder()).await;
    let b = fx
        .add(col, Some(api), "b", request("POST", "http://b"))
        .await;
    fx.repo()
        .create_node(
            col,
            Some(api),
            "a",
            request("GET", "http://a"),
            Placement::Before(b),
        )
        .await
        .unwrap();
    fx.repo()
        .create_node(
            col,
            Some(api),
            "c",
            request("GET", "http://c"),
            Placement::After(b),
        )
        .await
        .unwrap();
    fx.repo()
        .create_node(col, Some(api), "first", folder(), Placement::Start)
        .await
        .unwrap();
    assert_eq!(fx.children(col, Some(api)).await, ["first", "a", "b", "c"]);

    let node = fx.repo().get_node(b).await.unwrap();
    assert_eq!(node.parent_id, Some(api));
    let NodeData::Request(r) = node.data else {
        panic!()
    };
    assert_eq!(r.method, "POST");

    let summary = fx.repo().list_tree(col).await.unwrap();
    let b_summary = summary.iter().find(|n| n.id == b).unwrap();
    assert_eq!(b_summary.kind, NodeKind::Request);
    assert_eq!(b_summary.method.as_deref(), Some("POST"));
    assert_eq!(b_summary.url.as_deref(), Some("http://b"));
}

#[tokio::test]
async fn parent_must_be_a_folder_in_the_same_collection() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let req = fx.add(col, None, "r", request("GET", "x")).await;
    let err = fx
        .repo()
        .create_node(col, Some(req), "child", folder(), Placement::End)
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.parent_not_folder");

    let other = fx.collection().await;
    let f = fx.add(other, None, "f", folder()).await;
    let err = fx
        .repo()
        .create_node(col, Some(f), "x", folder(), Placement::End)
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.parent_other_collection");
}

#[tokio::test]
async fn update_bumps_version_and_checks_expected_version() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let id = fx.add(col, None, "r", request("GET", "x")).await;
    fx.clock.advance(10);
    let updated = fx
        .repo()
        .update_node(id, Some("renamed"), Some(request("PUT", "y")), None)
        .await
        .unwrap();
    assert_eq!(updated.meta.version.0, 2);
    assert_eq!(updated.meta.updated_at.0, 1_010);
    assert_eq!(updated.name, "renamed");

    let err = fx
        .repo()
        .update_node(id, Some("z"), None, Some(reqly_core::Version(1)))
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.version_conflict");

    let err = fx
        .repo()
        .update_node(id, None, Some(folder()), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.kind_change");
}

#[tokio::test]
async fn move_reorders_and_reparents() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let a = fx.add(col, None, "a", folder()).await;
    let b = fx.add(col, None, "b", folder()).await;
    let r1 = fx.add(col, Some(a), "r1", request("GET", "1")).await;
    let r2 = fx.add(col, Some(a), "r2", request("GET", "2")).await;
    let r3 = fx.add(col, Some(a), "r3", request("GET", "3")).await;

    // Move a group into b keeping relative order regardless of selection order.
    fx.repo()
        .move_nodes(&[r3, r1], col, Some(b), Placement::End)
        .await
        .unwrap();
    assert_eq!(fx.children(col, Some(a)).await, ["r2"]);
    assert_eq!(fx.children(col, Some(b)).await, ["r1", "r3"]);

    // Reorder within a parent.
    fx.repo()
        .move_nodes(&[r3], col, Some(b), Placement::Before(r1))
        .await
        .unwrap();
    assert_eq!(fx.children(col, Some(b)).await, ["r3", "r1"]);

    // Move a folder to the root before another.
    fx.repo()
        .move_nodes(&[b], col, None, Placement::Before(a))
        .await
        .unwrap();
    assert_eq!(fx.children(col, None).await, ["b", "a"]);
    let _ = r2;
}

#[tokio::test]
async fn move_rejects_cycles() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let a = fx.add(col, None, "a", folder()).await;
    let b = fx.add(col, Some(a), "b", folder()).await;
    let err = fx
        .repo()
        .move_nodes(&[a], col, Some(b), Placement::End)
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.cycle");
    let err = fx
        .repo()
        .move_nodes(&[a], col, Some(a), Placement::End)
        .await
        .unwrap_err();
    assert_eq!(err.code, "tree.cycle");
}

#[tokio::test]
async fn move_across_collections_carries_descendants() {
    let fx = Fx::new().await;
    let src = fx.collection().await;
    let dst = fx.collection().await;
    let f = fx.add(src, None, "f", folder()).await;
    let inner = fx.add(src, Some(f), "inner", folder()).await;
    fx.add(src, Some(inner), "deep", request("GET", "d")).await;

    fx.repo()
        .move_nodes(&[f], dst, None, Placement::End)
        .await
        .unwrap();
    assert!(fx.repo().list_tree(src).await.unwrap().is_empty());
    let moved = fx.repo().list_tree(dst).await.unwrap();
    assert_eq!(moved.len(), 3);
    assert!(moved.iter().all(|n| n.collection_id == dst));
}

#[tokio::test]
async fn duplicate_deep_copies_next_to_original() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let f = fx.add(col, None, "f", folder()).await;
    fx.add(col, Some(f), "r", request("GET", "x")).await;
    fx.add(col, None, "after", folder()).await;

    let copies = fx.repo().duplicate_nodes(&[f]).await.unwrap();
    assert_eq!(copies.len(), 1);
    assert_eq!(fx.children(col, None).await, ["f", "f (copy)", "after"]);
    let copy_id = copies[0].meta.id;
    assert_ne!(copy_id, f);
    assert_eq!(fx.children(col, Some(copy_id)).await, ["r"]);

    // Row ids are regenerated so editors never share keys between copies.
    let orig_child = fx
        .repo()
        .list_tree(col)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.parent_id == Some(f))
        .unwrap();
    let copy_child = fx
        .repo()
        .list_tree(col)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.parent_id == Some(copy_id))
        .unwrap();
    let (NodeData::Request(a), NodeData::Request(b)) = (
        fx.repo().get_node(orig_child.id).await.unwrap().data,
        fx.repo().get_node(copy_child.id).await.unwrap().data,
    ) else {
        panic!()
    };
    assert_ne!(a.headers[0].id, b.headers[0].id);
    assert_eq!(a.headers[0].name, b.headers[0].name);
}

#[tokio::test]
async fn copy_pastes_into_another_collection() {
    let fx = Fx::new().await;
    let src = fx.collection().await;
    let dst = fx.collection().await;
    let r = fx.add(src, None, "r", request("GET", "x")).await;
    let target = fx.add(dst, None, "target", folder()).await;

    let pasted = fx
        .repo()
        .copy_nodes(&[r], dst, Some(target), Placement::End)
        .await
        .unwrap();
    assert_eq!(pasted[0].name, "r");
    assert_eq!(fx.children(dst, Some(target)).await, ["r"]);
    assert_eq!(fx.children(src, None).await, ["r"], "source untouched");
}

#[tokio::test]
async fn delete_is_soft_recursive_and_restorable() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let f = fx.add(col, None, "f", folder()).await;
    let r = fx.add(col, Some(f), "r", request("GET", "x")).await;
    let lone = fx.add(col, None, "lone", request("GET", "y")).await;

    fx.repo().delete_nodes(&[f, lone]).await.unwrap();
    assert!(fx.repo().list_tree(col).await.unwrap().is_empty());
    let trash = fx.repo().list_trash(col).await.unwrap();
    let mut names: Vec<_> = trash.iter().map(|n| n.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        ["f", "lone"],
        "only top-level deleted nodes are listed"
    );

    fx.repo().restore_nodes(&[f]).await.unwrap();
    assert_eq!(fx.children(col, None).await, ["f"]);
    assert_eq!(fx.children(col, Some(f)).await, ["r"]);
    assert_eq!(fx.repo().get_node(r).await.unwrap().meta.deleted_at, None);
}

#[tokio::test]
async fn restore_into_root_when_parent_is_gone() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let f = fx.add(col, None, "f", folder()).await;
    let r = fx.add(col, Some(f), "r", request("GET", "x")).await;
    fx.repo().delete_nodes(&[r]).await.unwrap();
    fx.clock.advance(1);
    fx.repo().delete_nodes(&[f]).await.unwrap();

    fx.repo().restore_nodes(&[r]).await.unwrap();
    assert_eq!(fx.children(col, None).await, ["r"]);
}

#[tokio::test]
async fn purge_removes_permanently() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let f = fx.add(col, None, "f", folder()).await;
    let r = fx.add(col, Some(f), "r", request("GET", "x")).await;
    fx.repo().delete_nodes(&[f]).await.unwrap();
    fx.repo().purge_nodes(&[f]).await.unwrap();
    assert!(fx.repo().list_trash(col).await.unwrap().is_empty());
    assert_eq!(fx.repo().get_node(r).await.unwrap_err().code, "not_found");
}

#[tokio::test]
async fn purge_deleted_before_cleans_old_trash() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let old = fx.add(col, None, "old", folder()).await;
    let fresh = fx.add(col, None, "fresh", folder()).await;
    fx.repo().delete_nodes(&[old]).await.unwrap();
    fx.clock.advance(100);
    fx.repo().delete_nodes(&[fresh]).await.unwrap();
    let purged = fx
        .repo()
        .purge_deleted_before(reqly_core::Timestamp(1_050))
        .await
        .unwrap();
    assert_eq!(purged, 1);
    assert_eq!(fx.repo().list_trash(col).await.unwrap()[0].name, "fresh");
}

#[tokio::test]
async fn deleting_a_collection_hides_its_nodes_from_get() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let r = fx.add(col, None, "r", request("GET", "x")).await;
    fx.repo().delete_collection(col).await.unwrap();
    assert_eq!(
        fx.repo().get_collection(col).await.unwrap_err().code,
        "not_found"
    );
    // Node rows still exist for restore; reading by id still works.
    assert!(fx.repo().get_node(r).await.is_ok());
}

#[tokio::test]
async fn ancestors_are_outermost_first() {
    let fx = Fx::new().await;
    let col = fx.collection().await;
    let a = fx.add(col, None, "a", folder()).await;
    let b = fx.add(col, Some(a), "b", folder()).await;
    let r = fx.add(col, Some(b), "r", request("GET", "x")).await;
    let names: Vec<_> = fx
        .repo()
        .ancestors(r)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    assert_eq!(names, ["a", "b"]);
    assert!(fx.repo().ancestors(a).await.unwrap().is_empty());
}
