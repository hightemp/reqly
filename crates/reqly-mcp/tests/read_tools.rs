//! Read tools (TASKS T13.3).

mod common;

use common::Fx;
use reqly_testserver::TestServer;
use serde_json::{Value, json};

const HIDDEN: &str = "••••";

struct Shop {
    fx: Fx,
    col: Value,
    folder: Value,
    list: Value,
    health: Value,
}

async fn shop() -> Shop {
    let fx = Fx::new().await;
    let col = fx.collection("Shop").await;
    let folder = fx
        .node(&col, None, "Orders", json!({ "kind": "folder" }))
        .await;
    let list = fx
        .node(
            &col,
            Some(&folder),
            "List orders",
            json!({
                "kind": "request",
                "method": "GET",
                "url": "{{base_url}}/orders",
                "auth": { "type": "bearer", "token": "abc123", "prefix": "Bearer" },
                "headers": [
                    { "name": "X-Api-Key", "value": "k-999" },
                    { "name": "X-Trace", "value": "{{trace}}" },
                    { "name": "Accept", "value": "application/json" }
                ]
            }),
        )
        .await;
    let health = fx
        .node(
            &col,
            None,
            "Health",
            json!({ "kind": "request", "method": "HEAD", "url": "https://example.com/health" }),
        )
        .await;
    Shop {
        fx,
        col,
        folder,
        list,
        health,
    }
}

