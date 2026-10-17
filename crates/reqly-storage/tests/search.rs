use reqly_core::model::{FolderData, HttpRequest, NodeData};
use reqly_core::{FixedClock, Id};
use reqly_storage::Storage;
use reqly_storage::search::search_nodes;
use reqly_storage::tree::{Placement, TreeRepo};

async fn setup() -> (Storage, FixedClock, Id, Id) {
    let storage = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let repo = TreeRepo::new(&storage, &clock);
    let ws = repo.create_workspace("W").await.unwrap().meta.id;
    let col = repo.create_collection(ws, "C").await.unwrap().meta.id;
    let req = |method: &str, url: &str, description: &str| {
        NodeData::Request(HttpRequest {
            method: method.into(),
            url: url.into(),
            description: description.into(),
            ..Default::default()
        })
    };
    let users = repo
        .create_node(
            col,
            None,
            "Users",
            NodeData::Folder(FolderData::default()),
            Placement::End,
        )
        .await
        .unwrap()
        .meta
        .id;
    repo.create_node(
        col,
        Some(users),
        "List users",
        req("GET", "https://api.example.com/users", ""),
        Placement::End,
    )
    .await
    .unwrap();
    repo.create_node(
        col,
        Some(users),
        "Create user",
        req(
            "POST",
            "https://api.example.com/users",
            "Registers an account",
        ),
        Placement::End,
    )
    .await
    .unwrap();
    repo.create_node(
        col,
        None,
        "Health",
        req("GET", "https://api.example.com/healthz", ""),
        Placement::End,
    )
    .await
    .unwrap();
    (storage, clock, ws, col)
}

async fn names(storage: &Storage, ws: Id, query: &str) -> Vec<String> {
    let mut v: Vec<String> = search_nodes(storage, ws, query, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn prefix_search_on_name() {
    let (s, _, ws, _) = setup().await;
    assert_eq!(
        names(&s, ws, "us").await,
        ["Create user", "List users", "Users"]
    );
    assert_eq!(names(&s, ws, "heal").await, ["Health"]);
}

#[tokio::test]
async fn method_and_url_filters() {
    let (s, _, ws, _) = setup().await;
    assert_eq!(names(&s, ws, "method:post").await, ["Create user"]);
    assert_eq!(names(&s, ws, "url:healthz").await, ["Health"]);
    assert_eq!(names(&s, ws, "method:get user").await, ["List users"]);
}

#[tokio::test]
async fn description_is_searchable() {
    let (s, _, ws, _) = setup().await;
    assert_eq!(names(&s, ws, "registers").await, ["Create user"]);
}

#[tokio::test]
async fn updates_and_deletes_are_reflected() {
    let (s, clock, ws, col) = setup().await;
    let repo = TreeRepo::new(&s, &clock);
    let health = repo
        .list_tree(col)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.name == "Health")
        .unwrap();
    repo.update_node(health.id, Some("Liveness"), None, None)
        .await
        .unwrap();
    // Still found through its URL (/healthz), now under the new name.
    assert_eq!(names(&s, ws, "health").await, ["Liveness"]);
    assert_eq!(names(&s, ws, "liveness").await, ["Liveness"]);
    repo.delete_nodes(&[health.id]).await.unwrap();
    assert!(names(&s, ws, "liveness").await.is_empty());
}

#[tokio::test]
async fn scoped_to_workspace_and_safe_with_odd_input() {
    let (s, clock, _, _) = setup().await;
    let other = TreeRepo::new(&s, &clock)
        .create_workspace("Other")
        .await
        .unwrap()
        .meta
        .id;
    assert!(names(&s, other, "users").await.is_empty());
    let (s2, _, ws, _) = setup().await;
    for q in [
        "\"",
        "AND",
        "*",
        "users\") OR (",
        "  ",
        "method:",
        "NEAR(a b)",
    ] {
        search_nodes(&s2, ws, q, 10)
            .await
            .unwrap_or_else(|e| panic!("{q}: {e}"));
    }
}
