//! Scripts around request.send through the command registry.

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

struct Fx {
    ctx: AppContext,
    collection: Value,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests().await;
        let workspace = call(&ctx, "workspace.ensureDefault", json!({}))
            .await
            .unwrap()[0]["id"]
            .clone();
        let collection = call(&ctx, "collection.list", json!({ "workspaceId": workspace }))
            .await
            .unwrap()[0]["id"]
            .clone();
        Self { ctx, collection }
    }

    async fn node(&self, parent: Value, data: Value) -> Value {
        call(&self.ctx, "node.create", json!({ "collectionId": self.collection, "parentId": parent, "name": "n", "data": data }))
            .await
            .unwrap()["id"]
            .clone()
    }

    async fn send(&self, id: &Value) -> Value {
        call(&self.ctx, "request.send", json!({ "requestId": id }))
            .await
            .unwrap()
    }

    async fn body(&self, record: &Value) -> Value {
        let chunk = call(
            &self.ctx,
            "response.body",
            json!({ "id": record["id"], "offset": 0, "limit": 1_000_000 }),
        )
        .await
        .unwrap();
        use base64::Engine;
        serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(chunk["base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap()
    }

    async fn env(&self, kind: &str) -> Value {
        let envs = call(
            &self.ctx,
            "environment.list",
            json!({ "collectionId": self.collection }),
        )
        .await
        .unwrap();
        let list = if kind == "global" {
            &envs["global"]
        } else {
            &envs["collection"]
        };
        list.as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == kind)
            .cloned()
            .unwrap_or(Value::Null)
    }
}

#[tokio::test]
async fn order_variables_request_edits_and_tests() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    call(
        &fx.ctx,
        "collection.updateScripts",
        json!({ "id": fx.collection, "scripts": {
            "preRequest": "console.log('collection'); pm.variables.set('order', 'c');",
            "afterResponse": "pm.test('collection test', () => pm.response.to.have.status(200));"
        } }),
    )
    .await
    .unwrap();
    let folder = fx
        .node(
            Value::Null,
            json!({ "kind": "folder", "scripts": { "preRequest": "console.log('folder'); pm.variables.set('order', pm.variables.get('order') + 'f');" } }),
        )
        .await;
    let req = fx
        .node(
            folder,
            json!({ "kind": "request", "method": "POST", "url": format!("{}/echo", server.url("")),
                    "headers": [{ "name": "X-Order", "value": "{{ order }}" }],
                    "body": { "kind": "json", "text": "{\"a\":1}" },
                    "scripts": {
                        "preRequest": "console.log('request'); pm.variables.set('order', pm.variables.get('order') + 'r');\npm.request.headers.add({ key: 'X-Sig', value: CryptoJS.SHA256(pm.request.body.raw).toString() });\npm.request.url.query.add('q=1');",
                        "afterResponse": "const json = pm.response.json();\npm.environment.set('echoed', json.headers.find(h => h[0] === 'x-order')[1]);\npm.globals.set('g', 1);\npm.test('has sig', () => pm.expect(json.headers.some(h => h[0] === 'x-sig')).to.be.true);\npm.test('fails', () => pm.expect(1).to.equal(2));"
                    } }),
        )
        .await;
    let record = fx.send(&req).await;
    assert_eq!(record["status"], 200, "{record}");
    let logs: Vec<&str> = record["scriptLogs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap())
        .collect();
    assert_eq!(logs, ["collection", "folder", "request"]);
    let tests: Vec<(String, bool)> = record["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_string(),
                t["passed"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        tests,
        [
            ("collection test".into(), true),
            ("has sig".into(), true),
            ("fails".into(), false)
        ]
    );

    let echoed = fx.body(&record).await;
    assert_eq!(echoed["query"], json!([["q", "1"]]), "{echoed}");
    let headers = echoed["headers"].as_array().unwrap();
    assert!(
        headers.iter().any(|h| h[0] == "x-order" && h[1] == "cfr"),
        "templates see runtime variables"
    );
    // The stored request is unchanged.
    let stored = call(&fx.ctx, "node.get", json!({ "id": req }))
        .await
        .unwrap();
    assert_eq!(stored["data"]["headers"].as_array().unwrap().len(), 1);

    // Environment, globals and runtime variables.
    let base = fx.env("base").await;
    assert_eq!(base["data"]["echoed"], "cfr");
    assert_eq!(
        fx.env("global").await["data"]["g"],
        1,
        "a global environment is created"
    );
    let runtime = call(
        &fx.ctx,
        "variables.runtime",
        json!({ "collectionId": fx.collection }),
    )
    .await
    .unwrap();
    assert_eq!(runtime["variables"]["order"], "cfr");
    call(
        &fx.ctx,
        "variables.clearRuntime",
        json!({ "collectionId": fx.collection }),
    )
    .await
    .unwrap();
    assert_eq!(
        call(
            &fx.ctx,
            "variables.runtime",
            json!({ "collectionId": fx.collection })
        )
        .await
        .unwrap(),
        json!({ "variables": {} })
    );
}

#[tokio::test]
async fn errors_skips_and_send_request() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let failing = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/echo"),
                    "scripts": { "preRequest": "console.log('before'); undefinedFunction();" } }),
        )
        .await;
    let record = fx.send(&failing).await;
    assert!(record["status"].is_null());
    assert_eq!(record["error"]["code"], "script.error");
    assert_eq!(record["scriptLogs"][0]["text"], "before");

    let skipped = fx
        .node(Value::Null, json!({ "kind": "request", "url": server.url("/echo"), "scripts": { "preRequest": "pm.execution.skipRequest();" } }))
        .await;
    assert_eq!(fx.send(&skipped).await["error"]["code"], "script.skipped");

    // sendRequest from a pre-request script; after-response errors keep the response.
    let token_url = server.url("/bearer/secret-token");
    let chained = fx
        .node(
            Value::Null,
            json!({ "kind": "request", "url": server.url("/echo"),
                    "auth": { "type": "bearer", "token": "{{ token }}", "prefix": "" },
                    "scripts": {
                        "preRequest": format!("const res = await pm.sendRequest({{ url: '{token_url}', header: {{ Authorization: 'Bearer secret-token' }} }});\npm.variables.set('token', res.code === 200 ? 'got-it' : 'no');"),
                        "afterResponse": "throw new Error('boom');"
                    } }),
        )
        .await;
    let record = fx.send(&chained).await;
    assert_eq!(record["status"], 200);
    let echoed = fx.body(&record).await;
    assert!(
        echoed["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "authorization" && h[1] == "Bearer got-it"),
        "{echoed}"
    );
    let logs = record["scriptLogs"].as_array().unwrap();
    assert_eq!(logs.last().unwrap()["level"], "error");
    assert!(
        logs.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("boom")
    );
}

#[tokio::test]
async fn time_limit_from_settings() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    call(
        &fx.ctx,
        "settings.set",
        json!({ "key": "scripts.settings", "value": { "timeoutMs": 200 } }),
    )
    .await
    .unwrap();
    let req = fx
        .node(Value::Null, json!({ "kind": "request", "url": server.url("/echo"), "scripts": { "preRequest": "while (true) {}" } }))
        .await;
    let started = std::time::Instant::now();
    let record = fx.send(&req).await;
    assert_eq!(record["error"]["code"], "script.timeout");
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
}
