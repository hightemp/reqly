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
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("healthcheck") => return healthcheck(&config.bind),
        Some("admin") => {
            let password = std::env::var("REQLY_ADMIN_PASSWORD").ok().or_else(|| {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok().map(|_| line)
            });
            println!(
                "{}",
                reqly_server::cli::admin(&config, &args[1..], password).await?
            );
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown command {other}\n{}", reqly_server::cli::USAGE),
        None => {}
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
