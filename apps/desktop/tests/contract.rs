//! Runs the shared transport contract through Tauri IPC (mock runtime).

use reqly_api::commands::{AppContext, Platform};
use reqly_api::contract::{Client, run_all};
use reqly_core::CoreError;
use reqly_desktop_lib::{Core, configure};
use reqly_storage::Storage;
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::{InvokeRequest, WebviewWindow, WebviewWindowBuilder};

struct IpcClient(WebviewWindow<MockRuntime>);

impl Client for IpcClient {
    async fn call(&self, name: &str, input: Value) -> Result<Value, CoreError> {
        let request = InvokeRequest {
            cmd: "dispatch".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // Same origin as the webview, so the request counts as local.
            url: self.0.url().expect("webview url"),
            body: InvokeBody::Json(json!({ "name": name, "input": input })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        match tauri::test::get_ipc_response(&self.0, request) {
            Ok(body) => Ok(body.deserialize::<Value>().expect("json response")),
            Err(err) => Err(serde_json::from_value(err).expect("CoreError")),
        }
    }
}

#[test]
fn ipc_transport_satisfies_contract() {
    let storage = tauri::async_runtime::block_on(Storage::in_memory()).unwrap();
    let core = Core::new(AppContext::new(Platform::Desktop, storage));
    let app = configure(mock_builder(), Some(core))
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    tauri::async_runtime::block_on(run_all(&IpcClient(webview), "desktop"));
}
