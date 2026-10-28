//! The collection runner (runner.run) through the command registry.

use std::sync::Arc;

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

struct Fx {
    ctx: Arc<AppContext>,
    workspace: Value,
    collection: Value,
    server: TestServer,
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
        Self {
            ctx: Arc::new(ctx),
            workspace,
            collection,
            server: TestServer::start().await,
        }
    }

    async fn node(&self, parent: &Value, name: &str, data: Value) -> Value {
        call(
            &self.ctx,
            "node.create",
            json!({ "collectionId": self.collection, "parentId": parent, "name": name, "data": data }),
        )
        .await
        .unwrap()["id"]
        .clone()
    }

    /// A GET of `/echo?n=<name>` with extra request fields.
    async fn echo(&self, parent: &Value, name: &str, extra: Value) -> Value {
        let mut data =
            json!({ "kind": "request", "url": format!("{}?n={name}", self.server.url("/echo")) });
        data.as_object_mut()
            .unwrap()
            .extend(extra.as_object().cloned().unwrap_or_default());
        self.node(parent, name, data).await
    }

    async fn run(&self, input: Value) -> Value {
        let mut input = input;
        input["collectionId"] = self.collection.clone();
        if input.get("runId").is_none() {
            input["runId"] = json!("run-1");
        }
        call(&self.ctx, "runner.run", input).await.unwrap()
    }
}

fn names(report: &Value) -> Vec<String> {
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| format!("{}:{}", i["iteration"], i["name"].as_str().unwrap()))
        .collect()
}

#[tokio::test]
async fn runs_requests_in_tree_order_and_skips_streams() {
    let fx = Fx::new().await;
    let a = fx.echo(&Value::Null, "a", json!({})).await;
    let folder = fx
        .node(&Value::Null, "F", json!({ "kind": "folder" }))
        .await;
    fx.echo(&folder, "b", json!({})).await;
    fx.node(
        &Value::Null,
        "ws",
        json!({ "kind": "webSocket", "url": "ws://127.0.0.1:1" }),
    )
    .await;
    fx.echo(&Value::Null, "sse", json!({ "eventStream": true }))
        .await;
    fx.echo(&Value::Null, "c", json!({})).await;

    let report = fx.run(json!({})).await;
    assert_eq!(names(&report), ["0:a", "0:b", "0:c"], "{report}");
    let first = &report["items"][0];
    assert_eq!(first["requestId"], a);
    assert_eq!(first["status"], 200);
    assert_eq!(first["method"], "GET");
    assert!(first["responseId"].is_string());
    assert_eq!(report["items"][1]["path"], json!(["F"]));
    assert_eq!(report["iterations"], 1);
    assert_eq!(report["cancelled"], false);

    // A folder runs on its own; an explicit list sets the order.
    let report = fx.run(json!({ "folderId": folder })).await;
    assert_eq!(names(&report), ["0:b"]);
    assert_eq!(report["name"], "F");
    let ids: Vec<Value> = vec![report["items"][0]["requestId"].clone(), a];
    let report = fx.run(json!({ "requestIds": ids })).await;
    assert_eq!(names(&report), ["0:b", "0:a"]);
}

