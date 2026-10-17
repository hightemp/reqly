use std::path::PathBuf;

use reqly_server::{AppState, Config, router};
use tracing_subscriber::EnvFilter;

/// `reqly-server healthcheck`: exit 0 when /healthz answers (for Docker).
fn healthcheck(bind: &str) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let port = bind.rsplit(':').next().unwrap_or("8484");
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port.parse::<u16>()?))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    stream.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    anyhow::ensure!(
        response.starts_with("HTTP/1.0 200") || response.starts_with("HTTP/1.1 200"),
        "unhealthy"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_file = std::env::var_os("REQLY_CONFIG")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("reqly.toml")).filter(|p| p.exists()));
    let config = Config::load(config_file.as_deref(), |k| std::env::var(k).ok())?;
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck(&config.bind);
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log))
        .init();

    let state = AppState::from_config(&config).await?;
    let loopback = config.bind.starts_with("127.")
        || config.bind.starts_with("localhost")
        || config.bind.starts_with("[::1]");
    if config.auth.mode == reqly_server::AuthMode::None && !loopback {
        tracing::warn!(bind = %config.bind, "authentication is disabled on a non-loopback address; set REQLY_AUTH_MODE=token");
    }
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(addr = %listener.local_addr()?, data_dir = %config.data_dir.display(), "reqly server listening");

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
