//! Import and export through the command registry.

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    workspace: Value,
    collection: Value,
}

impl Fx {
    async fn new() -> Self {
        let ctx = AppContext::for_tests().await;
        let workspace = call(&ctx, "workspace.ensureDefault", json!({})).await[0]["id"].clone();
        let collection = call(&ctx, "collection.list", json!({ "workspaceId": workspace })).await
            [0]["id"]
            .clone();
        Self {
            ctx,
            workspace,
            collection,
        }
    }

    async fn call(&self, name: &str, input: Value) -> Value {
        call(&self.ctx, name, input).await
    }

    async fn node(&self, parent: Value, name: &str, data: Value) -> Value {
        self.call("node.create", json!({ "collectionId": self.collection, "parentId": parent, "name": name, "data": data }))
            .await["id"]
            .clone()
    }

    /// (name, parent name, kind) in depth-first order.
    async fn tree(&self, collection: &Value) -> Vec<(String, Option<String>, String)> {
        let nodes = self
            .call("tree.list", json!({ "collectionId": collection }))
            .await;
        let nodes = nodes.as_array().unwrap().clone();
        fn walk(
            nodes: &[Value],
            parent: &Value,
            parent_name: Option<String>,
            out: &mut Vec<(String, Option<String>, String)>,
        ) {
            for n in nodes.iter().filter(|n| &n["parentId"] == parent) {
                let name = n["name"].as_str().unwrap().to_string();
                out.push((
                    name.clone(),
                    parent_name.clone(),
                    n["kind"].as_str().unwrap().to_string(),
                ));
                walk(nodes, &n["id"], Some(name), out);
            }
        }
        let mut out = Vec::new();
        walk(&nodes, &Value::Null, None, &mut out);
        out
    }

    async fn sample(&self) -> (Value, Value) {
        let folder = self
            .node(
                Value::Null,
                "Users",
                json!({ "kind": "folder", "headers": [{ "name": "X-Team", "value": "core" }] }),
            )
            .await;
        let login = self
            .node(
                folder.clone(),
                "Login",
                json!({ "kind": "request", "method": "POST", "url": "{{ host }}/login" }),
            )
            .await;
        let token = format!(
            "{{% response 'body', '{}', '$.token', 'never' %}}",
            login.as_str().unwrap()
        );
        let me = self
            .node(
                folder.clone(),
                "Me",
                json!({ "kind": "request", "url": "{{ host }}/me",
                "auth": { "type": "bearer", "token": token, "prefix": "" } }),
            )
            .await;
        let envs = self
            .call(
                "environment.list",
                json!({ "collectionId": self.collection }),
            )
            .await;
        self.call(
            "environment.update",
            json!({ "id": envs["collection"][0]["id"],
            "data": { "host": "https://api.test", "pw": "hunter2" }, "secrets": ["pw"] }),
        )
        .await;
        self.call(
            "environment.create",
            json!({ "workspaceId": self.workspace, "collectionId": self.collection,
            "kind": "sub", "name": "prod", "data": { "host": "https://prod.test" } }),
        )
        .await;
        (login, me)
    }

    /// The single exported file as `{ fileName, content }`.
    async fn export(&self, scope: Value, options: Value) -> Value {
        let out = self
            .call(
                "export.run",
                json!({ "scope": scope, "format": "reqlyJson", "options": options }),
            )
            .await;
        assert_eq!(out["files"].as_array().unwrap().len(), 1);
        json!({ "fileName": out["files"][0]["name"], "content": out["files"][0]["content"] })
    }

