//! Reqly desktop shell: forwards UI commands to the shared core registry and
//! core events to the webview.

use reqly_api::Registry;
use reqly_api::commands::{AppContext, Platform};
use reqly_core::CoreError;
use reqly_storage::blobs::BlobStore;
use reqly_storage::{OpenOptions, Storage};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// Name of the webview event carrying core events (see ui/src/lib/api/transport.ts).
pub const EVENT_NAME: &str = "reqly://event";

pub struct Core {
    ctx: AppContext,
    registry: Registry<AppContext>,
}

impl Core {
    pub fn new(ctx: AppContext) -> Self {
        Self {
            ctx,
            registry: reqly_api::commands::registry(),
        }
    }
}

#[tauri::command]
async fn dispatch(core: State<'_, Core>, name: String, input: Value) -> Result<Value, CoreError> {
    core.registry.dispatch(&core.ctx, &name, input).await
}

/// Opens the provider's sign-in page in a window and resolves with the URL
/// it redirects to (`redirect_uri…`), which is never loaded.
#[tauri::command]
async fn oauth2_sign_in<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    redirect_uri: String,
    title: String,
) -> Result<String, CoreError> {
    use std::sync::{Arc, Mutex};
    use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent};

    let start: tauri::Url = url.parse().map_err(|e| {
        CoreError::new(
            "auth.oauth2_config",
            format!("invalid authorization URL: {e}"),
        )
    })?;
    // Match on scheme, host, port and path; the query carries the answer.
    let target = redirect_uri
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let send = move |tx: &Arc<Mutex<Option<tokio::sync::oneshot::Sender<Option<String>>>>>, v| {
        if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
            let _ = tx.send(v);
        }
    };
    let on_nav = tx.clone();
    let label = format!(
        "oauth2-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(start))
        .title(title)
        .inner_size(520.0, 720.0)
        .on_navigation(move |nav| {
            let current = nav.as_str();
            let base = current.split(['?', '#']).next().unwrap_or("");
            if base == target {
                send(&on_nav, Some(current.to_string()));
                return false;
            }
            true
        })
        .build()
        .map_err(|e| CoreError::internal(e.to_string()))?;
    let on_close = tx.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            send(&on_close, None);
        }
    });
    let result = rx.await.ok().flatten();
    let _ = window.destroy();
    result.ok_or_else(|| CoreError::new("auth.oauth2_cancelled", "the sign-in window was closed"))
}

async fn open_core<R: Runtime>(app: &AppHandle<R>) -> Result<Core, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let storage = Storage::open(&data_dir.join("reqly.db"), OpenOptions::default()).await?;
    tracing::info!(data_dir = %data_dir.display(), "storage opened");
    let blobs = BlobStore::new(data_dir.join("blobs"));
    Ok(Core::new(AppContext::new(
        Platform::Desktop,
        storage,
        blobs,
    )))
}

/// Forwards core events to every webview until the bus closes.
fn forward_events<R: Runtime>(app: AppHandle<R>, core: &Core) {
    let mut events = core.ctx.events.subscribe();
    tauri::async_runtime::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match events.recv().await {
                Ok(event) => {
                    if let Err(e) = app.emit(EVENT_NAME, &event) {
                        tracing::warn!(error = %e, "failed to emit event");
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    let lagged = reqly_api::Event::new(
                        "transport.lagged",
                        serde_json::json!({ "skipped": skipped }),
                    );
                    let _ = app.emit(EVENT_NAME, &lagged);
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

/// Builder shared by the real app and tests. Without `core`, it is opened
/// during setup (it needs the app data dir); tests pass an in-memory one.
pub fn configure<R: Runtime>(base: tauri::Builder<R>, core: Option<Core>) -> tauri::Builder<R> {
    let base = match core {
        Some(core) => base.manage(core),
        None => base,
    };
    base.setup(|app| {
        let handle = app.handle().clone();
        if app.try_state::<Core>().is_none() {
            let core = tauri::async_runtime::block_on(open_core(&handle))?;
            app.manage(core);
        }
        forward_events(handle, &app.state::<Core>());
        Ok(())
    })
    .invoke_handler(tauri::generate_handler![dispatch, oauth2_sign_in])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("REQLY_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    configure(
        tauri::Builder::default().plugin(tauri_plugin_dialog::init()),
        None,
    )
    .run(tauri::generate_context!())
    .expect("error while running Reqly");
}
