//! `reqly-cli run` against the test server, from files and from a Reqly server.

use reqly_testserver::TestServer;
use serde_json::{Value, json};

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn cli(args: &[&str]) -> Out {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let code = reqly_cli::main_with(args, &mut stdout, &mut stderr).await;
    Out {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

/// A Postman collection: `ok` at the root, `bad` (expects 201) in `Group`.
fn postman() -> Value {
    let test = |status: u16| {
        json!([{ "listen": "test", "script": { "exec": [
            format!("pm.test('status is {status}', () => pm.response.to.have.status({status}));")
        ] } }])
    };
    json!({
        "info": {
            "name": "Demo",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": [
            { "name": "ok", "event": test(200), "request": { "method": "GET", "url": "{{base}}/echo?user={{user}}" } },
            { "name": "Group", "item": [
                { "name": "bad", "event": test(201), "request": { "method": "GET", "url": "{{base}}/echo" } }
            ] }
        ]
    })
}

fn write(dir: &tempfile::TempDir, name: &str, text: &str) -> String {
    let path = dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    path.display().to_string()
}

#[tokio::test]
async fn runs_an_exported_collection_and_fails_on_failed_tests() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let file = write(&dir, "demo.postman_collection.json", &postman().to_string());
    let base = format!("base={}", server.url(""));

    let out = cli(&[
        "run",
        &file,
        "--var",
        &base,
        "--var",
        "user=ada",
        "--reporter",
        "json",
    ])
    .await;
    assert_eq!(out.code, 1, "{}", out.stderr);
    let report: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(report["name"], "Demo");
    assert_eq!(report["stats"]["requests"], 2);
    assert_eq!(report["stats"]["failedChecks"], 1);
    assert_eq!(
        report["items"][0]["url"],
        format!("{}/echo?user=ada", server.url(""))
    );

    let out = cli(&[
        "run", &file, "--var", &base, "--var", "user=x", "--folder", "Group",
    ])
    .await;
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("bad"), "{}", out.stdout);
    assert!(!out.stdout.contains(" ok "), "{}", out.stdout);
    assert!(out.stdout.contains("status is 201"), "{}", out.stdout);
}

#[tokio::test]
async fn data_iterations_bail_and_report_files() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let file = write(&dir, "demo.json", &postman().to_string());
    let data = write(&dir, "users.csv", "user\nada\nbob\n");
    let junit = dir.path().join("out.xml").display().to_string();
    let html = dir.path().join("out.html").display().to_string();
    let base = format!("base={}", server.url(""));

    let out = cli(&[
        "run", &file, "--var", &base, "-d", &data, "--report", &junit, "--report", &html,
    ])
    .await;
    assert_eq!(out.code, 1, "{}", out.stderr);
    // Pretty output: an iteration heading per data row and a summary.
    assert!(out.stdout.contains("Iteration 2"), "{}", out.stdout);
    assert!(out.stdout.contains("4 requests"), "{}", out.stdout);
    let xml = std::fs::read_to_string(&junit).unwrap();
    sxd_document::parser::parse(&xml).expect("well-formed JUnit");
    assert!(xml.contains("tests=\"4\""), "{xml}");
    assert!(
        std::fs::read_to_string(&html)
            .unwrap()
            .starts_with("<!doctype html>")
    );

    let out = cli(&[
        "run", &file, "--var", &base, "--var", "user=x", "-n", "3", "--bail", "-r", "json",
    ])
    .await;
    let report: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(report["stopped"], true);
    assert_eq!(report["items"].as_array().unwrap().len(), 2);

    let out = cli(&[
        "run", &file, "--var", &base, "-r", "junit", "--folder", "missing",
    ])
    .await;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("missing"), "{}", out.stderr);
}

#[tokio::test]
async fn runs_a_collection_from_a_server_with_a_token() {
    use reqly_server::{AppState, AuthConfig, AuthMode, router};

    let target = TestServer::start().await;
    let mut state = AppState::for_tests().await;
    state.auth = AuthConfig {
        mode: AuthMode::Token,
        token: Some("t0ken-for-ci".into()),
        ..AuthConfig::default()
    };
    let ctx = state.ctx.clone();
    let registry = reqly_api::commands::registry();
    let workspace = registry
        .dispatch(&ctx, "workspace.ensureDefault", json!({}))
        .await
        .unwrap()[0]["id"]
        .clone();
    let collection = registry
        .dispatch(
            &ctx,
            "collection.create",
            json!({ "workspaceId": workspace, "name": "Remote API" }),
        )
        .await
        .unwrap();
    registry
        .dispatch(
            &ctx,
            "node.create",
            json!({ "collectionId": collection["id"], "parentId": null, "name": "ping",
                    "data": { "kind": "request", "url": target.url("/echo"),
                              "assertions": [{ "source": "status", "operator": "equals", "value": "200" }] } }),
        )
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum_serve(listener, router(state)).await });

    let out = cli(&[
        "run",
        "--server",
        &url,
        "--token",
        "t0ken-for-ci",
        "--collection",
        "Remote API",
        "-r",
        "json",
    ])
    .await;
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let report: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(report["items"][0]["name"], "ping");

    let out = cli(&[
        "run",
        "--server",
        &url,
        "--token",
        "wrong",
        "--collection",
        "Remote API",
    ])
    .await;
    assert_eq!(out.code, 2);
    assert!(!out.stderr.is_empty());
}

async fn axum_serve(listener: tokio::net::TcpListener, app: axum::Router) {
    axum::serve(listener, app).await.unwrap();
}

#[tokio::test]
async fn usage_errors_exit_with_2() {
    let out = cli(&[]).await;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("Usage"), "{}", out.stderr);
    let out = cli(&["--help"]).await;
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("--reporter"));
    let out = cli(&["--version"]).await;
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains(env!("CARGO_PKG_VERSION")));
    let out = cli(&["run", "missing.json"]).await;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("missing.json"), "{}", out.stderr);
    let out = cli(&["run", "x.json", "--iterations", "many"]).await;
    assert_eq!(out.code, 2);
    let out = cli(&["run", "x.json", "--nope"]).await;
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("--nope"));
}
