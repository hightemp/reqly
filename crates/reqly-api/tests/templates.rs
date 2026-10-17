//! Environments and templates through the command registry.

use reqly_api::commands::{AppContext, Platform, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    collection: Value,
    workspace: Value,
}

impl Fx {
    async fn new() -> Self {
        Self::on(Platform::Test).await
    }

    async fn on(platform: Platform) -> Self {
        let ctx = AppContext::for_tests_on(platform).await;
        let ws = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let col =
            call(&ctx, "collection.list", json!({ "workspaceId": ws })).await[0]["id"].clone();
        Self {
            ctx,
            collection: col,
            workspace: ws,
        }
    }

    async fn node(&self, parent: Value, name: &str, data: Value) -> Value {
        self.call("node.create", json!({ "collectionId": self.collection, "parentId": parent, "name": name, "data": data }))
            .await["id"]
            .clone()
    }

    async fn call(&self, name: &str, input: Value) -> Value {
        call(&self.ctx, name, input).await
    }

    async fn try_call(&self, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
        registry().dispatch(&self.ctx, name, input).await
    }

    async fn envs(&self) -> Value {
        self.call(
            "environment.list",
            json!({ "collectionId": self.collection }),
        )
        .await
    }

    async fn set_data(&self, id: &Value, data: Value) {
        self.call("environment.update", json!({ "id": id, "data": data }))
            .await;
    }

    /// Sends and returns the echoed request as JSON.
    async fn echo(&self, req: &Value, prompts: Value) -> Value {
        let record = self
            .call(
                "request.send",
                json!({ "requestId": req, "promptValues": prompts }),
            )
            .await;
        assert_eq!(record["status"], 200, "{record}");
        let body = self
            .call(
                "response.body",
                json!({ "id": record["id"], "offset": 0, "limit": 1_000_000 }),
            )
            .await;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body["base64"].as_str().unwrap())
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    registry()
        .dispatch(ctx, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn header<'a>(echo: &'a Value, name: &str) -> Option<&'a str> {
    echo["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h[0] == name)
        .and_then(|h| h[1].as_str())
}

#[tokio::test]
async fn environment_crud_and_activation() {
    let fx = Fx::new().await;
    let envs = fx.envs().await;
    assert_eq!(envs["collection"].as_array().unwrap().len(), 1);
    assert_eq!(envs["collection"][0]["kind"], "base");
    assert!(envs["activeId"].is_null());

    let dev = fx
        .call("environment.create", json!({ "workspaceId": fx.workspace, "collectionId": fx.collection, "kind": "sub", "name": "dev" }))
        .await;
    let global = fx
        .call("environment.create", json!({ "workspaceId": fx.workspace, "collectionId": null, "kind": "global", "name": "shared" }))
        .await;
    fx.call(
        "environment.setActive",
        json!({ "collectionId": fx.collection, "environmentId": dev["id"] }),
    )
    .await;
    fx.call(
        "environment.setActiveGlobal",
        json!({ "workspaceId": fx.workspace, "environmentId": global["id"] }),
    )
    .await;
    let envs = fx.envs().await;
    assert_eq!(envs["activeId"], dev["id"]);
    assert_eq!(envs["activeGlobalId"], global["id"]);
    assert_eq!(envs["global"].as_array().unwrap().len(), 1);

    let mut events = fx.ctx.events.subscribe();
    fx.call(
        "environment.update",
        json!({ "id": dev["id"], "name": "development", "color": "#00ff00" }),
    )
    .await;
    assert_eq!(events.recv().await.unwrap().topic, "environments.changed");

    fx.call("environment.delete", json!({ "id": dev["id"] }))
        .await;
    assert!(
        fx.envs().await["activeId"].is_null(),
        "deleting the active environment deactivates it"
    );
}

#[tokio::test]
async fn send_renders_variables_with_precedence() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let envs = fx.envs().await;
    let base = envs["collection"][0]["id"].clone();
    fx.set_data(
        &base,
        json!({ "host": server.url(""), "who": "base", "token": "base-token", "n": 1 }),
    )
    .await;
    let dev = fx
        .call("environment.create", json!({ "workspaceId": fx.workspace, "collectionId": fx.collection, "kind": "sub", "name": "dev" }))
        .await;
    fx.set_data(
        &dev["id"],
        json!({ "who": "dev", "api": { "version": "v2" } }),
    )
    .await;
    fx.call(
        "environment.setActive",
        json!({ "collectionId": fx.collection, "environmentId": dev["id"] }),
    )
    .await;
    let global = fx
        .call("environment.create", json!({ "workspaceId": fx.workspace, "collectionId": null, "kind": "global", "name": "g" }))
        .await;
    fx.set_data(
        &global["id"],
        json!({ "who": "global", "only_global": "G" }),
    )
    .await;
    fx.call(
        "environment.setActiveGlobal",
        json!({ "workspaceId": fx.workspace, "environmentId": global["id"] }),
    )
    .await;

