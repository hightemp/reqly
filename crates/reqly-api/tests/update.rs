//! Update checks (update.check) against a fake GitHub Releases API.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use reqly_api::commands::{AppContext, registry, update::UpdateConfig};
use reqly_core::FixedClock;
use serde_json::{Value, json};

const HOUR: i64 = 3_600_000;

/// Answers `GET /releases` with whatever the test put in `body`.
#[derive(Clone, Default)]
struct Fake {
    status: Arc<Mutex<u16>>,
    body: Arc<Mutex<String>>,
    hits: Arc<AtomicUsize>,
    user_agent: Arc<Mutex<String>>,
}

impl Fake {
    fn answer(&self, status: u16, body: impl Into<String>) {
        *self.status.lock().unwrap() = status;
        *self.body.lock().unwrap() = body.into();
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

async fn releases(State(fake): State<Fake>, headers: HeaderMap) -> impl IntoResponse {
    fake.hits.fetch_add(1, Ordering::SeqCst);
    *fake.user_agent.lock().unwrap() = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let status = StatusCode::from_u16(*fake.status.lock().unwrap()).unwrap();
    (
        status,
        [("content-type", "application/json")],
        fake.body.lock().unwrap().clone(),
    )
}

fn release(tag: &str, prerelease: bool) -> Value {
    json!({
        "tag_name": tag,
        "name": format!("Reqly {tag}"),
        "body": format!("Notes of {tag}"),
        "draft": false,
        "prerelease": prerelease,
        "published_at": "2026-11-04T10:00:00Z",
        "html_url": format!("https://github.com/hightemp/reqly/releases/tag/{tag}"),
        "assets": [{
            "name": "latest.json",
            "size": 2048,
            "browser_download_url": format!("https://example.test/{tag}/latest.json"),
        }],
    })
}

struct Fx {
    ctx: AppContext,
    clock: Arc<FixedClock>,
    fake: Fake,
}

impl Fx {
    async fn new(current: &str) -> Self {
        let fake = Fake::default();
        fake.answer(200, "[]");
        let app = axum::Router::new()
            .route("/releases", axum::routing::get(releases))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/releases", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let clock = Arc::new(FixedClock::new(1_800_000_000_000));
        let mut ctx = AppContext::for_tests().await;
        ctx.clock = clock.clone();
        ctx.updates = UpdateConfig {
            url: Some(url),
            current: current.into(),
        };
        Self { ctx, clock, fake }
    }

    async fn call(&self, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
        registry().dispatch(&self.ctx, name, input).await
    }

    async fn check(&self, manual: bool) -> Value {
        self.call("update.check", json!({ "manual": manual }))
            .await
            .unwrap()
    }

    async fn settings(&self, value: Value) {
        self.call("settings.set", json!({ "key": "updates", "value": value }))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn finds_the_newest_stable_release() {
    let fx = Fx::new("0.9.0").await;
    fx.fake.answer(
        200,
        json!([
            release("v0.11.0-beta.1", true),
            release("v0.10.1", false),
            release("v0.10.0", false),
            release("v0.9.0", false),
            release("nightly", false),
        ])
        .to_string(),
    );
    let out = fx.check(true).await;
    assert_eq!(out["enabled"], true);
    assert_eq!(out["current"], "0.9.0");
    assert_eq!(out["checkedAt"], 1_800_000_000_000_i64);
    let r = &out["release"];
    assert_eq!(r["version"], "0.10.1");
    assert_eq!(r["tag"], "v0.10.1");
    assert_eq!(r["notes"], "Notes of v0.10.1");
    assert_eq!(r["prerelease"], false);
    assert_eq!(
        r["url"],
        "https://github.com/hightemp/reqly/releases/tag/v0.10.1"
    );
    assert_eq!(r["publishedAt"], "2026-11-04T10:00:00Z");
    assert_eq!(r["assets"][0]["name"], "latest.json");
    assert_eq!(
        r["assets"][0]["url"],
        "https://example.test/v0.10.1/latest.json"
    );
    // No telemetry: a plain product User-Agent (GitHub requires one).
    assert_eq!(
        *fx.fake.user_agent.lock().unwrap(),
        format!("Reqly/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn the_prerelease_channel_and_up_to_date() {
    let fx = Fx::new("0.10.0").await;
    let mut draft = release("v0.12.0", false);
    draft["draft"] = json!(true);
    fx.fake.answer(
        200,
        json!([
            draft,
            release("v0.11.0-beta.2", true),
            release("v0.10.0", false)
        ])
        .to_string(),
    );
    let out = fx.check(true).await;
    assert_eq!(out["release"], Value::Null, "{out}");

    fx.settings(json!({ "channel": "prerelease" })).await;
    let out = fx.check(true).await;
    assert_eq!(out["release"]["version"], "0.11.0-beta.2");
    assert_eq!(out["release"]["prerelease"], true);
}

#[tokio::test]
async fn a_prerelease_build_sees_the_final_release() {
    let fx = Fx::new("0.10.0-beta.1").await;
    fx.fake
        .answer(200, json!([release("v0.10.0", false)]).to_string());
    let out = fx.check(true).await;
    assert_eq!(out["release"]["version"], "0.10.0");
}

#[tokio::test]
async fn skipped_versions_and_the_check_interval() {
    let fx = Fx::new("0.9.0").await;
    fx.fake.answer(
        200,
        json!([release("v0.10.0", false), release("v0.9.0", false)]).to_string(),
    );
    assert_eq!(fx.check(false).await["release"]["version"], "0.10.0");
    assert_eq!(fx.fake.hits(), 1);

    // Within the interval an automatic check answers from the last result.
    fx.clock.advance(2 * HOUR);
    let out = fx.check(false).await;
    assert_eq!(out["release"]["version"], "0.10.0");
    assert_eq!(out["checkedAt"], 1_800_000_000_000_i64);
    assert_eq!(fx.fake.hits(), 1);

    // A skipped version is hidden from automatic checks, not manual ones.
    fx.settings(json!({ "skippedVersion": "0.10.0" })).await;
    assert_eq!(fx.check(false).await["release"], Value::Null);
    assert_eq!(fx.check(true).await["release"]["version"], "0.10.0");
    assert_eq!(fx.fake.hits(), 2);

    // A newer release than the skipped one shows again.
    fx.fake.answer(
        200,
        json!([release("v0.10.1", false), release("v0.10.0", false)]).to_string(),
    );
    fx.clock.advance(25 * HOUR);
    assert_eq!(fx.check(false).await["release"]["version"], "0.10.1");
    assert_eq!(fx.fake.hits(), 3);

    // A shorter interval from the settings.
    fx.settings(json!({ "intervalHours": 1 })).await;
    fx.clock.advance(2 * HOUR);
    fx.check(false).await;
    assert_eq!(fx.fake.hits(), 4);
}

#[tokio::test]
async fn automatic_checks_can_be_turned_off() {
    let fx = Fx::new("0.9.0").await;
    fx.fake
        .answer(200, json!([release("v0.10.0", false)]).to_string());
    fx.settings(json!({ "autoCheck": false })).await;
    let out = fx.check(false).await;
    assert_eq!(out["release"], Value::Null);
    assert_eq!(fx.fake.hits(), 0);
    // "Check for updates" still works.
    assert_eq!(fx.check(true).await["release"]["version"], "0.10.0");
}

#[tokio::test]
async fn disabled_without_a_source() {
    let mut fx = Fx::new("0.9.0").await;
    fx.ctx.updates.url = None;
    let out = fx.check(true).await;
    assert_eq!(out["enabled"], false);
    assert_eq!(out["release"], Value::Null);
    assert_eq!(fx.fake.hits(), 0);
}

#[tokio::test]
async fn errors_keep_the_last_result() {
    let fx = Fx::new("0.9.0").await;
    fx.fake
        .answer(200, json!([release("v0.10.0", false)]).to_string());
    fx.check(true).await;

    fx.fake.answer(503, "{}");
    let err = fx
        .call("update.check", json!({ "manual": true }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "update.network");
    assert_eq!(err.params["status"], 503);

    fx.fake.answer(200, "<html>not json</html>");
    let err = fx
        .call("update.check", json!({ "manual": true }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "update.invalid");

    // The last good answer is still there for automatic checks.
    let out = fx.check(false).await;
    assert_eq!(out["release"]["version"], "0.10.0");
}

#[tokio::test]
async fn unreachable_source_is_a_network_error() {
    let mut fx = Fx::new("0.9.0").await;
    fx.ctx.updates.url = Some("http://127.0.0.1:9/releases".into());
    let err = fx
        .call("update.check", json!({ "manual": true }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "update.network");
}

#[tokio::test]
async fn only_admins_check_on_a_shared_server() {
    use reqly_api::access::Caller;
    use reqly_api::commands::dispatch_as;

    let fx = Fx::new("0.9.0").await;
    fx.fake
        .answer(200, json!([release("v0.10.0", false)]).to_string());
    let r = registry();
    let check = |caller| {
        dispatch_as(
            &r,
            &fx.ctx,
            caller,
            "update.check",
            json!({ "manual": true }),
        )
    };
    let id = reqly_core::Id::new();
    let err = check(Caller::User { id, admin: false }).await.unwrap_err();
    assert_eq!(err.code, "access.denied");
    let out = check(Caller::User { id, admin: true }).await.unwrap();
    assert_eq!(out["release"]["version"], "0.10.0");
}
