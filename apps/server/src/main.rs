use std::path::PathBuf;

use reqly_server::{AppState, Config, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_file = std::env::var_os("REQLY_CONFIG")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("reqly.toml")).filter(|p| p.exists()));
    let config = Config::load(config_file.as_deref(), |k| std::env::var(k).ok())?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log))
        .init();

    let state = AppState::from_config(&config).await?;
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(addr = %listener.local_addr()?, data_dir = %config.data_dir.display(), "reqly server listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
