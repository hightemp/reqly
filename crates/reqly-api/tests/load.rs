//! Load test of the multi-user server on SQLite (T7.1.6, ADR-011).
//!
//! Not part of the normal suite. Run with
//!
//! ```sh
//! LOAD_USERS=50 LOAD_SECS=10 cargo test --release -p reqly-api --test load -- --ignored --nocapture
//! ```
//!
//! Every simulated user works in their own workspace and in one shared
//! workspace: 70% reads (`tree.list`, `node.get`), 25% `node.update`, 5%
//! `node.create`, all through `dispatch_as` (authorization included) on a
//! database file in WAL mode, as the server runs it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_core::Id;
use reqly_storage::blobs::BlobStore;
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use reqly_storage::{OpenOptions, Storage};
use serde_json::{Value, json};

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Default)]
struct Stats {
    reads: Vec<Duration>,
    writes: Vec<Duration>,
    creates: Vec<Duration>,
    errors: Vec<String>,
}

fn percentile(sorted: &[Duration], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i].as_secs_f64() * 1000.0
}

fn report(name: &str, samples: &mut [Duration], secs: f64) {
    samples.sort();
    println!(
        "{name:<8} {:>8} ops {:>9.0}/s  p50 {:>7.2} ms  p95 {:>7.2} ms  p99 {:>7.2} ms  max {:>8.2} ms",
        samples.len(),
        samples.len() as f64 / secs,
        percentile(samples, 0.50),
        percentile(samples, 0.95),
        percentile(samples, 0.99),
        percentile(samples, 1.0),
    );
}

struct Space {
    collection: Value,
    nodes: Vec<Value>,
}

async fn seed(
    ctx: &AppContext,
    caller: Caller,
    workspace: Option<Value>,
    nodes: usize,
) -> (Value, Space) {
    let r = registry();
    let call = |name: &'static str, input: Value| {
        let r = &r;
        async move { dispatch_as(r, ctx, caller, name, input).await.unwrap() }
    };
    let ws = match workspace {
        Some(ws) => ws,
        None => call("workspace.create", json!({ "name": "Load" })).await["id"].clone(),
    };
    let collection = call(
        "collection.create",
        json!({ "workspaceId": ws, "name": "Load" }),
    )
    .await["id"]
        .clone();
    let mut ids = Vec::new();
    for i in 0..nodes {
        let node = call(
            "node.create",
            json!({
                "collectionId": collection, "parentId": null, "name": format!("Request {i}"),
                "data": { "kind": "request", "method": "GET", "url": format!("https://api.example.com/items/{i}") }
            }),
        )
        .await;
        ids.push(node["id"].clone());
    }
    (
        ws,
        Space {
            collection,
            nodes: ids,
        },
    )
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "load test: run explicitly with --ignored"]
async fn sqlite_under_concurrent_users() {
    let users = env("LOAD_USERS", 50) as usize;
    let secs = env("LOAD_SECS", 10);
    let nodes = env("LOAD_NODES", 100) as usize;

    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("reqly.db"), OpenOptions::default())
        .await
        .unwrap();
    let ctx = Arc::new(AppContext::new(
        Platform::Server,
        storage,
        BlobStore::new(dir.path().join("blobs")),
    ));

    // Users, their own workspaces and one workspace they all share.
    let repo = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let mut callers = Vec::new();
    for i in 0..users {
        let user = repo
            .create(&format!("user{i}"), "", "password1", false)
            .await
            .unwrap();
        callers.push(Caller::User {
            id: user.id,
            admin: false,
        });
    }
    let (team, shared) = seed(&ctx, callers[0], None, nodes).await;
    let team_id: Id = serde_json::from_value(team.clone()).unwrap();
    let members = MemberRepo::new(&ctx.storage, ctx.clock.as_ref());
    for caller in &callers[1..] {
        members
            .set(team_id, caller.user_id().unwrap(), Role::Editor)
            .await
            .unwrap();
    }
    let shared = Arc::new(shared);
    let mut own = Vec::new();
    for caller in &callers {
        own.push(Arc::new(seed(&ctx, *caller, None, nodes / 4).await.1));
    }
    println!(
        "seeded {users} users, {} workspaces, {} requests",
        users + 1,
        nodes + users * (nodes / 4)
    );

    let deadline = Instant::now() + Duration::from_secs(secs);
    let started = Instant::now();
    let mut tasks = Vec::new();
    for (n, caller) in callers.iter().copied().enumerate() {
        let ctx = ctx.clone();
        let shared = shared.clone();
        let own = own[n].clone();
        tasks.push(tokio::spawn(async move {
            let r = registry();
            let mut stats = Stats::default();
            let mut i: usize = n * 7919;
            while Instant::now() < deadline {
                i = i.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let space = if i.is_multiple_of(2) { &shared } else { &own };
                let node = space.nodes[(i >> 8) % space.nodes.len()].clone();
                let roll = (i >> 20) % 100;
                let (name, input, bucket) = match roll {
                    0..35 => (
                        "tree.list",
                        json!({ "collectionId": space.collection }),
                        0,
                    ),
                    35..70 => ("node.get", json!({ "id": node }), 0),
                    70..95 => (
                        "node.update",
                        json!({ "id": node, "name": format!("Renamed {i}") }),
                        1,
                    ),
                    _ => (
                        "node.create",
                        json!({ "collectionId": space.collection, "parentId": null, "name": "New",
                                "data": { "kind": "request", "method": "POST", "url": "https://x" } }),
                        2,
                    ),
                };
                let t = Instant::now();
                let result = dispatch_as(&r, &ctx, caller, name, input).await;
                let took = t.elapsed();
                match result {
                    Ok(_) => match bucket {
                        0 => stats.reads.push(took),
                        1 => stats.writes.push(took),
                        _ => stats.creates.push(took),
                    },
                    Err(e) => stats.errors.push(format!("{name}: {}: {}", e.code, e.message)),
                }
            }
            stats
        }));
    }
    let mut all = Stats::default();
    for task in tasks {
        let s = task.await.unwrap();
        all.reads.extend(s.reads);
        all.writes.extend(s.writes);
        all.creates.extend(s.creates);
        all.errors.extend(s.errors);
    }
    let elapsed = started.elapsed().as_secs_f64();

    println!("{users} users for {elapsed:.1} s:");
    report("read", &mut all.reads, elapsed);
    report("update", &mut all.writes, elapsed);
    report("create", &mut all.creates, elapsed);
    let total = all.reads.len() + all.writes.len() + all.creates.len();
    println!("total    {total:>8} ops {:>9.0}/s", total as f64 / elapsed);
    let mut errors = all.errors.clone();
    errors.sort();
    errors.dedup();
    println!("errors   {} ({errors:?})", all.errors.len());
    let db = std::fs::metadata(dir.path().join("reqly.db"))
        .unwrap()
        .len();
    println!("database {:.1} MiB", db as f64 / 1_048_576.0);
    assert!(
        all.errors
            .iter()
            .all(|e| e.ends_with("version_conflict") || e.contains("conflict")),
        "unexpected errors: {errors:?}"
    );
}
