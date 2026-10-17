use reqly_core::{FixedClock, Id, Timestamp};
use reqly_storage::Storage;
use reqly_storage::history::{HistoryLimits, HistoryRepo, NewResponse};

fn record(request_id: Id, size: u64, hash: &str) -> NewResponse {
    NewResponse {
        request_id,
        workspace_id: Id::new(),
        status: Some(200),
        status_text: "OK".into(),
        http_version: "HTTP/1.1".into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body_hash: Some(hash.into()),
        body_size: size,
        headers_size: 10,
        content_type: Some("application/json".into()),
        timings: serde_json::json!({ "totalMs": 12.5 }),
        timeline: serde_json::json!([{ "kind": "info", "text": "hi" }]),
        request_snapshot: serde_json::json!({ "method": "GET", "url": "http://x" }),
        redirects: 0,
        final_url: "http://x".into(),
        remote_addr: None,
        error: None,
    }
}

async fn setup() -> (Storage, FixedClock) {
    (Storage::in_memory().await.unwrap(), FixedClock::new(1_000))
}

#[tokio::test]
async fn insert_list_get_newest_first() {
    let (s, clock) = setup().await;
    let repo = HistoryRepo::new(&s, &clock);
    let req = Id::new();
    let first = repo.insert(record(req, 5, &"a".repeat(64))).await.unwrap();
    clock.advance(10);
    let second = repo.insert(record(req, 6, &"b".repeat(64))).await.unwrap();
    let list = repo.list_for_request(req, 10).await.unwrap();
    assert_eq!(
        list.iter().map(|r| r.id).collect::<Vec<_>>(),
        [second.id, first.id]
    );
    let got = repo.get(first.id).await.unwrap();
    assert_eq!(got.status, Some(200));
    assert_eq!(got.timings["totalMs"], 12.5);
    assert_eq!(got.headers[0].0, "content-type");
}

#[tokio::test]
async fn enforce_limits_per_request_total_size_and_age() {
    let (s, clock) = setup().await;
    let repo = HistoryRepo::new(&s, &clock);
    let a = Id::new();
    let b = Id::new();
    for i in 0..5 {
        repo.insert(record(a, 100, &format!("{i:064}")))
            .await
            .unwrap();
        clock.advance(1);
    }
    repo.insert(record(b, 100, &"f".repeat(64))).await.unwrap();

    let removed = repo
        .enforce_limits(&HistoryLimits {
            per_request: 3,
            total_bytes: u64::MAX,
            max_age_ms: None,
        })
        .await
        .unwrap();
    assert_eq!(removed, 2);
    assert_eq!(repo.list_for_request(a, 10).await.unwrap().len(), 3);

    // Total size: keep newest until the budget is used.
    repo.enforce_limits(&HistoryLimits {
        per_request: 100,
        total_bytes: 250,
        max_age_ms: None,
    })
    .await
    .unwrap();
    assert_eq!(repo.total_body_bytes().await.unwrap(), 200);

    clock.advance(10_000);
    repo.enforce_limits(&HistoryLimits {
        per_request: 100,
        total_bytes: u64::MAX,
        max_age_ms: Some(5_000),
    })
    .await
    .unwrap();
    assert_eq!(repo.total_body_bytes().await.unwrap(), 0);
}

#[tokio::test]
async fn clear_and_live_hashes() {
    let (s, clock) = setup().await;
    let repo = HistoryRepo::new(&s, &clock);
    let a = Id::new();
    let b = Id::new();
    repo.insert(record(a, 1, &"1".repeat(64))).await.unwrap();
    repo.insert(record(b, 1, &"2".repeat(64))).await.unwrap();
    assert_eq!(repo.live_body_hashes().await.unwrap().len(), 2);
    repo.clear_for_requests(&[a]).await.unwrap();
    assert!(repo.list_for_request(a, 10).await.unwrap().is_empty());
    assert_eq!(
        repo.live_body_hashes().await.unwrap(),
        std::collections::HashSet::from(["2".repeat(64)])
    );
}

#[tokio::test]
async fn workspace_log_is_searchable() {
    let (s, clock) = setup().await;
    let repo = HistoryRepo::new(&s, &clock);
    let ws = Id::new();
    let mut r = record(Id::new(), 1, &"1".repeat(64));
    r.workspace_id = ws;
    r.request_snapshot = serde_json::json!({ "method": "POST", "url": "https://api.test/orders" });
    repo.insert(r).await.unwrap();
    let mut r = record(Id::new(), 1, &"2".repeat(64));
    r.workspace_id = ws;
    repo.insert(r).await.unwrap();
    assert_eq!(
        repo.list_for_workspace(ws, Some("orders"), 50)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_for_workspace(ws, None, 50).await.unwrap().len(),
        2
    );
    let _ = Timestamp(0);
}
