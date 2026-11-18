//! The MCP endpoint of the server (ADR-014, TASKS T13.7).

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use reqly_api::access::Caller;
use reqly_api::commands::{dispatch_as, registry};
use reqly_server::{AppState, AuthConfig, AuthMode, router};
use reqly_storage::users::UserRepo;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn enable(state: &AppState) -> String {
    let status = dispatch_as(
        &registry(),
        &state.ctx,
        Caller::Local,
        "mcp.setConfig",
        json!({ "enabled": true, "port": 7655, "tools": { "create_request": true } }),
    )
    .await
    .unwrap();
    status["token"].as_str().unwrap().to_string()
}

async fn mcp(app: &axum::Router, token: Option<&str>, message: Value) -> (StatusCode, Value) {
    let mut req = Request::post("/mcp").header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(message.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn list() -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
}

fn call(name: &str, args: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": name, "arguments": args } })
}

#[tokio::test]
async fn not_found_until_enabled() {
    let state = AppState::for_tests().await;
    let app = router(state);
    let (status, _) = mcp(&app, Some("x"), list()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn single_user_modes_take_the_mcp_token() {
    for mode in [AuthMode::None, AuthMode::Token] {
        let mut state = AppState::for_tests().await;
        state.auth = AuthConfig {
            mode,
            token: Some("server-secret".into()),
            ..AuthConfig::default()
        };
        let token = enable(&state).await;
        let app = router(state.clone());

        let (status, body) = mcp(&app, Some(&token), list()).await;
        assert_eq!(status, StatusCode::OK, "{mode:?}");
        let names: Vec<_> = body["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"get_tree".to_string()));
        assert!(names.contains(&"create_request".to_string()));
        assert!(
            !names.contains(&"delete_item".to_string()),
            "off by default"
        );

        for wrong in [None, Some("server-secret"), Some("nope")] {
            let (status, _) = mcp(&app, wrong, list()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{mode:?} {wrong:?}");
        }

        // The settings show where to connect.
        let status = dispatch_as(
            &registry(),
            &state.ctx,
            Caller::Local,
            "mcp.config",
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(status["listen"]["endpoint"], "/mcp");
    }
}

#[tokio::test]
async fn multi_user_mode_runs_tools_as_the_token_owner() {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Multi,
        ..AuthConfig::default()
    };
    let mcp_token = enable(&state).await;
    let users = UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref());
    let ada = users.create("ada", "", "password1", false).await.unwrap();
    let (_, api_token) = users.create_token(ada.id, "agent", None).await.unwrap();
    let as_ada = Caller::User {
        id: ada.id,
        admin: false,
    };
    let ws = dispatch_as(
        &registry(),
        &state.ctx,
        as_ada,
        "workspace.create",
        json!({ "name": "Ada" }),
    )
    .await
    .unwrap();
    let col = dispatch_as(
        &registry(),
        &state.ctx,
        as_ada,
        "collection.create",
        json!({ "workspaceId": ws["id"], "name": "C" }),
    )
    .await
    .unwrap();
    let app = router(state.clone());

    let (status, _) = mcp(&app, Some(&mcp_token), list()).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the MCP token is for single-user modes"
    );

    let mut events = state.ctx.events.subscribe();
    let (status, body) = mcp(
        &app,
        Some(&api_token),
        call(
            "create_request",
            json!({ "collectionId": col["id"], "name": "From agent" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "{body}");
    let event = events.recv().await.unwrap();
    assert_eq!(event.topic, "tree.changed");
    assert_eq!(
        event.actor,
        Some(ada.id),
        "other clients see who changed it"
    );

    // Only Ada's workspaces exist for her agent.
    let (_, body) = mcp(&app, Some(&api_token), call("list_workspaces", json!({}))).await;
    let names: Vec<_> = body["result"]["structuredContent"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].clone())
        .collect();
    assert_eq!(names, [json!("Ada")]);

    // The call log records the user.
    let log = state.ctx.mcp_log.recent();
    assert_eq!(log[0].user, Some(ada.id));
}

#[tokio::test]
async fn browsers_from_other_sites_are_refused() {
    let state = AppState::for_tests().await;
    let token = enable(&state).await;
    let app = router(state);
    let res = app
        .oneshot(
            Request::post("/mcp")
                .header("content-type", "application/json")
                .header(header::HOST, "reqly.example.com")
                .header(header::ORIGIN, "https://evil.example")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(list().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
