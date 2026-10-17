use reqly_core::model::EnvironmentKind;
use reqly_core::{FixedClock, Id};
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::tree::TreeRepo;
use reqly_storage::{OpenOptions, Storage};
use serde_json::json;

async fn setup(storage: &Storage, clock: &FixedClock) -> (Id, Id) {
    let tree = TreeRepo::new(storage, clock);
    let ws = tree.create_workspace("W").await.unwrap().meta.id;
    let col = tree.create_collection(ws, "C").await.unwrap().meta.id;
    (ws, col)
}

#[tokio::test]
async fn base_environment_is_created_on_demand() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let (_, col) = setup(&s, &clock).await;
    let repo = EnvironmentRepo::new(&s, &clock);
    let first = repo.list_for_collection(col).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].kind, EnvironmentKind::Base);
    assert_eq!(
        repo.list_for_collection(col).await.unwrap().len(),
        1,
        "created once"
    );
}

#[tokio::test]
async fn create_update_duplicate_delete() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let (ws, col) = setup(&s, &clock).await;
    let repo = EnvironmentRepo::new(&s, &clock);
    let dev = repo
        .create(ws, Some(col), EnvironmentKind::Sub, "dev")
        .await
        .unwrap();
    let updated = repo
        .update(
            dev.meta.id,
            Some("development"),
            Some(Some("#ff0000")),
            Some(
                json!({ "host": "localhost", "port": 8080 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "development");
    assert_eq!(updated.color.as_deref(), Some("#ff0000"));
    assert_eq!(updated.data["port"], 8080);
    assert_eq!(updated.meta.version.0, 2);

    let copy = repo.duplicate(dev.meta.id).await.unwrap();
    assert_eq!(copy.name, "development (copy)");
    assert_eq!(copy.data, updated.data);

    let names: Vec<_> = repo
        .list_for_collection(col)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        names,
        ["Base Environment", "development", "development (copy)"]
    );

    repo.delete(copy.meta.id).await.unwrap();
    assert_eq!(repo.list_for_collection(col).await.unwrap().len(), 2);
}

#[tokio::test]
async fn base_cannot_be_deleted() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let (_, col) = setup(&s, &clock).await;
    let repo = EnvironmentRepo::new(&s, &clock);
    let base = repo.list_for_collection(col).await.unwrap().remove(0);
    assert_eq!(
        repo.delete(base.meta.id).await.unwrap_err().code,
        "environment.base_delete"
    );
}

#[tokio::test]
async fn global_environments_belong_to_the_workspace() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let (ws, _) = setup(&s, &clock).await;
    let repo = EnvironmentRepo::new(&s, &clock);
    repo.create(ws, None, EnvironmentKind::Global, "shared")
        .await
        .unwrap();
    let globals = repo.list_global(ws).await.unwrap();
    assert_eq!(globals.len(), 1);
    assert_eq!(globals[0].collection_id, None);
}

#[tokio::test]
async fn secrets_are_encrypted_at_rest_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reqly.db");
    let clock = FixedClock::new(1);
    let env_id;
    {
        let s = Storage::open(&path, OpenOptions::default()).await.unwrap();
        let (ws, col) = setup(&s, &clock).await;
        let repo = EnvironmentRepo::new(&s, &clock);
        let env = repo
            .create(ws, Some(col), EnvironmentKind::Sub, "prod")
            .await
            .unwrap();
        env_id = env.meta.id;
        repo.update(
            env_id,
            None,
            None,
            Some(
                json!({ "token": "s3cr3t", "host": "api.prod" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            Some(vec!["token".into()]),
        )
        .await
        .unwrap();
        let raw: String = sqlx::query_scalar("SELECT data FROM environments WHERE id = ?")
            .bind(env_id.to_string())
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert!(!raw.contains("s3cr3t"), "{raw}");
        assert!(raw.contains("api.prod"));
        s.close().await;
    }
    assert!(dir.path().join("secret.key").exists());
    let s = Storage::open(&path, OpenOptions::default()).await.unwrap();
    let env = EnvironmentRepo::new(&s, &clock).get(env_id).await.unwrap();
    assert_eq!(env.data["token"], "s3cr3t");
    assert_eq!(env.secrets, ["token"]);
}