    let folder = fx
        .node(
            Value::Null,
            "f",
            json!({ "kind": "folder", "variables": { "path": "/echo", "n": 2 },
        "headers": [{ "name": "X-Folder", "value": "{{ who }}" }] }),
        )
        .await;
    let req = fx
        .node(folder, "r", json!({
            "kind": "request", "method": "POST",
            "url": "{{ host }}{{ path }}",
            "params": [{ "name": "v", "value": "{{ _.api.version }}" }, { "name": "n", "value": "{{ n }}" }],
            "headers": [{ "name": "X-Global", "value": "{{ only_global }}" }, { "name": "X-Req", "value": "{{ mine }}" }],
            "auth": { "type": "bearer", "token": "{{ token }}", "prefix": "" },
            "body": { "kind": "json", "text": "{\"id\": \"{% base64 'encode', who %}\"}" },
            "variables": { "mine": "request" }
        }))
        .await;

    let echo = fx.echo(&req, json!({})).await;
    assert_eq!(echo["path"], "/echo");
    assert_eq!(echo["query"], json!([["v", "v2"], ["n", "2"]]));
    assert_eq!(header(&echo, "x-folder"), Some("dev"));
    assert_eq!(header(&echo, "x-global"), Some("G"));
    assert_eq!(header(&echo, "x-req"), Some("request"));
    assert_eq!(header(&echo, "authorization"), Some("Bearer base-token"));
    assert_eq!(echo["body"], "{\"id\": \"ZGV2\"}");
}