#[tokio::test]
async fn iterations_use_data_rows_in_templates_scripts_and_assertions() {
    let fx = Fx::new().await;
    fx.echo(
        &Value::Null,
        "user",
        json!({
            "headers": [{ "name": "X-User", "value": "{{ user }}" }],
            "scripts": { "afterResponse": "rq.test('iteration ' + rq.info.iteration + ' of ' + rq.info.iterationCount + ' is ' + rq.iterationData.get('user'), () => {});" },
            "assertions": [{ "source": "jsonPath", "property": "$.headers[?@[0]=='x-user'][1]", "operator": "equals", "value": "{{ user }}" }]
        }),
    )
    .await;
    let report = fx
        .run(json!({ "data": { "format": "csv", "text": "user\nada\nbob\n" } }))
        .await;
    assert_eq!(names(&report), ["0:user", "1:user"], "{report}");
    assert_eq!(report["iterations"], 2);
    let tests: Vec<&str> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["tests"][0]["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        tests,
        ["iteration 0 of 2 is ada", "iteration 1 of 2 is bob"]
    );
    assert!(
        report["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["assertions"][0]["passed"] == true),
        "{report}"
    );

    // More iterations than rows reuse the last row; data stays out of later sends.
    let report = fx
        .run(json!({ "iterations": 3, "data": { "format": "json", "text": "[{\"user\":\"ada\"}]" } }))
        .await;
    assert_eq!(report["iterations"], 3);
    assert!(
        report["items"][2]["tests"][0]["name"]
            .as_str()
            .unwrap()
            .ends_with("is ada")
    );
    let id = report["items"][0]["requestId"].clone();
    let err = call(&fx.ctx, "request.send", json!({ "requestId": id }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "template.undefined");
}

#[tokio::test]
async fn reports_render_in_each_format() {
    let fx = Fx::new().await;
    fx.echo(&Value::Null, "a", json!({})).await;
    let report = fx.run(json!({})).await;
    for (format, ext) in [("json", "json"), ("junit", "xml"), ("html", "html")] {
        let out = call(
            &fx.ctx,
            "runner.report",
            json!({ "report": report, "format": format }),
        )
        .await
        .unwrap();
        let name = out["fileName"].as_str().unwrap();
        assert!(name.ends_with(&format!("-report.{ext}")), "{name}");
        assert!(out["text"].as_str().unwrap().contains('a'));
    }
}

#[tokio::test]
async fn bad_data_is_refused() {
    let fx = Fx::new().await;
    fx.echo(&Value::Null, "a", json!({})).await;
    let err = call(
        &fx.ctx,
        "runner.run",
        json!({ "runId": "r", "collectionId": fx.collection, "data": { "format": "json", "text": "{}" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "runner.data");
}

#[tokio::test]
async fn the_chosen_environment_applies_only_to_the_run() {
    let fx = Fx::new().await;
    let create = |name: &str, host: &str| json!({ "workspaceId": fx.workspace, "collectionId": fx.collection, "kind": "sub", "name": name, "data": { "who": host } });
    let dev = call(&fx.ctx, "environment.create", create("dev", "dev"))
        .await
        .unwrap()["id"]
        .clone();
    let prod = call(&fx.ctx, "environment.create", create("prod", "prod"))
        .await
        .unwrap()["id"]
        .clone();
    call(
        &fx.ctx,
        "environment.setActive",
        json!({ "collectionId": fx.collection, "environmentId": dev }),
    )
    .await
    .unwrap();
    fx.echo(
        &Value::Null,
        "env",
        json!({
            "headers": [{ "name": "X-Who", "value": "{{ who }}" }],
            "scripts": { "afterResponse": "rq.test(rq.environment.get('who'), () => {});" }
        }),
    )
    .await;

    let report = fx.run(json!({ "environmentId": prod })).await;
    assert_eq!(report["environment"], "prod");
    assert_eq!(report["items"][0]["tests"][0]["name"], "prod", "{report}");
    let list = call(
        &fx.ctx,
        "environment.list",
        json!({ "collectionId": fx.collection }),
    )
    .await
    .unwrap();
    assert_eq!(list["activeId"], dev);
}

#[tokio::test]
async fn stop_on_failure_and_set_next_request() {
    let fx = Fx::new().await;
    let fail = json!([{ "source": "status", "operator": "equals", "value": "201" }]);
    fx.echo(&Value::Null, "a", json!({})).await;
    fx.echo(&Value::Null, "b", json!({ "assertions": fail }))
        .await;
    fx.echo(&Value::Null, "c", json!({})).await;

    let report = fx.run(json!({ "iterations": 2 })).await;
    assert_eq!(names(&report).len(), 6);
    assert_eq!(report["stopped"], false);

    let report = fx
        .run(json!({ "iterations": 2, "stopOnFailure": true }))
        .await;
    assert_eq!(names(&report), ["0:a", "0:b"]);
    assert_eq!(report["stopped"], true);

    let fx = Fx::new().await;
    fx.echo(
        &Value::Null,
        "a",
        json!({ "scripts": { "afterResponse": "pm.execution.setNextRequest('c');" } }),
    )
    .await;
    fx.echo(&Value::Null, "b", json!({})).await;
    fx.echo(
        &Value::Null,
        "c",
        json!({ "scripts": { "afterResponse": "if (rq.info.iteration === 0) postman.setNextRequest(null);" } }),
    )
    .await;
    fx.echo(&Value::Null, "d", json!({})).await;
    let report = fx.run(json!({ "iterations": 2 })).await;
    assert_eq!(names(&report), ["0:a", "0:c", "1:a", "1:c", "1:d"]);
}

#[tokio::test]
async fn progress_events_and_cancel() {
    let fx = Fx::new().await;
    for name in ["a", "b", "c"] {
        fx.echo(&Value::Null, name, json!({})).await;
    }
    let mut events = fx.ctx.events.subscribe();
    let ctx = fx.ctx.clone();
    let collection = fx.collection.clone();
    let run = tokio::spawn(async move {
        call(
            &ctx,
            "runner.run",
            json!({ "runId": "slow", "collectionId": collection, "delayMs": 300 }),
        )
        .await
        .unwrap()
    });
    loop {
        let e = events.recv().await.unwrap();
        if e.topic == "runner.progress" {
            assert_eq!(e.payload["runId"], "slow");
            assert_eq!(e.payload["item"]["name"], "a");
            assert_eq!(e.payload["total"], 3);
            break;
        }
    }
    call(&fx.ctx, "runner.cancel", json!({ "runId": "slow" }))
        .await
        .unwrap();
    let report = run.await.unwrap();
    assert_eq!(report["cancelled"], true);
    assert_eq!(names(&report), ["0:a"]);
    loop {
        let e = events.recv().await.unwrap();
        if e.topic == "runner.finished" {
            assert_eq!(e.payload["runId"], "slow");
            break;
        }
    }
}
