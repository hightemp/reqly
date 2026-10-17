//! Reqly self-hosted server: serves the web UI and the core API.

mod api;
mod config;
mod static_files;

use std::sync::Arc;

use reqly_api::Registry;
use reqly_api::commands::{AppContext, Platform};
use reqly_storage::{OpenOptions, Storage};

pub use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub ctx: Arc<AppContext>,
    pub registry: Arc<Registry<AppContext>>,
}

impl AppState {
    pub fn new(ctx: AppContext) -> Self {
        Self {
            ctx: Arc::new(ctx),
            registry: Arc::new(reqly_api::commands::registry()),
        }
    }

    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        let storage = Storage::open(&config.database_path(), OpenOptions::default()).await?;
        Ok(Self::new(AppContext::new(Platform::Server, storage)))
    }

    pub async fn for_tests() -> Self {
        let storage = Storage::in_memory().await.expect("in-memory storage");
        Self::new(AppContext::new(Platform::Server, storage))
    }
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route(
            "/healthz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
        .nest("/api/v1", api::router())
        .fallback(static_files::serve)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
