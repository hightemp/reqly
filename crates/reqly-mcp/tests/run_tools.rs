//! Run and import/export tools (TASKS T13.5).

mod common;

use common::Fx;
use reqly_mcp::HIDDEN;
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn env(fx: &Fx, col: &Value, name: &str, data: Value, secrets: &[&str]) -> Value {
    let ws = fx.workspace().await;
    let env = fx
        .cmd(
            "environment.create",
            json!({ "workspaceId": ws["id"], "collectionId": col["id"], "kind": "sub", "name": name, "data": data }),
        )
        .await;
    if !secrets.is_empty() {
        fx.cmd(
            "environment.update",
            json!({ "id": env["id"], "secrets": secrets }),
        )
        .await;
    }
    env
}

fn request(method: &str, url: &str) -> Value {
    json!({ "kind": "request", "method": method, "url": url })
}

#[tokio::test]
async fn sends_a_request_and_keeps_the_response_in_history() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let mut data = request("POST", &server.url("/echo?x=1"));
    data["body"] = json!({ "kind": "json", "text": "{\"hello\":\"world\"}" });
    let req = fx.node(&col, None, "Echo", data).await;

    let out = fx.tool("send_request", json!({ "id": req["id"] })).await;
    assert_eq!(out["status"], 200);
    assert_eq!(out["request"]["method"], "POST");
    assert!(out["body"].as_str().unwrap().contains("world"), "{out}");
    assert!(out["totalMs"].is_number());
    let history = fx
        .cmd("response.list", json!({ "requestId": req["id"] }))
        .await;
    assert_eq!(
        history[0]["id"], out["id"],
        "stored like a send from the app"
    );

    let out = fx
        .tool(
            "send_request",
            json!({ "id": req["id"], "maxBodyBytes": 8 }),
        )
        .await;
    assert_eq!(out["body"].as_str().unwrap().len(), 8);
    assert_eq!(out["bodyTruncated"], true);
}

#[tokio::test]
async fn sends_with_another_environment_and_variables_without_switching() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let staging = env(
        &fx,
        &col,
        "Staging",
        json!({ "base_url": server.url("") }),
        &[],
    )
    .await;
    let req = fx
        .node(
            &col,
            None,
            "Status",
            request("GET", "{{base_url}}/status/{{code}}"),
        )
        .await;

    let out = fx
        .tool(
            "send_request",
            json!({ "id": req["id"], "environmentId": staging["id"], "variables": { "code": 201 } }),
        )
        .await;
    assert_eq!(out["status"], 201, "{out}");
    let list = fx
        .tool("list_environments", json!({ "collectionId": col["id"] }))
        .await;
    assert_eq!(
        list["activeId"],
        Value::Null,
        "the active environment is untouched"
    );
    let history = fx
        .cmd("response.list", json!({ "requestId": req["id"] }))
        .await;
    assert_eq!(history[0]["id"], out["id"]);
}

#[tokio::test]
async fn secrets_do_not_come_back_in_responses_or_renders() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let prod = env(
        &fx,
        &col,
        "Prod",
        json!({ "base_url": server.url(""), "token": "tok-s3cret-1" }),
        &["token"],
    )
    .await;
    fx.cmd(
        "environment.setActive",
        json!({ "collectionId": col["id"], "environmentId": prod["id"] }),
    )
    .await;
    let mut data = request("GET", "{{base_url}}/echo?t={{token}}");
    data["headers"] = json!([
        { "name": "Authorization", "value": "Bearer {{token}}" },
        { "name": "X-Literal", "value": "lit-k3y-9" }
    ]);
    data["auth"] = json!({ "type": "apiKey", "key": "X-Literal-Auth", "value": "lit-k3y-9", "location": "header" });
    let req = fx.node(&col, None, "Echo", data).await;

    let out = fx.tool("send_request", json!({ "id": req["id"] })).await;
    assert_eq!(out["status"], 200);
    let text = out.to_string();
    assert!(!text.contains("tok-s3cret-1"), "{text}");
    assert!(!text.contains("lit-k3y-9"), "{text}");
    assert!(text.contains(HIDDEN));

    let again = fx.tool("get_response", json!({ "id": out["id"] })).await;
    assert!(!again.to_string().contains("tok-s3cret-1"));

    let rendered = fx.tool("render_request", json!({ "id": req["id"] })).await;
    assert_eq!(rendered["method"], "GET");
    assert_eq!(
        rendered["url"],
        format!("{}/echo?t={HIDDEN}", server.url(""))
    );
    assert_eq!(rendered["headers"][0]["value"], format!("Bearer {HIDDEN}"));
    assert!(!rendered.to_string().contains("lit-k3y-9"));
}

#[tokio::test]
async fn render_shows_the_request_as_it_would_be_sent() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let dev = env(
        &fx,
        &col,
        "Dev",
        json!({ "base_url": "https://dev.example", "user": "ada" }),
        &[],
    )
    .await;
    fx.cmd(
        "environment.setActive",
        json!({ "collectionId": col["id"], "environmentId": dev["id"] }),
    )
    .await;
    let mut data = request("PUT", "{{base_url}}/users/{{user}}");
    data["headers"] = json!([{ "name": "X-User", "value": "{{user}}" }, { "name": "Off", "value": "x", "enabled": false }]);
    data["params"] = json!([{ "name": "v", "value": "{{user}}" }]);
    data["body"] = json!({ "kind": "json", "text": "{\"name\":\"{{user}}\"}" });
    let req = fx.node(&col, None, "Put", data).await;
    let out = fx.tool("render_request", json!({ "id": req["id"] })).await;
    assert_eq!(out["url"], "https://dev.example/users/ada");
    assert_eq!(
        out["headers"],
        json!([{ "name": "X-User", "value": "ada" }])
    );
    assert_eq!(out["params"], json!([{ "name": "v", "value": "ada" }]));
    assert_eq!(out["body"], "{\"name\":\"ada\"}");
}