#[tokio::test]
async fn lists_workspaces_and_collections() {
    let s = shop().await;
    let ws = s.fx.workspace().await;
    let out = s.fx.tool("list_workspaces", json!({})).await;
    assert_eq!(
        out["workspaces"],
        json!([{ "id": ws["id"], "name": ws["name"] }])
    );

    let out = s.fx.tool("list_collections", json!({})).await;
    let cols = out["collections"].as_array().unwrap();
    let shop = cols.iter().find(|c| c["name"] == "Shop").unwrap();
    assert_eq!(shop["id"], s.col["id"]);
    assert_eq!(shop["workspaceId"], ws["id"]);

    let out =
        s.fx.tool("list_collections", json!({ "workspaceId": ws["id"] }))
            .await;
    assert!(
        out["collections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "Shop")
    );
}

#[tokio::test]
async fn tree_is_in_display_order_with_paths() {
    let s = shop().await;
    let out =
        s.fx.tool("get_tree", json!({ "collectionId": s.col["id"] }))
            .await;
    assert_eq!(out["collection"]["name"], "Shop");
    let items = out["items"].as_array().unwrap();
    let rows: Vec<_> = items
        .iter()
        .map(|i| {
            (
                i["name"].as_str().unwrap(),
                i["path"].as_str().unwrap(),
                i["depth"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Orders", "Orders", 0),
            ("List orders", "Orders/List orders", 1),
            ("Health", "Health", 0),
        ]
    );
    let list = &items[1];
    assert_eq!(list["id"], s.list["id"]);
    assert_eq!(list["parentId"], s.folder["id"]);
    assert_eq!(list["kind"], "request");
    assert_eq!(list["method"], "GET");
    assert_eq!(list["url"], "{{base_url}}/orders");
    assert_eq!(items[0]["kind"], "folder");
}

#[tokio::test]
async fn request_secrets_are_masked_but_templates_shown() {
    let s = shop().await;
    let out =
        s.fx.tool("get_request", json!({ "id": s.list["id"] }))
            .await;
    assert_eq!(out["id"], s.list["id"]);
    assert_eq!(out["name"], "List orders");
    assert_eq!(out["path"], "Orders/List orders");
    assert_eq!(out["collectionId"], s.col["id"]);
    let data = &out["data"];
    assert_eq!(data["kind"], "request");
    assert_eq!(data["url"], "{{base_url}}/orders");
    assert_eq!(data["auth"]["type"], "bearer");
    assert_eq!(data["auth"]["token"], HIDDEN);
    let headers = data["headers"].as_array().unwrap();
    assert_eq!(headers[0]["value"], HIDDEN);
    assert_eq!(headers[1]["value"], "{{trace}}");
    assert_eq!(headers[2]["value"], "application/json");

    let err = s.fx.tool_err("get_request", json!({ "id": "nope" })).await;
    assert!(err.contains("invalid"), "{err}");
    let err = s.fx.tool_err("get_request", json!({})).await;
    assert!(err.contains("mcp.invalid_arguments"), "{err}");
}

#[tokio::test]
async fn search_finds_requests_across_workspaces() {
    let s = shop().await;
    let out =
        s.fx.tool("search_requests", json!({ "query": "orders" }))
            .await;
    let hits = out["results"].as_array().unwrap();
    assert!(hits.iter().any(|h| h["id"] == s.list["id"]), "{out}");
    let hit = hits.iter().find(|h| h["id"] == s.list["id"]).unwrap();
    assert_eq!(hit["collectionId"], s.col["id"]);
    assert_eq!(hit["method"], "GET");
    let out =
        s.fx.tool("search_requests", json!({ "query": "zzzz" }))
            .await;
    assert_eq!(out["results"], json!([]));
}

#[tokio::test]
async fn environments_list_names_and_the_active_one() {
    let s = shop().await;
    let ws = s.fx.workspace().await;
    let env =
        s.fx.cmd(
            "environment.create",
            json!({
                "workspaceId": ws["id"], "collectionId": s.col["id"], "kind": "sub",
                "name": "Staging", "data": { "base_url": "https://staging", "token": "t-1" }
            }),
        )
        .await;
    s.fx.cmd(
        "environment.setActive",
        json!({ "collectionId": s.col["id"], "environmentId": env["id"] }),
    )
    .await;
    let out =
        s.fx.tool("list_environments", json!({ "collectionId": s.col["id"] }))
            .await;
    assert_eq!(out["activeId"], env["id"]);
    let envs = out["environments"].as_array().unwrap();
    let staging = envs.iter().find(|e| e["name"] == "Staging").unwrap();
    assert_eq!(staging["id"], env["id"]);
    assert_eq!(staging["kind"], "sub");
    assert_eq!(staging["variables"], json!(["base_url", "token"]));
    assert!(
        staging.get("data").is_none(),
        "values come from get_variables"
    );
}

#[tokio::test]
async fn history_and_responses() {
    let server = TestServer::start().await;
    let s = shop().await;
    let echo =
        s.fx.node(
            &s.col,
            None,
            "Echo",
            json!({ "kind": "request", "method": "POST", "url": server.url("/echo"),
                    "body": { "kind": "json", "text": "{\"hello\":\"world\"}" } }),
        )
        .await;
    let sent =
        s.fx.cmd("request.send", json!({ "requestId": echo["id"] }))
            .await;

    let out =
        s.fx.tool("get_history", json!({ "requestId": echo["id"] }))
            .await;
    let rows = out["responses"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], sent["id"]);
    assert_eq!(rows[0]["status"], 200);

    let ws = s.fx.workspace().await;
    let out =
        s.fx.tool(
            "get_history",
            json!({ "workspaceId": ws["id"], "limit": 5 }),
        )
        .await;
    assert_eq!(out["responses"].as_array().unwrap().len(), 1);
    let err = s.fx.tool_err("get_history", json!({})).await;
    assert!(err.contains("mcp.invalid_arguments"), "{err}");

    let out = s.fx.tool("get_response", json!({ "id": sent["id"] })).await;
    assert_eq!(out["status"], 200);
    assert_eq!(out["request"]["method"], "POST");
    assert!(out["body"].as_str().unwrap().contains("world"), "{out}");
    assert_eq!(out["bodyTruncated"], false);
    assert!(
        out["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0].as_str().unwrap().eq_ignore_ascii_case("content-type"))
    );

    let out =
        s.fx.tool(
            "get_response",
            json!({ "id": sent["id"], "maxBodyBytes": 10 }),
        )
        .await;
    assert_eq!(out["body"].as_str().unwrap().len(), 10);
    assert_eq!(out["bodyTruncated"], true);
    assert!(out["bodySize"].as_u64().unwrap() > 10);
}

#[tokio::test]
async fn binary_bodies_are_base64() {
    let server = TestServer::start().await;
    let s = shop().await;
    let bytes =
        s.fx.node(
            &s.col,
            None,
            "Bytes",
            json!({ "kind": "request", "method": "GET", "url": server.url("/bytes/300") }),
        )
        .await;
    let sent =
        s.fx.cmd("request.send", json!({ "requestId": bytes["id"] }))
            .await;
    let out = s.fx.tool("get_response", json!({ "id": sent["id"] })).await;
    assert_eq!(out["bodyEncoding"], "base64");
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(out["body"].as_str().unwrap())
        .unwrap();
    let expected: Vec<u8> = (0..300).map(|i| (i % 251) as u8).collect();
    assert_eq!(decoded, expected);
    let _ = s.health;
}

#[tokio::test]
async fn implemented_tools_follow_the_catalog() {
    use reqly_api::commands::mcp::CATALOG;
    for tool in reqly_mcp::catalog_tools().iter() {
        let entry = CATALOG.iter().find(|(n, _)| *n == tool.name);
        assert_eq!(entry.map(|(_, g)| *g), Some(tool.group), "{}", tool.name);
        assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
        assert!(tool.description.len() > 20, "{}", tool.name);
    }
}

#[tokio::test]
async fn docs_list_topics_and_return_pages() {
    let fx = Fx::new().await;
    let topics = fx.tool("get_docs", json!({})).await;
    let list = topics["topics"].as_array().unwrap();
    let ids: Vec<&str> = list.iter().map(|t| t["id"].as_str().unwrap()).collect();
    for id in ["variables", "scripts", "script-api", "auth", "mcp"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    assert!(
        list.iter()
            .all(|t| !t["title"].as_str().unwrap().is_empty())
    );

    // Template tags and response chaining live on the variables page.
    let page = fx.tool("get_docs", json!({ "topic": "variables" })).await;
    assert_eq!(page["topic"], "variables");
    assert_eq!(page["title"], "Variables and environments");
    let text = page["markdown"].as_str().unwrap();
    assert!(text.contains("{% response"), "{text}");

    let err = fx.tool_err("get_docs", json!({ "topic": "nope" })).await;
    assert!(err.contains("variables"), "{err}");
}
