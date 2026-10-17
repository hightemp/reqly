//! Advanced auth through the command registry: OAuth 2.0 grants and token
//! storage, Digest and signing schemes on `request.send`.

use reqly_api::commands::{AppContext, Platform, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    collection: Value,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests().await;
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let collection =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();
        Self { ctx, collection }
    }

    async fn node(&self, parent: Value, data: Value) -> Value {
        self.call("node.create", json!({ "collectionId": self.collection, "parentId": parent, "name": "n", "data": data }))
            .await["id"]
            .clone()
    }

    async fn call(&self, name: &str, input: Value) -> Value {
        call(&self.ctx, name, input).await
    }

    async fn try_call(&self, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
        registry().dispatch(&self.ctx, name, input).await
    }

    /// Sends and returns (status, body as JSON).
    async fn send(&self, id: &Value) -> (Value, Value) {
        // Auth failures happen before sending: the command fails.
        let record = match self
            .try_call("request.send", json!({ "requestId": id }))
            .await
        {
            Ok(r) => r,
            Err(e) => return (Value::Null, serde_json::to_value(&e).unwrap()),
        };
        if record["status"].is_null() {
            return (Value::Null, record["error"].clone());
        }
        let chunk = self
            .call(
                "response.body",
                json!({ "id": record["id"], "offset": 0, "limit": 1_000_000 }),
            )
            .await;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk["base64"].as_str().unwrap())
            .unwrap();
        (
            record["status"].clone(),
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    registry()
        .dispatch(ctx, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn oauth2(server: &TestServer, extra: Value) -> Value {
    let mut auth = json!({
        "type": "oauth2",
        "tokenUrl": server.url("/oauth/token"),
        "authUrl": server.url("/oauth/authorize"),
        "deviceUrl": server.url("/oauth/device"),
        "clientId": "reqly",
        "clientSecret": "secret",
        "autoRefresh": true,
    });
    for (k, v) in extra.as_object().unwrap() {
        auth[k] = v.clone();
    }
    auth
}

#[tokio::test]
async fn client_credentials_on_a_folder_are_inherited_and_cached() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let folder = fx
        .node(Value::Null, json!({ "kind": "folder", "auth": oauth2(&server, json!({ "grant": "clientCredentials", "scope": "read" })) }))
        .await;
    let req = fx
        .node(
            folder.clone(),
            json!({ "kind": "request", "url": server.url("/oauth/resource") }),
        )
        .await;

    assert_eq!(
        fx.call("oauth2.status", json!({ "nodeId": folder })).await["status"],
        "none"
    );
    let (status, body) = fx.send(&req).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["scope"], "read");
    let first = body["token"].clone();

    // The stored token is reused.
    let (_, again) = fx.send(&req).await;
    assert_eq!(again["token"], first);
    let info = fx.call("oauth2.status", json!({ "nodeId": folder })).await;
    assert_eq!(info["status"], "valid");
    assert_eq!(info["accessToken"], first);

    // Changing the settings makes the token stale.
    fx.call(
        "node.update",
        json!({ "id": folder, "data": { "kind": "folder", "auth": oauth2(&server, json!({ "grant": "clientCredentials", "scope": "write" })) } }),
    )
    .await;
    assert_eq!(
        fx.call("oauth2.status", json!({ "nodeId": folder })).await["status"],
        "stale"
    );
    let (_, body) = fx.send(&req).await;
    assert_eq!(body["scope"], "write");

    fx.call("oauth2.clear", json!({ "nodeId": folder })).await;
    assert_eq!(
        fx.call("oauth2.status", json!({ "nodeId": folder })).await["status"],
        "none"
    );
}

#[tokio::test]
async fn password_grant_refreshes_expired_tokens() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    // Tokens live 1 s: within the refresh margin right away.
    let auth = oauth2(
        &server,
        json!({ "grant": "password", "tokenUrl": server.url("/oauth/token?expires_in=1"), "username": "alice", "password": "pw", "clientAuth": "body" }),
    );
    let req = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/oauth/resource"), "auth": auth }),
        )
        .await;
    let (status, first) = fx.send(&req).await;
    assert_eq!(status, 200, "{first}");
    assert!(
        fx.call("oauth2.status", json!({ "nodeId": req })).await["hasRefreshToken"]
            .as_bool()
            .unwrap()
    );
    let (status, second) = fx.send(&req).await;
    assert_eq!(status, 200);
    assert_ne!(first["token"], second["token"], "refreshed");

    // A wrong password surfaces the provider's error.
    let bad = oauth2(
        &server,
        json!({ "grant": "password", "username": "alice", "password": "nope" }),
    );
    let req = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/oauth/resource"), "auth": bad }),
        )
        .await;
    let (status, error) = fx.send(&req).await;
    assert!(status.is_null());
    assert_eq!(error["code"], "auth.oauth2_error");
    assert_eq!(error["params"]["error"], "invalid_grant");
}

/// Follows the provider's authorize redirect without a browser.
async fn redirect_of(url: &str) -> String {
    let request = reqly_core::model::HttpRequest {
        url: url.into(),
        settings: reqly_core::model::RequestSettings {
            follow_redirects: Some(false),
            ..Default::default()
        },
        ..Default::default()
    };
    let prepared = reqly_http::prepare::prepare(&request, &[]).unwrap();
    let res = reqly_http::engine::execute(
        prepared,
        Default::default(),
        reqly_http::engine::CancelToken::new(),
        None,
    )
    .await
    .unwrap();
    res.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("location"))
        .unwrap()
        .1
        .clone()
}