#[tokio::test]
async fn network_errors_are_tool_errors() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let req = fx
        .node(
            &col,
            None,
            "Down",
            request("GET", "http://127.0.0.1:9/nothing"),
        )
        .await;
    let err = fx
        .tool_err("send_request", json!({ "id": req["id"] }))
        .await;
    assert!(err.contains("http."), "{err}");
}

#[tokio::test]
async fn runs_a_collection_and_reports_failures() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let folder = fx
        .node(&col, None, "Checks", json!({ "kind": "folder" }))
        .await;
    let test = "rq.test('is ok', () => rq.response.to.have.status(200));";
    for (name, path) in [("Fine", "/status/200"), ("Broken", "/status/500")] {
        let mut data = request("GET", &server.url(path));
        data["scripts"] = json!({ "preRequest": "", "afterResponse": test });
        fx.node(&col, Some(&folder), name, data).await;
    }
    fx.node(
        &col,
        None,
        "Outside",
        request("GET", &server.url("/status/200")),
    )
    .await;

    let out = fx
        .tool(
            "run_collection",
            json!({ "collectionId": col["id"], "folderId": folder["id"], "iterations": 2 }),
        )
        .await;
    assert_eq!(out["passed"], false);
    assert_eq!(out["iterations"], 2);
    assert_eq!(out["summary"]["requests"], 4);
    assert_eq!(out["summary"]["failed"], 2);
    assert_eq!(out["summary"]["testsPassed"], 2);
    assert_eq!(out["summary"]["testsFailed"], 2);
    let items = out["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let broken = items.iter().find(|i| i["name"] == "Broken").unwrap();
    assert_eq!(broken["status"], 500);
    assert_eq!(broken["passed"], false);
    assert_eq!(broken["path"], "Checks/Broken");
    assert!(broken["responseId"].is_string());

    let out = fx
        .tool(
            "run_collection",
            json!({ "collectionId": col["id"], "requestIds": [items[0]["requestId"]] }),
        )
        .await;
    assert_eq!(out["passed"], true);
    assert_eq!(out["summary"]["requests"], 1);
}

#[tokio::test]
async fn imports_text_into_a_folder_or_a_new_collection() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    let folder = fx
        .node(&col, None, "Imported", json!({ "kind": "folder" }))
        .await;
    let out = fx
        .tool(
            "import_text",
            json!({
                "collectionId": col["id"],
                "folderId": folder["id"],
                "text": "curl -X POST https://api.example.com/users -H 'Content-Type: application/json' -d '{\"a\":1}'"
            }),
        )
        .await;
    assert_eq!(out["requests"], 1);
    assert_eq!(out["format"], "curl");
    let tree = fx
        .tool("get_tree", json!({ "collectionId": col["id"] }))
        .await;
    let imported = tree["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "request")
        .unwrap();
    assert_eq!(imported["parentId"], folder["id"]);
    assert_eq!(imported["method"], "POST");

    let ws = fx.workspace().await;
    let out = fx
        .tool(
            "import_text",
            json!({
                "workspaceId": ws["id"],
                "text": "### Health\nGET https://api.example.com/health\n\n### Login\nPOST https://api.example.com/login\n"
            }),
        )
        .await;
    assert_eq!(out["requests"], 2);
    assert_eq!(out["format"], "http");
    assert_eq!(out["collectionIds"].as_array().unwrap().len(), 1);

    let err = fx.tool_err("import_text", json!({ "text": "x" })).await;
    assert!(err.contains("mcp.invalid_arguments"), "{err}");
}

#[tokio::test]
async fn exports_a_collection_without_secrets() {
    let fx = Fx::new().await;
    let col = fx.collection("API").await;
    env(
        &fx,
        &col,
        "Prod",
        json!({ "token": "tok-s3cret-1" }),
        &["token"],
    )
    .await;
    fx.node(&col, None, "List users", request("GET", "https://x/users"))
        .await;
    let out = fx
        .tool("export_collection", json!({ "collectionId": col["id"] }))
        .await;
    let file = &out["files"][0];
    assert!(file["name"].as_str().unwrap().ends_with(".json"), "{file}");
    let content = file["content"].as_str().unwrap();
    assert!(content.contains("List users"));
    assert!(!content.contains("tok-s3cret-1"));

    let out = fx
        .tool(
            "export_collection",
            json!({ "collectionId": col["id"], "format": "curl" }),
        )
        .await;
    assert!(
        out["files"][0]["content"]
            .as_str()
            .unwrap()
            .contains("curl")
    );
}

#[test]
fn every_catalog_tool_is_implemented_in_catalog_order() {
    use reqly_api::commands::mcp::CATALOG;
    let implemented: Vec<_> = reqly_mcp::catalog_tools()
        .iter()
        .map(|t| (t.name, t.group))
        .collect();
    assert_eq!(implemented, CATALOG.to_vec());
}
