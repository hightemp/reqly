//! MCP settings, token and call log in the core (ADR-014, TASKS T13.2).

use reqly_api::access::Caller;
use reqly_api::commands::mcp::{
    CATALOG, DEFAULT_PORT, McpCall, McpGroup, enabled_tools, load_config, token_matches,
};
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_storage::users::UserRepo;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    dispatch_as(&registry(), ctx, Caller::Local, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

async fn call_err(ctx: &AppContext, caller: Caller, name: &str, input: Value) -> String {
    dispatch_as(&registry(), ctx, caller, name, input)
        .await
        .unwrap_err()
        .code
}

#[test]
fn catalog_names_are_unique_and_grouped() {
    let mut names: Vec<_> = CATALOG.iter().map(|(n, _)| *n).collect();
    let count = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), count);
    assert_eq!(count, 25);
    for group in [
        McpGroup::Read,
        McpGroup::Edit,
        McpGroup::Run,
        McpGroup::Environments,
        McpGroup::Interop,
    ] {
        assert!(CATALOG.iter().any(|(_, g)| *g == group), "{group:?}");
    }
}

#[tokio::test]
async fn off_by_default_with_read_and_run_tools_preselected() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let status = call(&ctx, "mcp.config", json!({})).await;
    assert_eq!(status["available"], true);
    assert_eq!(status["config"]["enabled"], false);
    assert_eq!(status["config"]["port"], DEFAULT_PORT);
    assert_eq!(status["hasToken"], false);
    let tools = status["tools"].as_array().unwrap();
    assert_eq!(tools.len(), CATALOG.len());
    for t in tools {
        let on = matches!(t["group"].as_str().unwrap(), "read" | "run");
        assert_eq!(t["enabled"], on, "{t}");
    }
    let config = load_config(&ctx.storage).await.unwrap();
    assert!(!config.enabled);
    let enabled = enabled_tools(&config);
    assert!(enabled.contains("get_tree") && enabled.contains("send_request"));
    assert!(!enabled.contains("delete_item"));
}

#[tokio::test]
async fn first_enable_creates_the_token_once() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let mut events = ctx.events.subscribe();
    let status = call(
        &ctx,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7700, "tools": { "delete_item": true, "get_tree": false } }),
    )
    .await;
    let token = status["token"].as_str().unwrap().to_string();
    assert!(token.len() >= 64, "{token}");
    assert_eq!(status["hasToken"], true);
    assert_eq!(status["config"]["port"], 7700);
    assert!(token_matches(&ctx.storage, &token).await.unwrap());
    assert!(!token_matches(&ctx.storage, "wrong").await.unwrap());

    let config = load_config(&ctx.storage).await.unwrap();
    let enabled = enabled_tools(&config);
    assert!(enabled.contains("delete_item"));
    assert!(!enabled.contains("get_tree"));
    assert!(
        enabled.contains("send_request"),
        "unlisted tools keep their default"
    );

    let event = events.recv().await.unwrap();
    assert_eq!(event.topic, "mcp.configChanged");
    assert_eq!(event.payload["enabled"], true);

    // Saving again keeps the token and does not show it.
    let again = call(
        &ctx,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7700, "tools": {} }),
    )
    .await;
    assert_eq!(again["token"], Value::Null);
    assert!(token_matches(&ctx.storage, &token).await.unwrap());

    // A new token replaces the old one.
    let fresh = call(&ctx, "mcp.regenerateToken", json!({})).await;
    let fresh = fresh["token"].as_str().unwrap();
    assert_ne!(fresh, token);
    assert!(token_matches(&ctx.storage, fresh).await.unwrap());
    assert!(!token_matches(&ctx.storage, &token).await.unwrap());
}

#[tokio::test]
async fn rejects_bad_ports_and_unknown_tools() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    for input in [
        json!({ "enabled": true, "port": 80, "tools": {} }),
        json!({ "enabled": true, "port": 7655, "tools": { "rm_rf": true } }),
    ] {
        let code = call_err(&ctx, Caller::Local, "mcp.setConfig", input).await;
        assert_eq!(code, "mcp.invalid_config");
    }
}

#[tokio::test]
async fn generic_settings_commands_cannot_touch_mcp_keys() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    call(
        &ctx,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7655, "tools": {} }),
    )
    .await;
    for key in ["mcp", "mcp.token"] {
        let code = call_err(&ctx, Caller::Local, "settings.get", json!({ "key": key })).await;
        assert_eq!(code, "settings.reserved", "{key}");
        let code = call_err(
            &ctx,
            Caller::Local,
            "settings.set",
            json!({ "key": key, "value": { "enabled": true } }),
        )
        .await;
        assert_eq!(code, "settings.reserved", "{key}");
    }
    // Keys that only start alike are ordinary settings.
    call(&ctx, "settings.set", json!({ "key": "mcpish", "value": 1 })).await;
}

#[tokio::test]
async fn not_available_on_android() {
    let ctx = AppContext::for_tests_on(Platform::Mobile).await;
    let status = call(&ctx, "mcp.config", json!({})).await;
    assert_eq!(status["available"], false);
    let code = call_err(
        &ctx,
        Caller::Local,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7655, "tools": {} }),
    )
    .await;
    assert_eq!(code, "mcp.unavailable");
}

#[tokio::test]
async fn only_administrators_manage_mcp_on_a_multi_user_server() {
    let ctx = AppContext::for_tests_on(Platform::Server).await;
    let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let user = users.create("bob", "", "password1", false).await.unwrap();
    let admin = users.create("root", "", "password1", true).await.unwrap();
    let bob = Caller::User {
        id: user.id,
        admin: false,
    };
    for name in [
        "mcp.config",
        "mcp.setConfig",
        "mcp.regenerateToken",
        "mcp.log",
    ] {
        let input = if name == "mcp.setConfig" {
            json!({ "enabled": true, "port": 7655, "tools": {} })
        } else {
            json!({})
        };
        assert_eq!(
            call_err(&ctx, bob, name, input).await,
            "access.denied",
            "{name}"
        );
    }
    let root = Caller::User {
        id: admin.id,
        admin: true,
    };
    let status = dispatch_as(&registry(), &ctx, root, "mcp.config", json!({}))
        .await
        .unwrap();
    assert_eq!(status["config"]["enabled"], false);
}

#[tokio::test]
async fn keeps_the_last_calls_and_announces_each() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let mut events = ctx.events.subscribe();
    for i in 0..205 {
        ctx.mcp_log.record(
            &ctx.events,
            McpCall {
                at: i,
                tool: format!("tool_{i}"),
                arguments: "{}".into(),
                ok: i % 2 == 0,
                error: (i % 2 == 1).then(|| "boom".to_string()),
                duration_ms: 3,
                user: None,
            },
        );
    }
    let event = events.recv().await.unwrap();
    assert_eq!(event.topic, "mcp.call");
    assert_eq!(event.payload["tool"], "tool_0");

    let log = call(&ctx, "mcp.log", json!({})).await;
    let log = log.as_array().unwrap();
    assert_eq!(log.len(), 200);
    // Newest first.
    assert_eq!(log[0]["tool"], "tool_204");
    assert_eq!(log[199]["tool"], "tool_5");
    assert_eq!(log[0]["durationMs"], 3);
}