#[tokio::test]
async fn authorization_code_with_pkce() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let auth = oauth2(
        &server,
        json!({ "grant": "authorizationCode", "pkce": true, "clientSecret": "", "scope": "profile" }),
    );
    let req = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/oauth/resource"), "auth": auth }),
        )
        .await;

    // Sending without a token asks to sign in.
    let (_, error) = fx.send(&req).await;
    assert_eq!(error["code"], "auth.oauth2_authorize");

    let flow = fx
        .call(
            "oauth2.authorize",
            json!({ "nodeId": req, "defaultRedirectUri": "http://localhost:5173/oauth2/callback" }),
        )
        .await;
    let authorize = flow["authorizeUrl"].as_str().unwrap();
    assert!(
        authorize.contains("code_challenge_method=S256"),
        "{authorize}"
    );
    assert_eq!(flow["redirectUri"], "http://localhost:5173/oauth2/callback");
    let callback = redirect_of(authorize).await;
    assert!(
        callback.starts_with("http://localhost:5173/oauth2/callback?code="),
        "{callback}"
    );

    // A tampered state is refused.
    let tampered = callback.replace("state=", "state=x");
    let err = fx
        .try_call(
            "oauth2.complete",
            json!({ "flowId": flow["flowId"], "callbackUrl": tampered }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "auth.oauth2_state");

    let flow = fx
        .call(
            "oauth2.authorize",
            json!({ "nodeId": req, "defaultRedirectUri": "http://localhost:5173/oauth2/callback" }),
        )
        .await;
    let callback = redirect_of(flow["authorizeUrl"].as_str().unwrap()).await;
    let info = fx
        .call(
            "oauth2.complete",
            json!({ "flowId": flow["flowId"], "callbackUrl": callback }),
        )
        .await;
    assert_eq!(info["status"], "valid");
    let (status, body) = fx.send(&req).await;
    assert_eq!(status, 200);
    assert_eq!(body["scope"], "profile");
}

#[tokio::test]
async fn implicit_grant_reads_the_fragment() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let auth = oauth2(
        &server,
        json!({ "grant": "implicit", "redirectUrl": "https://app.test/cb" }),
    );
    let req = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/oauth/resource"), "auth": auth }),
        )
        .await;
    let flow = fx.call("oauth2.authorize", json!({ "nodeId": req })).await;
    assert_eq!(flow["redirectUri"], "https://app.test/cb");
    let callback = redirect_of(flow["authorizeUrl"].as_str().unwrap()).await;
    assert!(callback.contains("#access_token="), "{callback}");
    fx.call(
        "oauth2.complete",
        json!({ "flowId": flow["flowId"], "callbackUrl": callback }),
    )
    .await;
    assert_eq!(fx.send(&req).await.0, 200);
}

#[tokio::test]
async fn device_code_grant() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let auth = oauth2(
        &server,
        json!({ "grant": "deviceCode", "clientSecret": "" }),
    );
    let req = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/oauth/resource"), "auth": auth }),
        )
        .await;
    let start = fx
        .call("oauth2.deviceStart", json!({ "nodeId": req }))
        .await;
    assert!(
        start["verificationUri"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
    let poll = fx
        .call("oauth2.devicePoll", json!({ "flowId": start["flowId"] }))
        .await;
    assert_eq!(poll["status"], "pending");
    let approve = server.url(&format!(
        "/oauth/device/approve?user_code={}",
        start["userCode"].as_str().unwrap()
    ));
    let prepared = reqly_http::prepare::prepare(
        &reqly_core::model::HttpRequest {
            url: approve,
            ..Default::default()
        },
        &[],
    )
    .unwrap();
    reqly_http::engine::execute(
        prepared,
        Default::default(),
        reqly_http::engine::CancelToken::new(),
        None,
    )
    .await
    .unwrap();
    let poll = fx
        .call("oauth2.devicePoll", json!({ "flowId": start["flowId"] }))
        .await;
    assert_eq!(poll["status"], "done");
    assert_eq!(poll["token"]["status"], "valid");
    assert_eq!(fx.send(&req).await.0, 200);
}

#[tokio::test]
async fn templates_in_auth_fields_and_digest() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let folder = fx
        .node(
            Value::Null,
            json!({ "kind": "folder", "variables": { "user": "alice" },
                    "auth": { "type": "digest", "username": "{{ user }}", "password": "s3cret" } }),
        )
        .await;
    let req = fx
        .node(
            folder,
            json!({ "kind": "request", "url": server.url("/digest/auth/SHA-256/alice/s3cret") }),
        )
        .await;
    let (status, body) = fx.send(&req).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["user"], "alice");
}

#[tokio::test]
async fn server_refuses_netrc() {
    let mut ctx = AppContext::for_tests().await;
    ctx.platform = Platform::Server;
    let fx = Fx {
        collection: Value::Null,
        ctx,
    };
    let ws = fx.call("workspace.ensureDefault", json!({})).await[0]["id"].clone();
    let collection = fx
        .call("collection.list", json!({ "workspaceId": ws }))
        .await[0]["id"]
        .clone();
    let fx = Fx { collection, ..fx };
    let req = fx
        .node(Value::Null, json!({ "kind": "request", "url": "http://127.0.0.1:9/", "auth": { "type": "netrc", "file": "" } }))
        .await;
    let (status, error) = fx.send(&req).await;
    assert!(status.is_null());
    assert_eq!(error["code"], "http.not_allowed");
}
