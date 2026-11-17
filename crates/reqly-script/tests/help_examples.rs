//! Every `js` example of the in-app help pages about scripts runs without
//! errors and its tests pass (ui/src/features/help/content/*/script*.md).
//! Examples that read `rq.response` / `pm.response` run after a response.

use std::sync::Arc;

use reqly_script::{
    Limits, Pair, Phase, Scopes, ScriptBody, ScriptHost, ScriptInfo, ScriptInput, ScriptRequest,
    ScriptResponse, SendSpec, run,
};
use serde_json::json;

struct Host;

impl ScriptHost for Host {
    async fn send(&self, request: SendSpec) -> ScriptResponse {
        ScriptResponse {
            code: 200,
            status: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: json!({
                "access_token": "t-123",
                "expires_in": 3600,
                "items": [{ "id": 1 }],
                "next": null,
                "echo": request.url,
            })
            .to_string(),
            time_ms: 5.0,
            size: 80,
            error: None,
        }
    }
}

fn pair(key: &str, value: &str) -> Pair {
    Pair {
        key: key.into(),
        value: value.into(),
        disabled: false,
    }
}

fn input(phase: Phase, script: &str) -> ScriptInput {
    let body = json!({
        "token": "abc",
        "user": { "id": 42, "name": "Ada", "email": "ada@example.com", "roles": ["admin", "dev"] },
        "items": [
            { "id": 1, "name": "Pen", "price": 2.5, "status": "paid", "createdAt": "2026-11-01T10:00:00Z" },
            { "id": 2, "name": "Book", "price": 12, "status": "new", "createdAt": "2026-11-02T10:00:00Z" },
            { "id": 3, "name": "Lamp", "price": 30, "status": "paid", "createdAt": "2026-11-03T10:00:00Z" }
        ],
        "total": 3,
        "next": null
    });
    ScriptInput {
        phase,
        label: "help example".into(),
        script: script.into(),
        request: ScriptRequest {
            id: "r1".into(),
            name: "Create order".into(),
            method: "POST".into(),
            url: "{{base_url}}/orders".into(),
            query: vec![pair("page", "1")],
            headers: vec![
                pair("Accept", "application/json"),
                pair("Content-Type", "application/json"),
            ],
            body: ScriptBody {
                mode: "raw".into(),
                raw: json!({ "item": 1, "qty": 2 }).to_string(),
                fields: vec![],
            },
            auth: json!({ "type": "none" }),
        },
        response: (phase == Phase::Post).then(|| ScriptResponse {
            code: 200,
            status: "OK".into(),
            headers: vec![
                (
                    "Content-Type".into(),
                    "application/json; charset=utf-8".into(),
                ),
                ("X-Request-Id".into(), "req-7".into()),
                ("Date".into(), "Tue, 17 Nov 2026 10:00:00 GMT".into()),
            ],
            body: body.to_string(),
            time_ms: 120.0,
            size: body.to_string().len() as u64,
            error: None,
        }),
        variables: Scopes {
            globals: serde_json::from_value(json!({ "app": "reqly" })).unwrap(),
            collection: serde_json::from_value(json!({ "base_url": "https://api.example.com" }))
                .unwrap(),
            environment: serde_json::from_value(json!({
                "client_id": "my-client",
                "client_secret": "s3cr3t",
                "api_secret": "k3y",
                "username": "ada",
                "password": "pa55",
            }))
            .unwrap(),
            data: serde_json::from_value(json!({ "email": "ada@example.com", "qty": 2 })).unwrap(),
            ..Scopes::default()
        },
        info: ScriptInfo {
            request_name: "Create order".into(),
            request_id: "r1".into(),
            iteration: 0,
            iteration_count: 1,
        },
        limits: Limits::default(),
        locale: reqly_template::Locale::EN,
    }
}

/// `js` code blocks of a Markdown page.
fn examples(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```js") => current = Some(Vec::new()),
            (Some(lines), "```") => {
                out.push(lines.join("\n"));
                current = None;
            }
            (Some(lines), _) => lines.push(line),
            _ => {}
        }
    }
    out
}

#[tokio::test]
async fn script_help_examples_run() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ui/src/features/help/content"
    );
    let mut checked = 0;
    for locale in ["en", "ru"] {
        for page in ["scripts", "script-api", "script-libraries"] {
            let path = format!("{dir}/{locale}/{page}.md");
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            for (i, code) in examples(&text).into_iter().enumerate() {
                let post = code.contains("rq.response")
                    || code.contains("pm.response")
                    || code.contains("responseBody");
                let phase = if post { Phase::Post } else { Phase::Pre };
                let out = run(Arc::new(Host), input(phase, &code))
                    .await
                    .unwrap_or_else(|e| panic!("{locale}/{page} example {}: {e:?}\n{code}", i + 1));
                for t in &out.tests {
                    assert!(
                        t.passed,
                        "{locale}/{page} example {}: test {:?} failed: {:?}\n{code}",
                        i + 1,
                        t.name,
                        t.error
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 30, "only {checked} examples found");
}
