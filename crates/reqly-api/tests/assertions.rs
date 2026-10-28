//! No-code assertions evaluated by request.send.

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

async fn setup() -> (AppContext, Value) {
    let ctx = AppContext::for_tests().await;
    let workspace = call(&ctx, "workspace.ensureDefault", json!({}))
        .await
        .unwrap()[0]["id"]
        .clone();
    let collection = call(&ctx, "collection.list", json!({ "workspaceId": workspace }))
        .await
        .unwrap()[0]["id"]
        .clone();
    (ctx, collection)
}

async fn request(ctx: &AppContext, collection: &Value, data: Value) -> Value {
    call(
        ctx,
        "node.create",
        json!({ "collectionId": collection, "parentId": null, "name": "r", "data": data }),
    )
    .await
    .unwrap()["id"]
        .clone()
}

fn assertion(id: &str, source: &str, property: &str, operator: &str, value: &str) -> Value {
    json!({ "id": id, "source": source, "property": property, "operator": operator, "value": value })
}

#[tokio::test]
async fn send_evaluates_enabled_assertions_with_templates() {
    let server = TestServer::start().await;
    let (ctx, collection) = setup().await;
    let mut disabled = assertion(
        "00000000-0000-7000-8000-000000000004",
        "status",
        "",
        "equals",
        "500",
    );
    disabled["enabled"] = json!(false);
    let id = request(
        &ctx,
        &collection,
        json!({ "kind": "request", "method": "POST", "url": server.url("/echo"),
                "variables": { "method": "POST" },
                "body": { "kind": "json", "text": "{\"a\":1}" },
                "scripts": { "afterResponse": "pm.variables.set('expected', 'POST');" },
                "assertions": [
                    assertion("00000000-0000-7000-8000-000000000001", "status", "", "equals", "200"),
                    assertion("00000000-0000-7000-8000-000000000002", "jsonPath", "$.method", "equals", "{{ expected }}"),
                    assertion("00000000-0000-7000-8000-000000000003", "header", "X-Missing", "exists", ""),
                    disabled,
                ] }),
    )
    .await;
    let record = call(&ctx, "request.send", json!({ "requestId": id }))
        .await
        .unwrap();
    let results = record["assertions"].as_array().unwrap();
    let summary: Vec<(&str, bool)> = results
        .iter()
        .map(|r| (r["id"].as_str().unwrap(), r["passed"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        summary,
        [
            ("00000000-0000-7000-8000-000000000001", true),
            ("00000000-0000-7000-8000-000000000002", true),
            ("00000000-0000-7000-8000-000000000003", false),
        ],
        "{record}"
    );

    // Kept in history.
    let stored = call(&ctx, "response.get", json!({ "id": record["id"] }))
        .await
        .unwrap();
    assert_eq!(stored["assertions"], record["assertions"]);
}

#[tokio::test]
async fn failed_requests_fail_their_assertions() {
    let (ctx, collection) = setup().await;
    let id = request(
        &ctx,
        &collection,
        json!({ "kind": "request", "url": "http://127.0.0.1:1/",
                "assertions": [
                    assertion("00000000-0000-7000-8000-000000000001", "status", "", "equals", "200"),
                    assertion("00000000-0000-7000-8000-000000000002", "status", "", "notExists", ""),
                ] }),
    )
    .await;
    let record = call(&ctx, "request.send", json!({ "requestId": id }))
        .await
        .unwrap();
    assert!(record["error"].is_object());
    let passed: Vec<bool> = record["assertions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["passed"].as_bool().unwrap())
        .collect();
    assert_eq!(passed, [false, true]);
}

#[tokio::test]
async fn template_errors_are_reported_per_assertion() {
    let server = TestServer::start().await;
    let (ctx, collection) = setup().await;
    let id = request(
        &ctx,
        &collection,
        json!({ "kind": "request", "url": server.url("/echo"),
                "assertions": [
                    assertion("00000000-0000-7000-8000-000000000001", "status", "", "equals", "{% nope %}"),
                    assertion("00000000-0000-7000-8000-000000000002", "status", "", "equals", "200"),
                ] }),
    )
    .await;
    let record = call(&ctx, "request.send", json!({ "requestId": id }))
        .await
        .unwrap();
    let results = record["assertions"].as_array().unwrap();
    assert!(
        results[0]["error"]["code"]
            .as_str()
            .unwrap()
            .starts_with("template."),
        "{record}"
    );
    assert_eq!(results[1]["passed"], true);
}
