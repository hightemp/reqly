//! GraphQL requests and schemas through the command registry.

use reqly_api::commands::{AppContext, registry};
use reqly_testserver::TestServer;
use serde_json::{Value, json};

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
    registry().dispatch(ctx, name, input).await
}

async fn setup(server: &TestServer) -> (AppContext, Value) {
    let ctx = AppContext::for_tests().await;
    let ws = call(&ctx, "workspace.ensureDefault", json!({}))
        .await
        .unwrap()[0]["id"]
        .clone();
    let collection = call(&ctx, "collection.list", json!({ "workspaceId": ws }))
        .await
        .unwrap()[0]["id"]
        .clone();
    let req = call(
        &ctx,
        "node.create",
        json!({ "collectionId": collection, "parentId": null, "name": "gql", "data": {
            "kind": "request", "method": "POST", "url": server.url("/graphql"),
            "headers": [{ "name": "X-Token", "value": "{{ token }}" }],
            "variables": { "token": "t-1", "uid": 1 },
            "body": { "kind": "graphql", "graphql": {
                "query": "query($id: Int!) { user(id: $id) { name } header(name: \"x-token\") }",
                "variables": "{ \"id\": {{ uid }} }"
            } }
        } }),
    )
    .await
    .unwrap()["id"]
        .clone();
    (ctx, req)
}

#[tokio::test]
async fn sends_with_templates_in_variables() {
    let server = TestServer::start().await;
    let (ctx, req) = setup(&server).await;
    let record = call(&ctx, "request.send", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(record["status"], 200);
    let chunk = call(
        &ctx,
        "response.body",
        json!({ "id": record["id"], "offset": 0, "limit": 100000 }),
    )
    .await
    .unwrap();
    use base64::Engine;
    let body: Value = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(chunk["base64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["data"]["user"]["name"], "Alice");
    assert_eq!(body["data"]["header"], "t-1");
}

#[tokio::test]
async fn introspection_is_cached_and_files_are_accepted() {
    let server = TestServer::start().await;
    let (ctx, req) = setup(&server).await;
    assert!(
        call(&ctx, "graphql.schemaGet", json!({ "requestId": req }))
            .await
            .unwrap()["schema"]
            .is_null()
    );
    let schema = call(&ctx, "graphql.introspect", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(schema["format"], "introspection");
    let parsed: Value = serde_json::from_str(schema["schema"].as_str().unwrap()).unwrap();
    assert_eq!(parsed["__schema"]["queryType"]["name"], "Query");
    assert_eq!(
        parsed["__schema"]["subscriptionType"]["name"],
        "Subscription"
    );
    let cached = call(&ctx, "graphql.schemaGet", json!({ "requestId": req }))
        .await
        .unwrap();
    assert_eq!(cached["schema"]["source"], schema["source"]);

    let sdl = call(
        &ctx,
        "graphql.schemaSet",
        json!({ "requestId": req, "format": "sdl", "source": "schema.graphql", "schema": "type Query { a: Int }" }),
    )
    .await
    .unwrap();
    assert_eq!(sdl["format"], "sdl");
    let bad = call(
        &ctx,
        "graphql.schemaSet",
        json!({ "requestId": req, "format": "introspection", "source": "x.json", "schema": "{}" }),
    )
    .await
    .unwrap_err();
    assert_eq!(bad.code, "graphql.invalid_schema");
    call(&ctx, "graphql.schemaClear", json!({ "requestId": req }))
        .await
        .unwrap();
    assert!(
        call(&ctx, "graphql.schemaGet", json!({ "requestId": req }))
            .await
            .unwrap()["schema"]
            .is_null()
    );
}

#[tokio::test]
async fn introspection_errors_are_reported() {
    let server = TestServer::start().await;
    let (ctx, req) = setup(&server).await;
    call(&ctx, "node.update", json!({ "id": req, "data": { "kind": "request", "method": "POST", "url": server.url("/status/404") } }))
        .await
        .unwrap();
    let err = call(&ctx, "graphql.introspect", json!({ "requestId": req }))
        .await
        .unwrap_err();
    assert_eq!(err.code, "graphql.introspection_failed");
}
