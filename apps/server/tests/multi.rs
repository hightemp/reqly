//! Multi-user mode: password logins, registration, API tokens, lockout and
//! per-user command access (ADR-010).

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use reqly_server::{AppState, AuthConfig, AuthMode, Registration, router};
use reqly_storage::users::UserRepo;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn state(registration: Registration) -> AppState {
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Multi,
        registration,
        ..AuthConfig::default()
    };
    UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .create("root", "Root", "root-password", true)
        .await
        .unwrap();
    state
}

struct Res {
    status: StatusCode,
    cookie: Option<String>,
    body: Value,
}

async fn send(app: &axum::Router, req: Request<Body>) -> Res {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    Res {
        status,
        cookie,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn post(path: &str) -> axum::http::request::Builder {
    Request::post(path).header("content-type", "application/json")
}

fn json_post(path: &str, body: Value) -> Request<Body> {
    post(path).body(Body::from(body.to_string())).unwrap()
}

fn cmd(name: &str, input: Value, cookie: &str) -> Request<Body> {
    post(&format!("/api/v1/cmd/{name}"))
        .header(header::COOKIE, cookie)
        .body(Body::from(input.to_string()))
        .unwrap()
}

async fn login(app: &axum::Router, username: &str, password: &str) -> Res {
    send(
        app,
        json_post(
            "/api/v1/auth/login",
            json!({ "username": username, "password": password }),
        ),
    )
    .await
}

async fn session(app: &axum::Router, username: &str, password: &str) -> String {
    let res = login(app, username, password).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    res.cookie.unwrap()
}

#[tokio::test]
async fn password_login_and_status() {
    let app = router(state(Registration::Admin).await);
    let status = send(
        &app,
        Request::get("/api/v1/auth/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status.body,
        json!({ "mode": "multi", "authenticated": false, "registration": "admin" })
    );

    let wrong = login(&app, "root", "nope").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body["code"], "auth.invalid");
    let unknown = login(&app, "ghost", "nope").await;
    assert_eq!(unknown.body["code"], "auth.invalid");

    let cookie = session(&app, "root", "root-password").await;
    let status = send(
        &app,
        Request::get("/api/v1/auth/status")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status.body["authenticated"], true);
    assert_eq!(status.body["user"]["username"], "root");
    assert_eq!(status.body["user"]["isAdmin"], true);

    let me = send(&app, cmd("account.me", json!({}), &cookie)).await;
    assert_eq!(me.body["user"]["username"], "root");

    // The single-user token login does not exist here.
    let res = send(
        &app,
        json_post("/api/v1/auth/login", json!({ "token": "x" })),
    )
    .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn registration_follows_the_policy() {
    let closed = router(state(Registration::Admin).await);
    let body = json!({ "username": "ann", "password": "ann-password" });
    let res = send(&closed, json_post("/api/v1/auth/register", body.clone())).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.body["code"], "auth.registration_closed");

    let open = router(state(Registration::Open).await);
    let res = send(&open, json_post("/api/v1/auth/register", body.clone())).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let cookie = res.cookie.unwrap();
    let me = send(&open, cmd("account.me", json!({}), &cookie)).await;
    assert_eq!(me.body["user"]["username"], "ann");
    assert_eq!(me.body["user"]["isAdmin"], false);

    let again = send(&open, json_post("/api/v1/auth/register", body)).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(again.body["code"], "user.exists");
}

#[tokio::test]
async fn users_see_only_their_workspaces() {
    let state = state(Registration::Open).await;
    let app = router(state);
    let root = session(&app, "root", "root-password").await;
    let res = send(
        &app,
        json_post(
            "/api/v1/auth/register",
            json!({ "username": "bob", "password": "bob-password" }),
        ),
    )
    .await;
    let bob = res.cookie.unwrap();

    let root_ws = send(&app, cmd("workspace.ensureDefault", json!({}), &root)).await;
    let bob_ws = send(&app, cmd("workspace.ensureDefault", json!({}), &bob)).await;
    assert_ne!(root_ws.body[0]["id"], bob_ws.body[0]["id"]);

    let root_col = send(
        &app,
        cmd(
            "collection.list",
            json!({ "workspaceId": root_ws.body[0]["id"] }),
            &root,
        ),
    )
    .await;
    let col = root_col.body[0]["id"].clone();
    let res = send(&app, cmd("tree.list", json!({ "collectionId": col }), &bob)).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    let res = send(&app, cmd("admin.users", json!({}), &bob)).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.body["code"], "access.denied");
    let res = send(&app, cmd("admin.users", json!({}), &root)).await;
    assert_eq!(res.body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn api_tokens_authorize_as_their_user() {
    let app = router(state(Registration::Admin).await);
    let cookie = session(&app, "root", "root-password").await;
    let created = send(
        &app,
        cmd("account.createToken", json!({ "name": "cli" }), &cookie),
    )
    .await;
    let secret = created.body["secret"].as_str().unwrap().to_string();
    let me = |token: String| {
        Request::post("/api/v1/cmd/account.me")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from("{}"))
            .unwrap()
    };
    let res = send(&app, me(secret.clone())).await;
    assert_eq!(res.body["user"]["username"], "root");
    let res = send(&app, me("rq_wrong".into())).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);

    send(
        &app,
        cmd(
            "account.deleteToken",
            json!({ "tokenId": created.body["token"]["id"] }),
            &cookie,
        ),
    )
    .await;
    assert_eq!(
        send(&app, me(secret)).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn accounts_lock_and_disabled_users_lose_access() {
    let state = state(Registration::Admin).await;
    UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref())
        .create("ann", "", "ann-password", false)
        .await
        .unwrap();
    let app = router(state);
    let ann = session(&app, "ann", "ann-password").await;
    let root = session(&app, "root", "root-password").await;

    for _ in 0..reqly_storage::users::MAX_FAILED_LOGINS {
        login(&app, "ann", "wrong").await;
    }
    let locked = login(&app, "ann", "ann-password").await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(locked.body["code"], "auth.locked");

    let users = send(&app, cmd("admin.users", json!({}), &root)).await;
    let ann_id = users.body.as_array().unwrap()[0]["id"].clone();
    send(
        &app,
        cmd(
            "admin.updateUser",
            json!({ "userId": ann_id, "disabled": true }),
            &root,
        ),
    )
    .await;
    let res = send(&app, cmd("account.me", json!({}), &ann)).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_ends_the_session() {
    let app = router(state(Registration::Admin).await);
    let cookie = session(&app, "root", "root-password").await;
    send(
        &app,
        post("/api/v1/auth/logout")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let res = send(&app, cmd("account.me", json!({}), &cookie)).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bootstraps_the_first_admin() {
    let dir = tempfile::tempdir().unwrap();
    let env = |k: &str| match k {
        "REQLY_DATA_DIR" => Some(dir.path().display().to_string()),
        "REQLY_AUTH_MODE" => Some("multi".into()),
        "REQLY_REGISTRATION" => Some("open".into()),
        "REQLY_ADMIN_USERNAME" => Some("boss".into()),
        "REQLY_ADMIN_PASSWORD" => Some("boss-password".into()),
        _ => None,
    };
    let config = reqly_server::Config::load(None, env).unwrap();
    assert_eq!(config.auth.registration, Registration::Open);
    let state = AppState::from_config(&config).await.unwrap();
    let users = UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref());
    let all = users.list().await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].is_admin);
    drop(state);
    // Not again once users exist.
    let state = AppState::from_config(&config).await.unwrap();
    let users = UserRepo::new(&state.ctx.storage, state.ctx.clock.as_ref());
    assert_eq!(users.count().await.unwrap(), 1);
}

#[tokio::test]
async fn invitations_open_registration_and_workspaces() {
    let state = state(Registration::Invite).await;
    let app = router(state);
    let root = session(&app, "root", "root-password").await;
    let ws = send(&app, cmd("workspace.ensureDefault", json!({}), &root)).await;
    let ws_id = ws.body[0]["id"].clone();

    // Without an invitation registration is closed.
    let res = send(
        &app,
        json_post(
            "/api/v1/auth/register",
            json!({ "username": "cy", "password": "cy-password" }),
        ),
    )
    .await;
    assert_eq!(res.body["code"], "auth.registration_closed");

    let created = send(
        &app,
        cmd(
            "members.invite",
            json!({ "workspaceId": ws_id, "role": "viewer" }),
            &root,
        ),
    )
    .await;
    let token = created.body["token"].as_str().unwrap().to_string();
    let preview = send(
        &app,
        Request::get(format!("/api/v1/auth/invite/{token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert_eq!(preview.body["workspaceName"], "My Workspace");
    assert_eq!(preview.body["role"], "viewer");
    assert_eq!(preview.body["invitedBy"], "Root");

    let bad = send(
        &app,
        json_post(
            "/api/v1/auth/register",
            json!({ "username": "cy", "password": "cy-password", "invite": "inv_nope" }),
        ),
    )
    .await;
    assert_eq!(bad.body["code"], "invite.invalid");

    let res = send(
        &app,
        json_post(
            "/api/v1/auth/register",
            json!({ "username": "cy", "password": "cy-password", "invite": token }),
        ),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let cy = res.cookie.unwrap();
    let me = send(&app, cmd("account.me", json!({}), &cy)).await;
    assert_eq!(me.body["roles"][ws_id.as_str().unwrap()], "viewer");

    let used = send(
        &app,
        Request::get(format!("/api/v1/auth/invite/{token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(used.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_panel_shows_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let env = |k: &str| match k {
        "REQLY_DATA_DIR" => Some(dir.path().display().to_string()),
        "REQLY_AUTH_MODE" => Some("multi".into()),
        "REQLY_ADMIN_USERNAME" => Some("boss".into()),
        "REQLY_ADMIN_PASSWORD" => Some("boss-password".into()),
        "REQLY_BLOCK_PRIVATE" => Some("1".into()),
        _ => None,
    };
    let config = reqly_server::Config::load(None, env).unwrap();
    let app = router(AppState::from_config(&config).await.unwrap());
    let cookie = session(&app, "boss", "boss-password").await;
    let res = send(&app, cmd("admin.server", json!({}), &cookie)).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["info"]["authMode"], "multi");
    assert_eq!(res.body["info"]["registration"], "admin");
    assert_eq!(res.body["info"]["blockPrivate"], true);
    assert_eq!(res.body["users"], 1);
    assert!(!res.body.to_string().contains("boss-password"));
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The topic of the next event, `None` when none arrives soon.
async fn next_topic(ws: &mut Socket) -> Option<String> {
    use futures::StreamExt;
    match tokio::time::timeout(std::time::Duration::from_millis(500), ws.next()).await {
        Ok(Some(Ok(msg))) => {
            let v: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
            Some(v["event"]["topic"].as_str().unwrap_or_default().to_string())
        }
        _ => None,
    }
}

/// Events reach the members of the workspace they are about, and private
/// events only the user who caused them.
#[tokio::test]
async fn websocket_events_follow_membership() {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let state = state(Registration::Open).await;
    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app.clone()).into_future());

    let root = session(&app, "root", "root-password").await;
    let bob = send(
        &app,
        json_post(
            "/api/v1/auth/register",
            json!({ "username": "bob", "password": "bob-password" }),
        ),
    )
    .await
    .cookie
    .unwrap();
    let ws_of = |cookie: &str| {
        let cookie = cookie.to_string();
        async move {
            let mut req = format!("ws://{addr}/api/v1/ws")
                .into_client_request()
                .unwrap();
            req.headers_mut()
                .insert(header::COOKIE, cookie.parse().unwrap());
            let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
            ws.next().await.unwrap().unwrap(); // hello
            ws
        }
    };
    let mut root_ws = ws_of(&root).await;
    let mut bob_ws = ws_of(&bob).await;

    // Root's own workspace: Bob hears nothing but the workspace list change.
    let ws = send(&app, cmd("workspace.ensureDefault", json!({}), &root)).await;
    let ws_id = ws.body[0]["id"].clone();
    let col = send(
        &app,
        cmd("collection.list", json!({ "workspaceId": ws_id }), &root),
    )
    .await
    .body[0]["id"]
        .clone();
    while next_topic(&mut root_ws).await.is_some() {}
    while next_topic(&mut bob_ws).await.is_some() {}

    let create = json!({ "collectionId": col, "parentId": null, "name": "R",
        "data": { "kind": "request", "method": "GET", "url": "http://x" } });
    send(&app, cmd("node.create", create.clone(), &root)).await;
    assert_eq!(
        next_topic(&mut root_ws).await.as_deref(),
        Some("tree.changed")
    );
    assert_eq!(next_topic(&mut bob_ws).await, None);

    send(
        &app,
        cmd("settings.set", json!({ "key": "ui.x", "value": 1 }), &root),
    )
    .await;
    assert_eq!(
        next_topic(&mut root_ws).await.as_deref(),
        Some("settings.changed")
    );
    assert_eq!(next_topic(&mut bob_ws).await, None);

    // Once Bob is a member he gets the workspace's changes.
    send(
        &app,
        cmd(
            "members.set",
            json!({ "workspaceId": ws_id, "username": "bob", "role": "viewer" }),
            &root,
        ),
    )
    .await;
    assert_eq!(
        next_topic(&mut bob_ws).await.as_deref(),
        Some("workspaces.changed")
    );
    while next_topic(&mut root_ws).await.is_some() {}
    send(&app, cmd("node.create", create, &root)).await;
    assert_eq!(
        next_topic(&mut bob_ws).await.as_deref(),
        Some("tree.changed")
    );

    root_ws.close(None).await.unwrap();
    bob_ws.close(None).await.unwrap();
}
