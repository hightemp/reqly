//! The AI chat on a multi-user server (ADR-015, TASKS T14.8): chats and
//! their events belong to one user; the settings reach everyone.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::StreamExt;
use reqly_server::{AppState, AuthConfig, AuthMode, Registration, router};
use reqly_storage::users::UserRepo;
use reqly_testserver::{FakeLlm, LlmReply};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn call(app: &axum::Router, name: &str, input: Value, cookie: &str) -> (StatusCode, Value) {
    let req = Request::post(format!("/api/v1/cmd/{name}"))
        .header("content-type", "application/json")
        .header(header::COOKIE, cookie)
        .body(Body::from(input.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn ok(app: &axum::Router, name: &str, input: Value, cookie: &str) -> Value {
    let (status, body) = call(app, name, input, cookie).await;
    assert_eq!(status, StatusCode::OK, "{name}: {body}");
    body
}

async fn login(app: &axum::Router, username: &str, password: &str) -> String {
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "username": username, "password": password }).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    res.headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

/// Every event topic that arrives until the socket is quiet.
async fn topics(ws: &mut Socket) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(Some(Ok(msg))) =
        tokio::time::timeout(std::time::Duration::from_millis(400), ws.next()).await
    {
        let v: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        out.push(v["event"]["topic"].as_str().unwrap_or_default().to_string());
    }
    out
}

#[tokio::test]
async fn chats_and_their_events_stay_with_their_user() {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Multi,
        registration: Registration::Admin,
        ..AuthConfig::default()
    };
    let users = UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref());
    users
        .create("root", "Root", "root-password", true)
        .await
        .unwrap();
    users
        .create("bob", "Bob", "bob-password", false)
        .await
        .unwrap();
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app.clone()).into_future());
    let llm = FakeLlm::start().await;

    let root = login(&app, "root", "root-password").await;
    let bob = login(&app, "bob", "bob-password").await;
    let ws_of = |cookie: String| async move {
        let mut req = format!("ws://{addr}/api/v1/ws")
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        ws.next().await.unwrap().unwrap(); // hello
        ws
    };
    let mut root_ws = ws_of(root.clone()).await;
    let mut bob_ws = ws_of(bob.clone()).await;

    // Root's workspace, with Bob as a viewer.
    let ws = ok(&app, "workspace.ensureDefault", json!({}), &root).await[0]["id"].clone();
    ok(
        &app,
        "members.set",
        json!({ "workspaceId": ws, "username": "bob", "role": "viewer" }),
        &root,
    )
    .await;

    // Only an administrator sets up AI; everyone hears that it changed.
    let config = json!({
        "enabled": true,
        "providers": [{ "id": "fake", "name": "Fake", "preset": "custom", "baseUrl": llm.base_url(), "headers": [] }],
        "defaultModel": { "providerId": "fake", "model": "fake-small" },
    });
    let (status, _) = call(&app, "ai.setConfig", config.clone(), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    topics(&mut root_ws).await;
    topics(&mut bob_ws).await;
    ok(&app, "ai.setConfig", config, &root).await;
    assert!(
        topics(&mut bob_ws)
            .await
            .contains(&"ai.configChanged".to_string())
    );
    topics(&mut root_ws).await;

    // Bob chats; the stream reaches Bob alone.
    llm.push(LlmReply::Text("Hello Bob".into()));
    let chat = ok(&app, "ai.createChat", json!({ "workspaceId": ws }), &bob).await;
    let chat_id = chat["id"].clone();
    let out = ok(
        &app,
        "ai.send",
        json!({ "chatId": chat_id, "text": "hi" }),
        &bob,
    )
    .await;
    assert_eq!(out["status"], "done");
    let heard = topics(&mut bob_ws).await;
    for topic in ["ai.chatsChanged", "ai.message", "ai.delta", "ai.done"] {
        assert!(heard.contains(&topic.to_string()), "{topic}: {heard:?}");
    }
    let root_heard = topics(&mut root_ws).await;
    assert!(
        root_heard.iter().all(|t| !t.starts_with("ai.")),
        "{root_heard:?}"
    );

    // Root, an administrator, does not see Bob's chat.
    let list = ok(&app, "ai.chats", json!({ "workspaceId": ws }), &root).await;
    assert_eq!(list, json!([]));
    let (status, body) = call(&app, "ai.chat", json!({ "chatId": chat_id }), &root).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Bob, a viewer, cannot change the workspace through tools.
    let (_, cols) = call(&app, "collection.list", json!({ "workspaceId": ws }), &bob).await;
    let col = cols[0]["id"].clone();
    ok(
        &app,
        "ai.setConfig",
        json!({
            "enabled": true,
            "providers": [{ "id": "fake", "name": "Fake", "preset": "custom", "baseUrl": llm.base_url(), "headers": [] }],
            "defaultModel": { "providerId": "fake", "model": "fake-small" },
            "autoApprove": { "edit": true },
        }),
        &root,
    )
    .await;
    llm.push(LlmReply::ToolCalls(vec![(
        "create_folder".into(),
        json!({ "collectionId": col, "name": "Bob's" }),
    )]));
    llm.push(LlmReply::Text("No luck.".into()));
    ok(
        &app,
        "ai.send",
        json!({ "chatId": chat_id, "text": "make a folder" }),
        &bob,
    )
    .await;
    let view = ok(&app, "ai.chat", json!({ "chatId": chat_id }), &bob).await;
    let tool = view["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()
        .clone();
    assert_eq!(tool["error"]["code"], "access.denied", "{tool}");

    root_ws.close(None).await.unwrap();
    bob_ws.close(None).await.unwrap();
}
