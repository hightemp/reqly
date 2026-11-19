//! The agent tool catalog in the core (ADR-015, TASKS T14.1): the AI chat
//! runs tools from inside a command, with only a borrowed context.

use reqly_api::access::Caller;
use reqly_api::agent::{ToolContext, catalog_tools, registry};
use reqly_api::commands::mcp::CATALOG;
use reqly_api::commands::{AppContext, Platform};
use serde_json::json;

#[test]
fn catalog_tools_follow_the_catalog() {
    let tools = catalog_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name).collect();
    let catalog: Vec<_> = CATALOG.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, catalog);
    for (name, group) in CATALOG {
        assert_eq!(tools.get(name).unwrap().group, *group, "{name}");
    }
}

#[tokio::test]
async fn tools_run_with_a_borrowed_context_and_the_shared_registry() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    reqly_api::commands::dispatch_as(
        registry(),
        &ctx,
        Caller::Local,
        "workspace.create",
        json!({ "name": "Team" }),
    )
    .await
    .unwrap();
    let tools = catalog_tools();
    let cx = ToolContext::new(&ctx, Caller::Local);
    let out = tools
        .get("list_workspaces")
        .unwrap()
        .run(cx, json!({}))
        .await
        .unwrap();
    let names: Vec<_> = out["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Team"), "{out}");
    // The registry is built once.
    assert!(std::ptr::eq(registry(), registry()));
}

#[tokio::test]
async fn tool_errors_carry_codes() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let tools = catalog_tools();
    let err = tools
        .get("get_request")
        .unwrap()
        .run(ToolContext::new(&ctx, Caller::Local), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code, "mcp.invalid_arguments");
}
