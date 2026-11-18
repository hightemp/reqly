//! Who may use the endpoint, decided by the MCP settings (TASKS T13.2).

use reqly_api::access::Caller;
use reqly_api::commands::mcp::{enabled_tools, load_config};
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_mcp::{Allowed, Denied, Gate, SettingsGate, TokenKind};
use reqly_storage::users::UserRepo;
use serde_json::{Value, json};
use std::sync::Arc;

async fn enable(ctx: &AppContext, tools: Value) -> Option<String> {
    let status = dispatch_as(
        &registry(),
        ctx,
        Caller::Local,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7655, "tools": tools }),
    )
    .await
    .unwrap();
    status["token"].as_str().map(str::to_string)
}

#[tokio::test]
async fn closed_until_enabled() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Desktop).await);
    let gate = SettingsGate::new(ctx.clone(), TokenKind::Mcp);
    assert_eq!(gate.open(Some("x")).await.unwrap_err(), Denied::Disabled);
    assert_eq!(gate.open(None).await.unwrap_err(), Denied::Disabled);
}

#[tokio::test]
async fn mcp_token_opens_with_the_enabled_tools() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Desktop).await);
    let token = enable(&ctx, json!({ "delete_item": true })).await.unwrap();
    let gate = SettingsGate::new(ctx.clone(), TokenKind::Mcp);

    assert_eq!(gate.open(None).await.unwrap_err(), Denied::Unauthorized);
    assert_eq!(
        gate.open(Some("wrong")).await.unwrap_err(),
        Denied::Unauthorized
    );
    let session = gate.open(Some(&token)).await.unwrap();
    assert_eq!(session.caller, Caller::Local);
    let expected = enabled_tools(&load_config(&ctx.storage).await.unwrap());
    assert!(expected.contains("delete_item"));
    assert_eq!(session.allowed, Allowed::Only(expected));

    // Turning MCP off closes it again, token or not.
    dispatch_as(
        &registry(),
        &ctx,
        Caller::Local,
        "mcp.setConfig",
        json!({ "enabled": false, "port": 7655, "tools": {} }),
    )
    .await
    .unwrap();
    assert_eq!(gate.open(Some(&token)).await.unwrap_err(), Denied::Disabled);
}

#[tokio::test]
async fn closed_on_android() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Mobile).await);
    let gate = SettingsGate::new(ctx, TokenKind::Mcp);
    assert_eq!(gate.open(Some("x")).await.unwrap_err(), Denied::Disabled);
}

#[tokio::test]
async fn multi_user_server_takes_personal_api_tokens() {
    let ctx = Arc::new(AppContext::for_tests_on(Platform::Server).await);
    let mcp_token = enable(&ctx, json!({})).await.unwrap();
    let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let bob = users.create("bob", "", "password1", false).await.unwrap();
    let (_, api_token) = users.create_token(bob.id, "agent", None).await.unwrap();
    let gate = SettingsGate::new(ctx.clone(), TokenKind::UserApi);

    let session = gate.open(Some(&api_token)).await.unwrap();
    assert_eq!(
        session.caller,
        Caller::User {
            id: bob.id,
            admin: false
        }
    );
    // The MCP token belongs to the single-user modes.
    assert_eq!(
        gate.open(Some(&mcp_token)).await.unwrap_err(),
        Denied::Unauthorized
    );
}
