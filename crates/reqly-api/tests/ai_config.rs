//! AI settings in the core (ADR-015, TASKS T14.3).

use reqly_api::access::Caller;
use reqly_api::commands::mcp::CATALOG;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_storage::users::UserRepo;
use reqly_testserver::FakeLlm;
use serde_json::{Value, json};

async fn call_as(ctx: &AppContext, caller: Caller, name: &str, input: Value) -> Value {
    dispatch_as(&registry(), ctx, caller, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    call_as(ctx, Caller::Local, name, input).await
}

async fn call_err(ctx: &AppContext, caller: Caller, name: &str, input: Value) -> String {
    dispatch_as(&registry(), ctx, caller, name, input)
        .await
        .unwrap_err()
        .code
}

fn provider(id: &str, preset: &str, base_url: &str) -> Value {
    json!({ "id": id, "name": format!("P {id}"), "preset": preset, "baseUrl": base_url, "headers": [] })
}

#[tokio::test]
async fn off_by_default_with_presets_and_all_tools() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let status = call(&ctx, "ai.config", json!({})).await;
    assert_eq!(status["config"]["enabled"], false);
    assert_eq!(status["config"]["providers"], json!([]));
    assert!(status["config"]["defaultModel"].is_null());
    assert_eq!(status["canEdit"], true);

    let presets: Vec<_> = status["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["preset"].as_str().unwrap().to_string(),
                p["baseUrl"].as_str().unwrap().to_string(),
                p["keyRequired"].as_bool().unwrap(),
            )
        })
        .collect();
    let expect = [
        ("openai", "https://api.openai.com/v1", true),
        ("openrouter", "https://openrouter.ai/api/v1", true),
        ("ollamaCloud", "https://ollama.com/v1", true),
        ("ollama", "http://localhost:11434/v1", false),
        ("custom", "", false),
    ];
    assert_eq!(
        presets,
        expect
            .iter()
            .map(|(p, u, k)| (p.to_string(), u.to_string(), *k))
            .collect::<Vec<_>>()
    );

    // Every tool is on; only reading tools run without asking.
    let tools = status["tools"].as_array().unwrap();
    assert_eq!(tools.len(), CATALOG.len());
    assert!(tools.iter().all(|t| t["enabled"] == true));
    let ask = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["needsConfirmation"]
            .as_bool()
            .unwrap()
    };
    assert!(!ask("get_tree"));
    assert!(!ask("render_request"));
    assert!(!ask("export_collection"));
    assert!(ask("send_request"));
    assert!(ask("delete_item"));
    assert!(ask("set_variables"));
}

#[tokio::test]
async fn saves_providers_and_keeps_keys_out_of_every_answer() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let mut events = ctx.events.subscribe();
    let status = call(
        &ctx,
        "ai.setConfig",
        json!({
            "enabled": true,
            "providers": [
                provider("p1", "openai", "https://api.openai.com/v1"),
                provider("p2", "ollama", "http://localhost:11434/v1"),
            ],
            "defaultModel": { "providerId": "p1", "model": "gpt-4.1-mini" },
            "tools": { "delete_item": false },
            "autoApprove": { "run": true },
        }),
    )
    .await;
    assert_eq!(status["config"]["enabled"], true);
    assert_eq!(status["providers"][0]["hasKey"], false);
    assert_eq!(status["config"]["autoApprove"]["run"], true);
    let event = events.recv().await.unwrap();
    assert_eq!(event.topic, "ai.configChanged");

    call(
        &ctx,
        "ai.setKey",
        json!({ "providerId": "p1", "key": "sk-secret-123" }),
    )
    .await;
    let status = call(&ctx, "ai.config", json!({})).await;
    assert_eq!(status["providers"][0]["hasKey"], true);
    assert_eq!(status["providers"][1]["hasKey"], false);
    let send_tool = status["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "send_request")
        .unwrap()
        .clone();
    assert_eq!(
        send_tool["needsConfirmation"], false,
        "run is auto-approved"
    );
    let delete = status["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "delete_item")
        .unwrap()
        .clone();
    assert_eq!(delete["enabled"], false);
    assert!(!status.to_string().contains("sk-secret-123"));

    // Clearing.
    call(
        &ctx,
        "ai.setKey",
        json!({ "providerId": "p1", "key": null }),
    )
    .await;
    let status = call(&ctx, "ai.config", json!({})).await;
    assert_eq!(status["providers"][0]["hasKey"], false);
}

#[tokio::test]
async fn removing_a_provider_drops_its_key() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let config = |providers: Value| json!({ "enabled": true, "providers": providers });
    call(
        &ctx,
        "ai.setConfig",
        config(json!([provider(
            "p1",
            "openai",
            "https://api.openai.com/v1"
        )])),
    )
    .await;
    call(
        &ctx,
        "ai.setKey",
        json!({ "providerId": "p1", "key": "sk-1" }),
    )
    .await;
    call(&ctx, "ai.setConfig", config(json!([]))).await;
    call(
        &ctx,
        "ai.setConfig",
        config(json!([provider(
            "p1",
            "openai",
            "https://api.openai.com/v1"
        )])),
    )
    .await;
    let status = call(&ctx, "ai.config", json!({})).await;
    assert_eq!(status["providers"][0]["hasKey"], false);
}

