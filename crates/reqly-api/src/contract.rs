//! Transport contract suite.
//!
//! Each shell (axum server, Tauri desktop) implements [`Client`] over its real
//! transport and runs [`run_all`], so command routing, (de)serialization and
//! error mapping are verified end to end in the same way everywhere.

use std::future::Future;

use reqly_core::CoreError;
use serde_json::{Value, json};

pub trait Client {
    /// Sends a command through the real transport and returns the decoded result.
    fn call(&self, name: &str, input: Value) -> impl Future<Output = Result<Value, CoreError>>;
}

pub async fn run_all(client: &impl Client, expected_platform: &str) {
    app_info(client, expected_platform).await;
    ping_echoes(client).await;
    settings_round_trip(client).await;
    unknown_command(client).await;
    invalid_input(client).await;
}

async fn app_info(client: &impl Client, platform: &str) {
    let out = client.call("app.info", json!({})).await.expect("app.info");
    assert_eq!(out["name"], "Reqly");
    assert_eq!(out["platform"], platform);
}

async fn ping_echoes(client: &impl Client) {
    let out = client
        .call("app.ping", json!({ "message": "ünïcødé ✓" }))
        .await
        .expect("ping");
    assert_eq!(out, json!({ "echo": "ünïcødé ✓" }));
}

async fn settings_round_trip(client: &impl Client) {
    let key = "contract.settings";
    let value = json!({ "nested": [1, 2, { "x": null }], "big": 9007199254740991_i64 });
    client
        .call("settings.set", json!({ "key": key, "value": value }))
        .await
        .expect("set");
    let out = client
        .call("settings.get", json!({ "key": key }))
        .await
        .expect("get");
    assert_eq!(out["value"], value);
}

async fn unknown_command(client: &impl Client) {
    let err = client.call("no.such.command", json!({})).await.unwrap_err();
    assert_eq!(err.code, "command.unknown");
    assert_eq!(err.params["name"], "no.such.command");
}

async fn invalid_input(client: &impl Client) {
    let err = client
        .call("app.ping", json!({ "message": 42 }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "command.invalid_input");
}