    async fn import(&self, content: &str, target: Value) -> Value {
        self.call(
            "import.apply",
            json!({ "sources": [{ "name": "x.reqly.json", "content": content }],
            "workspaceId": self.workspace, "target": target }),
        )
        .await
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    registry()
        .dispatch(ctx, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[tokio::test]
async fn export_and_import_into_a_new_collection() {
    let fx = Fx::new().await;
    let (login, _) = fx.sample().await;
    let out = fx
        .export(
            json!({ "kind": "collection", "collectionId": fx.collection }),
            json!({}),
        )
        .await;
    assert_eq!(out["fileName"], "my-collection.reqly.json");
    let content = out["content"].as_str().unwrap();
    assert!(
        !content.contains("hunter2"),
        "secrets are stripped by default"
    );

    let preview = fx
        .call(
            "import.preview",
            json!({ "sources": [{ "name": "x", "content": content }] }),
        )
        .await;
    assert_eq!(preview["format"], "reqly");
    assert_eq!(preview["collections"][0]["items"][0]["name"], "Users");

    let result = fx
        .import(content, json!({ "kind": "newCollections" }))
        .await;
    assert_eq!(result["requests"], 2);
    assert_eq!(result["environments"], 1);
    let new_col = result["collectionIds"][0].clone();
    assert_ne!(new_col, fx.collection);
    assert_eq!(fx.tree(&new_col).await, fx.tree(&fx.collection).await);

    // References to request ids point at the new copies.
    let nodes = fx
        .call("tree.list", json!({ "collectionId": new_col }))
        .await;
    let new_login = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "Login")
        .unwrap()["id"]
        .clone();
    let new_me = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "Me")
        .unwrap()["id"]
        .clone();
    let me = fx.call("node.get", json!({ "id": new_me })).await;
    let token = me["data"]["auth"]["token"].as_str().unwrap();
    assert!(token.contains(new_login.as_str().unwrap()), "{token}");
    assert!(!token.contains(login.as_str().unwrap()));

    let envs = fx
        .call("environment.list", json!({ "collectionId": new_col }))
        .await;
    assert_eq!(envs["collection"][0]["data"]["host"], "https://api.test");
    assert_eq!(envs["collection"][0]["data"]["pw"], "");
    assert_eq!(envs["collection"][1]["name"], "prod");
}

#[tokio::test]
async fn secrets_history_and_cookies_on_request() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    fx.sample().await;
    let req = fx
        .node(
            Value::Null,
            "Echo",
            json!({ "kind": "request", "url": server.url("/echo") }),
        )
        .await;
    fx.call("request.send", json!({ "requestId": req })).await;
    fx.call(
        "cookies.upsert",
        json!({ "workspaceId": fx.workspace, "cookie": {
        "id": reqly_core::Id::new(), "domain": "a.test", "includeSubdomains": false, "path": "/",
        "secure": false, "httpOnly": false, "expires": null, "name": "sid", "value": "1" } }),
    )
    .await;

    let out = fx
        .export(
            json!({ "kind": "workspace", "workspaceId": fx.workspace }),
            json!({ "secrets": true, "history": true, "cookies": true }),
        )
        .await;
    let content = out["content"].as_str().unwrap();
    assert!(content.contains("hunter2"));

    let other = Fx::new().await;
    let result = other
        .import(content, json!({ "kind": "newCollections" }))
        .await;
    let col = result["collectionIds"][0].clone();
    let nodes = other
        .call("tree.list", json!({ "collectionId": col }))
        .await;
    let echo = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "Echo")
        .unwrap()["id"]
        .clone();
    let history = other
        .call("response.list", json!({ "requestId": echo }))
        .await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    let body = other
        .call(
            "response.body",
            json!({ "id": history[0]["id"], "offset": 0, "limit": 100000 }),
        )
        .await;
    assert!(!body["base64"].as_str().unwrap().is_empty());
    let cookies = other
        .call("cookies.list", json!({ "workspaceId": other.workspace }))
        .await;
    assert_eq!(cookies[0]["name"], "sid");
}