#[tokio::test]
async fn template_errors_stop_the_send() {
    let fx = Fx::new().await;
    let req = fx
        .node(
            Value::Null,
            "r",
            json!({ "kind": "request", "url": "http://localhost/{{ missing }}" }),
        )
        .await;
    let err = fx
        .try_call("request.send", json!({ "requestId": req }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.undefined");
    assert_eq!(err.params["name"], "missing");
    let history = fx
        .call("response.list", json!({ "requestId": req, "limit": 10 }))
        .await;
    assert_eq!(history.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn prompts_are_listed_and_required() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let req = fx
        .node(Value::Null, "r", json!({ "kind": "request",
            "url": format!("{}?code={{% prompt 'Code', 'Enter the code', '', 'otp' %}}", server.url("/echo")) }))
        .await;
    let prompts = fx
        .call("template.prompts", json!({ "requestId": req }))
        .await;
    assert_eq!(
        prompts,
        json!([{ "key": "otp", "title": "Code", "label": "Enter the code", "defaultValue": "" }])
    );
    let err = fx
        .try_call("request.send", json!({ "requestId": req }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.prompt_required");
    let echo = fx.echo(&req, json!({ "otp": "1234" })).await;
    assert_eq!(echo["query"], json!([["code", "1234"]]));
}

#[tokio::test]
async fn response_chaining() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let login = fx
        .node(
            Value::Null,
            "login",
            json!({ "kind": "request", "method": "PUT", "url": server.url("/echo"),
            "headers": [{ "name": "X-Token", "value": "t-42" }] }),
        )
        .await;
    let url = |field: &str, path: &str, trigger: &str| {
        format!(
            "{}?v={{% response '{field}', '{}', '{path}', '{trigger}' %}}",
            server.url("/echo"),
            login.as_str().unwrap()
        )
    };
    let never = fx
        .node(
            Value::Null,
            "a",
            json!({ "kind": "request", "url": url("body", "$.method", "never") }),
        )
        .await;
    assert_eq!(
        fx.try_call("request.send", json!({ "requestId": never }))
            .await
            .unwrap_err()
            .code,
        "template.response_missing"
    );

    let no_history = fx
        .node(
            Value::Null,
            "b",
            json!({ "kind": "request", "url": url("body", "$.method", "no-history") }),
        )
        .await;
    assert_eq!(
        fx.echo(&no_history, json!({})).await["query"],
        json!([["v", "PUT"]])
    );
    // Now there is a response, `never` works too.
    assert_eq!(
        fx.echo(&never, json!({})).await["query"],
        json!([["v", "PUT"]])
    );

    let status = fx
        .node(
            Value::Null,
            "c",
            json!({ "kind": "request", "url": url("status", "", "never") }),
        )
        .await;
    assert_eq!(
        fx.echo(&status, json!({})).await["query"],
        json!([["v", "200"]])
    );
    let hdr = fx
        .node(
            Value::Null,
            "d",
            json!({ "kind": "request", "url": url("header", "content-type", "never") }),
        )
        .await;
    assert_eq!(
        fx.echo(&hdr, json!({})).await["query"],
        json!([["v", "application/json"]])
    );

    let before = fx
        .call("response.list", json!({ "requestId": login, "limit": 10 }))
        .await
        .as_array()
        .unwrap()
        .len();
    let always = fx
        .node(
            Value::Null,
            "e",
            json!({ "kind": "request", "url": url("body", "$.method", "always") }),
        )
        .await;
    fx.echo(&always, json!({})).await;
    let after = fx
        .call("response.list", json!({ "requestId": login, "limit": 10 }))
        .await
        .as_array()
        .unwrap()
        .len();
    assert_eq!(after, before + 1, "trigger 'always' sends the dependency");
}

#[tokio::test]
async fn self_referencing_chains_fail() {
    let fx = Fx::new().await;
    let req = fx
        .node(
            Value::Null,
            "loop",
            json!({ "kind": "request", "url": "http://localhost/" }),
        )
        .await;
    let url = format!(
        "http://localhost/{{% response 'body', '{}', '$.x', 'always' %}}",
        req.as_str().unwrap()
    );
    fx.call(
        "node.update",
        json!({ "id": req, "data": { "kind": "request", "url": url } }),
    )
    .await;
    assert_eq!(
        fx.try_call("request.send", json!({ "requestId": req }))
            .await
            .unwrap_err()
            .code,
        "template.response_cycle"
    );
}

#[tokio::test]
async fn cookie_and_request_tags() {
    let fx = Fx::new().await;
    fx.call("cookies.upsert", json!({ "workspaceId": fx.workspace, "cookie": {
        "id": reqly_core::Id::new(), "domain": "example.com", "includeSubdomains": false, "path": "/",
        "secure": false, "httpOnly": false, "expires": null, "name": "sid", "value": "abc" } })).await;
    let req = fx
        .node(
            Value::Null,
            "named",
            json!({ "kind": "request", "method": "PATCH", "url": "https://example.com/x" }),
        )
        .await;
    let out = fx
        .call("template.preview", json!({ "collectionId": fx.collection, "nodeId": req,
            "text": "{% cookie 'https://example.com/a', 'sid' %}|{% request 'name' %}|{% request 'method' %}|{% request 'url' %}" }))
        .await;
    assert_eq!(out["value"], "abc|named|PATCH|https://example.com/x");
}

#[tokio::test]
async fn preview_variables_and_function_catalog() {
    let fx = Fx::new().await;
    let base = fx.envs().await["collection"][0]["id"].clone();
    fx.call(
        "environment.update",
        json!({ "id": base, "data": { "a": "1", "pw": "secret" }, "secrets": ["pw"] }),
    )
    .await;
    let vars = fx
        .call(
            "template.variables",
            json!({ "collectionId": fx.collection, "nodeId": null }),
        )
        .await;
    let names: Vec<_> = vars
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["a", "pw"]);
    assert_eq!(vars[1]["secret"], true);
    assert_eq!(vars[1]["source"], "base");

    let ok = fx
        .call(
            "template.preview",
            json!({ "collectionId": fx.collection, "text": "x={{ a }}" }),
        )
        .await;
    assert_eq!(ok["value"], "x=1");
    let err = fx
        .try_call(
            "template.preview",
            json!({ "collectionId": fx.collection, "text": "{{ nope }}" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.undefined");

    let catalog = fx.call("template.functions", json!({})).await;
    assert!(
        catalog["functions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "hash")
    );
    assert!(
        catalog["faker"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "person.firstName")
    );
}

#[tokio::test]
async fn file_and_os_env_are_desktop_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "from file").unwrap();
    let text = format!("{{% file '{}' %}}", file.display());

    let desktop = Fx::on(Platform::Desktop).await;
    let out = desktop
        .call(
            "template.preview",
            json!({ "collectionId": desktop.collection, "text": text }),
        )
        .await;
    assert_eq!(out["value"], "from file");

    let server = Fx::on(Platform::Server).await;
    let err = server
        .try_call(
            "template.preview",
            json!({ "collectionId": server.collection, "text": text }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.not_allowed");
    let err = server
        .try_call(
            "template.preview",
            json!({ "collectionId": server.collection, "text": "{% os 'env', 'PATH' %}" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.not_allowed");
    let os = server
        .call(
            "template.preview",
            json!({ "collectionId": server.collection, "text": "{% os 'platform' %}" }),
        )
        .await;
    assert!(!os["value"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn server_file_tag_reads_uploaded_blobs() {
    use base64::Engine;
    let fx = Fx::on(Platform::Server).await;
    let blob = fx
        .call("blob.upload", json!({ "name": "a.txt", "base64": base64::engine::general_purpose::STANDARD.encode("blob text") }))
        .await;
    let text = format!("{{% file '{}' %}}", blob["hash"].as_str().unwrap());
    let out = fx
        .call(
            "template.preview",
            json!({ "collectionId": fx.collection, "text": text }),
        )
        .await;
    assert_eq!(out["value"], "blob text");
}

#[tokio::test]
async fn faker_locale_per_collection_overrides_global() {
    let fx = Fx::new().await;
    fx.call(
        "settings.set",
        json!({ "key": "template.fakerLocale", "value": "en" }),
    )
    .await;
    let key = format!("template.fakerLocale.{}", fx.collection.as_str().unwrap());
    fx.call("settings.set", json!({ "key": key, "value": "ru" }))
        .await;
    let out = fx
        .call(
            "template.preview",
            json!({ "collectionId": fx.collection, "text": "{{ $faker.phone.number }}" }),
        )
        .await;
    assert!(out["value"].as_str().unwrap().starts_with("+7"), "{out}");
}

#[tokio::test]
async fn proxy_settings_are_rendered() {
    let proxy = TestServer::start_proxy().await;
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let base = fx.envs().await["collection"][0]["id"].clone();
    fx.set_data(
        &base,
        json!({ "proxy": format!("http://{}", proxy.addr()) }),
    )
    .await;
    let req = fx
        .node(Value::Null, "r", json!({ "kind": "request", "url": server.url("/echo"),
            "settings": { "proxy": { "mode": "manual", "url": "{{ proxy }}", "username": "", "password": "", "noProxy": "" } } }))
        .await;
    let echo = fx.echo(&req, json!({})).await;
    assert_eq!(echo["proxied"], true, "{echo}");
}
