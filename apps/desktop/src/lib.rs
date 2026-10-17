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
    .invoke_handler(tauri::generate_handler![dispatch])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("REQLY_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    configure(tauri::Builder::default(), None)
        .run(tauri::generate_context!())
        .expect("error while running Reqly");
}
