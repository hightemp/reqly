//! The Android build: features the platform cannot offer are refused with a
//! clear error, and `app.info` lists them so the UI can explain.

use reqly_api::commands::{AppContext, Platform, registry};
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    collection: Value,
}

impl Fx {
    async fn on(platform: Platform) -> Self {
        let ctx = AppContext::for_tests_on(platform).await;
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let collection =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();
        Self { ctx, collection }
    }

    async fn try_call(&self, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
        registry().dispatch(&self.ctx, name, input).await
    }

    /// Sends a request with these settings; returns the recorded error.
    async fn send_error(&self, settings: Value) -> Value {
        let id = call(
            &self.ctx,
            "node.create",
            json!({
                "collectionId": self.collection, "parentId": null, "name": "r",
                "data": { "kind": "request", "url": "http://127.0.0.1:9/", "settings": settings }
            }),
        )
        .await["id"]
            .clone();
        match self
            .try_call("request.send", json!({ "requestId": id }))
            .await
        {
            Ok(record) => record["error"].clone(),
            Err(e) => serde_json::to_value(&e).unwrap(),
        }
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    registry()
        .dispatch(ctx, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[tokio::test]
async fn app_info_lists_what_the_platform_cannot_do() {
    let mobile = Fx::on(Platform::Mobile).await;
    let info = call(&mobile.ctx, "app.info", json!({})).await;
    assert_eq!(info["platform"], "mobile");
    assert_eq!(
        info["unavailable"],
        json!(["unixSocket", "interface", "osTag"])
    );

    let desktop = Fx::on(Platform::Desktop).await;
    let info = call(&desktop.ctx, "app.info", json!({})).await;
    assert_eq!(info["unavailable"], json!([]));
}

#[tokio::test]
async fn mobile_refuses_unix_sockets_and_interface_binding() {
    let fx = Fx::on(Platform::Mobile).await;
    let error = fx.send_error(json!({ "unixSocket": "/tmp/x.sock" })).await;
    assert_eq!(error["code"], "http.unavailable");
    assert_eq!(error["params"]["setting"], "unixSocket");

    let error = fx.send_error(json!({ "interface": "eth0" })).await;
    assert_eq!(error["code"], "http.unavailable");
    assert_eq!(error["params"]["setting"], "interface");
}

#[tokio::test]
async fn mobile_refuses_the_os_tag() {
    let fx = Fx::on(Platform::Mobile).await;
    for text in ["{% os 'platform' %}", "{% os 'hostname' %}"] {
        let err = fx
            .try_call(
                "template.preview",
                json!({ "collectionId": fx.collection, "text": text }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, "template.unavailable", "{text}");
    }
}

#[tokio::test]
async fn mobile_leaves_the_os_tag_out_of_the_catalog() {
    let names = |out: Value| -> Vec<String> {
        out["functions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect()
    };
    let mobile = Fx::on(Platform::Mobile).await;
    let out = call(&mobile.ctx, "template.functions", json!({})).await;
    assert!(!names(out).contains(&"os".to_string()));

    let desktop = Fx::on(Platform::Desktop).await;
    let out = call(&desktop.ctx, "template.functions", json!({})).await;
    assert!(names(out).contains(&"os".to_string()));
}