#[tokio::test]
async fn merge_and_replace_into_an_existing_collection() {
    let fx = Fx::new().await;
    fx.sample().await;
    let out = fx
        .export(
            json!({ "kind": "collection", "collectionId": fx.collection }),
            json!({ "environments": false }),
        )
        .await;
    let content = out["content"].as_str().unwrap().to_string();

    // Local edits: a new request and a changed one.
    fx.node(
        Value::Null,
        "Local",
        json!({ "kind": "request", "url": "/local" }),
    )
    .await;
    let before = fx.tree(&fx.collection).await;

    // Merge: same names are updated in place, nothing is duplicated.
    fx.import(
        &content,
        json!({ "kind": "collection", "collectionId": fx.collection, "strategy": "merge" }),
    )
    .await;
    assert_eq!(fx.tree(&fx.collection).await, before);

    // Append duplicates everything.
    fx.import(
        &content,
        json!({ "kind": "collection", "collectionId": fx.collection, "strategy": "append" }),
    )
    .await;
    assert_eq!(fx.tree(&fx.collection).await.len(), before.len() + 3);

    // Replace moves old items to the trash.
    fx.import(
        &content,
        json!({ "kind": "collection", "collectionId": fx.collection, "strategy": "replace" }),
    )
    .await;
    let names: Vec<_> = fx
        .tree(&fx.collection)
        .await
        .into_iter()
        .map(|t| t.0)
        .collect();
    assert_eq!(names, ["Users", "Login", "Me"]);
    let trash = fx
        .call("trash.list", json!({ "collectionId": fx.collection }))
        .await;
    assert!(!trash.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn export_selected_nodes() {
    let fx = Fx::new().await;
    let (login, me) = fx.sample().await;
    let other = fx
        .node(
            Value::Null,
            "Other",
            json!({ "kind": "request", "url": "/o" }),
        )
        .await;
    let out = fx
        .export(
            json!({ "kind": "nodes", "nodeIds": [login, me, other] }),
            json!({}),
        )
        .await;
    let doc: Value = serde_json::from_str(out["content"].as_str().unwrap()).unwrap();
    let names: Vec<_> = doc["collections"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].clone())
        .collect();
    assert_eq!(names, [json!("Login"), json!("Me"), json!("Other")]);
}

#[tokio::test]
async fn import_errors() {
    let fx = Fx::new().await;
    let err = registry()
        .dispatch(
            &fx.ctx,
            "import.preview",
            json!({ "sources": [{ "name": "a.txt", "content": "hello" }] }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "import.unknown_format");
}

#[tokio::test]
async fn import_from_url() {
    let server = TestServer::start().await;
    let fx = Fx::new().await;
    let out = fx
        .call("import.fromUrl", json!({ "url": server.url("/echo?x=1") }))
        .await;
    assert_eq!(out["name"], "echo");
    assert!(out["content"].as_str().unwrap().contains("\"method\""));
}

#[tokio::test]
async fn postman_export_has_environment_files() {
    let fx = Fx::new().await;
    fx.sample().await;
    let out = fx
        .call(
            "export.run",
            json!({ "scope": { "kind": "collection", "collectionId": fx.collection }, "format": "postman" }),
        )
        .await;
    let names: Vec<_> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "my-collection.postman_collection.json",
            "my-collection.prod.postman_environment.json"
        ]
    );
}

#[tokio::test]
async fn code_generation() {
    let fx = Fx::new().await;
    let base = fx
        .call("environment.list", json!({ "collectionId": fx.collection }))
        .await["collection"][0]["id"]
        .clone();
    fx.call(
        "environment.update",
        json!({ "id": base, "data": { "host": "https://api.test" } }),
    )
    .await;
    let req = fx
        .node(Value::Null, "r", json!({ "kind": "request", "method": "POST", "url": "{{ host }}/x",
            "headers": [{ "name": "X-A", "value": "1" }], "body": { "kind": "json", "text": "{\"a\":1}" } }))
        .await;
    let targets = fx.call("codegen.targets", json!({})).await;
    assert!(targets.as_array().unwrap().len() > 20);
    let out = fx
        .call(
            "codegen.generate",
            json!({ "requestId": req, "target": "pythonRequests" }),
        )
        .await;
    let code = out["code"].as_str().unwrap();
    assert!(code.contains("url = \"https://api.test/x\""), "{code}");
    let raw = fx
        .call(
            "codegen.generate",
            json!({ "requestId": req, "target": "curlBash", "resolve": false }),
        )
        .await;
    assert!(
        raw["code"].as_str().unwrap().contains("{{ host }}/x"),
        "{raw}"
    );
}
