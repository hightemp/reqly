//! The desktop listener: `127.0.0.1:<port>/mcp` while MCP is enabled.
//! It follows `mcp.configChanged`, so enabling, disabling and changing the
//! port need no restart.

use std::sync::Arc;

use reqly_api::commands::mcp::{McpConfig, McpListen, load_config};
use reqly_api::commands::{AppContext, Platform, registry};
use tokio::net::TcpListener;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::{McpState, OriginPolicy, SettingsGate, TokenKind, catalog_tools, router};

/// Runs the listener for the lifetime of the returned task.
pub fn serve_local(ctx: Arc<AppContext>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut events = ctx.events.subscribe();
        let state = McpState {
            ctx: ctx.clone(),
            registry: Arc::new(registry()),
            tools: Arc::new(catalog_tools()),
            gate: Arc::new(SettingsGate::new(ctx.clone(), TokenKind::Mcp)),
            origin: OriginPolicy::Loopback,
        };
        let mut running: Option<(u16, JoinHandle<()>)> = None;
        loop {
            let config = load_config(&ctx.storage).await.unwrap_or_default();
            reconcile(&ctx, &state, &config, &mut running).await;
            loop {
                match events.recv().await {
                    Ok(e) if e.topic == "mcp.configChanged" => break,
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    })
}

async fn reconcile(
    ctx: &AppContext,
    state: &McpState,
    config: &McpConfig,
    running: &mut Option<(u16, JoinHandle<()>)>,
) {
    let wanted = (config.enabled && ctx.platform != Platform::Mobile).then_some(config.port);
    let current = running.as_ref().map(|(port, _)| *port);
    let failed = ctx.mcp_listen.get().error.is_some();
    if wanted == current && !failed {
        return;
    }
    if let Some((_, task)) = running.take() {
        task.abort();
        let _ = task.await;
    }
    let Some(port) = wanted else {
        ctx.mcp_listen.set(&ctx.events, McpListen::default());
        return;
    };
    match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => {
            let app = router(state.clone());
            let task = tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::warn!(error = %e, "MCP listener stopped");
                }
            });
            *running = Some((port, task));
            tracing::info!(port, "MCP listening");
            ctx.mcp_listen.set(
                &ctx.events,
                McpListen {
                    endpoint: Some(format!("http://127.0.0.1:{port}/mcp")),
                    error: None,
                },
            );
        }
        Err(e) => {
            tracing::warn!(port, error = %e, "MCP listener could not start");
            ctx.mcp_listen.set(
                &ctx.events,
                McpListen {
                    endpoint: None,
                    error: Some(format!("127.0.0.1:{port}: {e}")),
                },
            );
        }
    }
}
