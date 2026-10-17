//! Built-in commands and the shared application context.

pub mod http;
pub mod tree;

use std::sync::Arc;

use reqly_core::{Clock, CoreResult, SystemClock};
use reqly_storage::Storage;
use reqly_storage::blobs::BlobStore;
use reqly_storage::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::events::EventBus;
use crate::registry::{Command, Registry};

/// Which shell hosts the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Platform {
    Desktop,
    Server,
    Test,
}

pub struct AppContext {
    pub platform: Platform,
    pub events: EventBus,
    pub storage: Storage,
    pub blobs: BlobStore,
    pub clock: Arc<dyn Clock>,
    /// Outgoing request restrictions (server build only).
    pub network_policy: Option<reqly_http::policy::NetworkPolicy>,
    /// Where the database and blobs live (shown in settings).
    pub data_dir: Option<String>,
    /// Cancel tokens of requests in flight, by request id.
    pub inflight: std::sync::Mutex<
        std::collections::HashMap<reqly_core::Id, reqly_http::engine::CancelToken>,
    >,
}

impl AppContext {
    pub fn new(platform: Platform, storage: Storage, blobs: BlobStore) -> Self {
        Self {
            platform,
            events: EventBus::default(),
            storage,
            blobs,
            clock: Arc::new(SystemClock),
            network_policy: None,
            data_dir: None,
            inflight: Default::default(),
        }
    }

    /// In-memory database and a throwaway blob dir, for tests.
    pub async fn for_tests() -> Self {
        Self::for_tests_on(Platform::Test).await
    }

    pub async fn for_tests_on(platform: Platform) -> Self {
        let dir = std::env::temp_dir().join(format!("reqly-test-{}", reqly_core::Id::new()));
        Self::new(
            platform,
            Storage::in_memory().await.expect("in-memory storage"),
            BlobStore::new(dir.join("blobs")),
        )
    }
}

/// Builds the registry with every command the UI may call.
pub fn registry() -> Registry<AppContext> {
    let mut r = Registry::new();
    r.register::<AppInfo>()
        .register::<AppPing>()
        .register::<SettingsGet>()
        .register::<SettingsSet>();
    tree::register(&mut r);
    http::register(&mut r);
    r.register::<SearchNodes>();
    r
}

#[derive(Debug, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Empty {}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfoOutput {
    pub name: String,
    pub version: String,
    pub platform: Platform,
    pub data_dir: Option<String>,
}

pub struct AppInfo;

impl Command<AppContext> for AppInfo {
    const NAME: &'static str = "app.info";
    type Input = Empty;
    type Output = AppInfoOutput;

    async fn run(ctx: &AppContext, _: Empty) -> CoreResult<AppInfoOutput> {
        Ok(AppInfoOutput {
            name: "Reqly".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: ctx.platform,
            data_dir: ctx.data_dir.clone(),
        })
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AppPingInput {
    pub message: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AppPingOutput {
    pub echo: String,
}

/// Round-trip check used by transport contract tests; also emits an event.
pub struct AppPing;

impl Command<AppContext> for AppPing {
    const NAME: &'static str = "app.ping";
    type Input = AppPingInput;
    type Output = AppPingOutput;

    async fn run(ctx: &AppContext, input: AppPingInput) -> CoreResult<AppPingOutput> {
        ctx.events
            .publish(crate::Event::new("app.pinged", &input.message));
        Ok(AppPingOutput {
            echo: input.message,
        })
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsGetInput {
    pub key: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SettingsGetOutput {
    #[ts(type = "unknown")]
    pub value: Option<Value>,
}

/// Reads a JSON setting; `value` is `null` when the key is absent.
pub struct SettingsGet;

impl Command<AppContext> for SettingsGet {
    const NAME: &'static str = "settings.get";
    type Input = SettingsGetInput;
    type Output = SettingsGetOutput;

    async fn run(ctx: &AppContext, input: SettingsGetInput) -> CoreResult<SettingsGetOutput> {
        let value = SettingsRepo::new(&ctx.storage).get(&input.key).await?;
        Ok(SettingsGetOutput { value })
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsSetInput {
    pub key: String,
    #[ts(type = "unknown")]
    pub value: Value,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingsChanged {
    pub key: String,
    #[ts(type = "unknown")]
    pub value: Value,
}

/// Stores a JSON setting and broadcasts `settings.changed` to other clients.
pub struct SettingsSet;

impl Command<AppContext> for SettingsSet {
    const NAME: &'static str = "settings.set";
    type Input = SettingsSetInput;
    type Output = Empty;

    async fn run(ctx: &AppContext, input: SettingsSetInput) -> CoreResult<Empty> {
        SettingsRepo::new(&ctx.storage)
            .set(&input.key, &input.value, ctx.clock.as_ref())
            .await?;
        ctx.events.publish(crate::Event::new(
            "settings.changed",
            SettingsChanged {
                key: input.key,
                value: input.value,
            },
        ));
        Ok(Empty {})
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchInput {
    pub workspace_id: reqly_core::Id,
    pub query: String,
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// Full-text search across a workspace (name, URL, method, description).
pub struct SearchNodes;

impl Command<AppContext> for SearchNodes {
    const NAME: &'static str = "search.nodes";
    type Input = SearchInput;
    type Output = Vec<reqly_core::model::NodeSummary>;

    async fn run(ctx: &AppContext, input: SearchInput) -> CoreResult<Self::Output> {
        let limit = input.limit.unwrap_or(50).min(500);
        reqly_storage::search::search_nodes(&ctx.storage, input.workspace_id, &input.query, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn app_info_reports_platform() {
        let ctx = AppContext::for_tests().await;
        let out = registry()
            .dispatch(&ctx, "app.info", json!({}))
            .await
            .unwrap();
        assert_eq!(out["name"], "Reqly");
        assert_eq!(out["platform"], "test");
    }

    #[tokio::test]
    async fn app_ping_echoes_and_publishes() {
        let ctx = AppContext::for_tests().await;
        let mut rx = ctx.events.subscribe();
        let out = registry()
            .dispatch(&ctx, "app.ping", json!({ "message": "hi" }))
            .await
            .unwrap();
        assert_eq!(out["echo"], "hi");
        let event = rx.recv().await.unwrap();
        assert_eq!(event.topic, "app.pinged");
        assert_eq!(event.payload, "hi");
    }

    #[tokio::test]
    async fn settings_round_trip_and_broadcast() {
        let ctx = AppContext::for_tests().await;
        let r = registry();
        let mut rx = ctx.events.subscribe();

        let missing = r
            .dispatch(&ctx, "settings.get", json!({ "key": "ui" }))
            .await
            .unwrap();
        assert_eq!(missing, json!({ "value": null }));

        r.dispatch(
            &ctx,
            "settings.set",
            json!({ "key": "ui", "value": { "theme": "nord" } }),
        )
        .await
        .unwrap();
        let got = r
            .dispatch(&ctx, "settings.get", json!({ "key": "ui" }))
            .await
            .unwrap();
        assert_eq!(got, json!({ "value": { "theme": "nord" } }));

        let event = rx.recv().await.unwrap();
        assert_eq!(event.topic, "settings.changed");
        assert_eq!(event.payload["key"], "ui");
    }
}