#[tokio::test]
async fn rejects_invalid_configs() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let bad = [
        json!({ "providers": [provider("p1", "custom", "ftp://x")] }),
        json!({ "providers": [provider("p1", "custom", "not a url")] }),
        json!({ "providers": [provider("", "openai", "https://api.openai.com/v1")] }),
        json!({ "providers": [
            provider("p1", "openai", "https://api.openai.com/v1"),
            provider("p1", "ollama", "http://localhost:11434/v1")
        ] }),
        json!({ "providers": [{ "id": "p1", "name": " ", "preset": "openai", "baseUrl": "https://api.openai.com/v1", "headers": [] }] }),
        json!({ "defaultModel": { "providerId": "nope", "model": "m" } }),
        json!({
            "providers": [provider("p1", "openai", "https://api.openai.com/v1")],
            "defaultModel": { "providerId": "p1", "model": " " }
        }),
        json!({ "tools": { "rm_rf": true } }),
    ];
    for input in bad {
        let code = call_err(&ctx, Caller::Local, "ai.setConfig", input.clone()).await;
        assert_eq!(code, "ai.invalid_config", "{input}");
    }
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.setKey",
        json!({ "providerId": "ghost", "key": "k" }),
    )
    .await;
    assert_eq!(code, "ai.unknown_provider");
}

#[tokio::test]
async fn generic_settings_commands_cannot_touch_ai_keys() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    for key in ["ai", "ai.keys"] {
        let code = call_err(&ctx, Caller::Local, "settings.get", json!({ "key": key })).await;
        assert_eq!(code, "settings.reserved", "{key}");
        let code = call_err(
            &ctx,
            Caller::Local,
            "settings.set",
            json!({ "key": key, "value": {} }),
        )
        .await;
        assert_eq!(code, "settings.reserved", "{key}");
    }
    call(&ctx, "settings.set", json!({ "key": "aim", "value": 1 })).await;
}

#[tokio::test]
async fn lists_models_of_a_provider() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    llm.require_key("sk-good");
    llm.set_models(&["m-2", "m-1"]);
    call(
        &ctx,
        "ai.setConfig",
        json!({ "enabled": true, "providers": [provider("p1", "custom", &llm.base_url())] }),
    )
    .await;
    // No key yet: the provider refuses.
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.models",
        json!({ "providerId": "p1" }),
    )
    .await;
    assert_eq!(code, "ai.auth");
    call(
        &ctx,
        "ai.setKey",
        json!({ "providerId": "p1", "key": "sk-good" }),
    )
    .await;
    let models = call(&ctx, "ai.models", json!({ "providerId": "p1" })).await;
    assert_eq!(models, json!(["m-1", "m-2"]));
}

#[tokio::test]
async fn openrouter_identifies_the_app() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    call(
        &ctx,
        "ai.setConfig",
        json!({ "enabled": true, "providers": [{
            "id": "or", "name": "OpenRouter", "preset": "openrouter", "baseUrl": llm.base_url(),
            "headers": [{ "name": "X-Extra", "value": "1" }]
        }] }),
    )
    .await;
    call(&ctx, "ai.models", json!({ "providerId": "or" })).await;
    let req = &llm.requests()[0];
    assert_eq!(req.header("x-title"), Some("Reqly"));
    assert!(req.header("http-referer").is_some());
    assert_eq!(req.header("x-extra"), Some("1"));
}

#[tokio::test]
async fn disabled_ai_lists_no_models() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    call(
        &ctx,
        "ai.setConfig",
        json!({ "enabled": false, "providers": [provider("p1", "ollama", "http://localhost:11434/v1")] }),
    )
    .await;
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.models",
        json!({ "providerId": "p1" }),
    )
    .await;
    assert_eq!(code, "ai.disabled");
}

#[tokio::test]
async fn users_see_providers_and_only_administrators_change_them() {
    let ctx = AppContext::for_tests_on(Platform::Server).await;
    let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let bob = users.create("bob", "", "password1", false).await.unwrap();
    let root = users.create("root", "", "password1", true).await.unwrap();
    let bob = Caller::User {
        id: bob.id,
        admin: false,
    };
    let root = Caller::User {
        id: root.id,
        admin: true,
    };
    call_as(
        &ctx,
        root,
        "ai.setConfig",
        json!({ "enabled": true, "providers": [{
            "id": "p1", "name": "Corp", "preset": "custom", "baseUrl": "https://llm.corp/v1",
            "headers": [{ "name": "X-Api-Key", "value": "corp-secret" }]
        }] }),
    )
    .await;

    let seen = call_as(&ctx, bob, "ai.config", json!({})).await;
    assert_eq!(seen["canEdit"], false);
    assert_eq!(seen["config"]["providers"][0]["name"], "Corp");
    assert!(!seen.to_string().contains("corp-secret"), "{seen}");
    let admin_view = call_as(&ctx, root, "ai.config", json!({})).await;
    assert_eq!(admin_view["canEdit"], true);
    assert!(admin_view.to_string().contains("corp-secret"));

    for (name, input) in [
        ("ai.setConfig", json!({ "enabled": false })),
        ("ai.setKey", json!({ "providerId": "p1", "key": "k" })),
    ] {
        assert_eq!(
            call_err(&ctx, bob, name, input).await,
            "access.denied",
            "{name}"
        );
    }
}
