//! Run tools: send a request, render it, run a collection.

use reqly_core::{CoreError, CoreResult, Id};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::read::{MAX_BODY_BYTES, body_limit, response_view};
use super::{args, invalid, mask, tool};
use crate::agent::{Annotations, Tool, ToolContext};

/// Collection runs list at most this many items.
const MAX_RUN_ITEMS: usize = 200;

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "send_request",
            "Send a saved request and return the response (status, headers, timing, body, test \
             and assertion results). The response is stored in the history like a send from \
             the app. By default the active environment is used; environmentId and variables \
             apply to this send only and change nothing. Secret values are masked in the \
             result.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Request id." },
                    "environmentId": { "type": "string", "description": "Use this environment instead of the active one." },
                    "variables": { "type": "object", "description": "Variables for this send only; they override every environment." },
                    "promptValues": { "type": "object", "description": "Answers to {% prompt %} tags: title → value." },
                    "maxBodyBytes": { "type": "integer", "minimum": 0, "maximum": MAX_BODY_BYTES, "default": super::read::DEFAULT_BODY_BYTES }
                },
                "required": ["id"]
            }),
            Annotations::default().open_world(),
            |cx, a| Box::pin(async move { send_request(cx, args(a)?).await }),
        ),
        tool(
            "render_request",
            "Show a request as it would be sent with the active environment: templates in the \
             URL, enabled params and headers and the body are replaced by their values. \
             Nothing is sent. Secret values are masked.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { render_request(cx, args(a)?).await }),
        ),
        tool(
            "run_collection",
            "Run the requests of a collection, of one folder or a chosen list, like the \
             Collection Runner, and report each result with its tests and assertions. Data \
             rows (an array of objects) give the variables of each iteration.",
            json!({
                "type": "object",
                "properties": {
                    "collectionId": { "type": "string" },
                    "folderId": { "type": "string" },
                    "requestIds": { "type": "array", "items": { "type": "string" } },
                    "environmentId": { "type": "string" },
                    "iterations": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 1 },
                    "data": { "type": "array", "items": { "type": "object" }, "description": "One object of variables per iteration." },
                    "delayMs": { "type": "integer", "minimum": 0 },
                    "stopOnFailure": { "type": "boolean" }
                },
                "required": ["collectionId"]
            }),
            Annotations::default().open_world(),
            |cx, a| Box::pin(async move { run_collection(cx, args(a)?).await }),
        ),
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendArgs {
    id: String,
    environment_id: Option<String>,
    variables: Option<Map<String, Value>>,
    #[serde(default)]
    prompt_values: Map<String, Value>,
    max_body_bytes: Option<u64>,
}

fn error_of(value: &Value) -> CoreError {
    let code = value["code"].as_str().unwrap_or("http.error");
    let message = value["message"].as_str().unwrap_or("request failed");
    CoreError::new(code, message)
}

async fn send_request(cx: ToolContext<'_>, a: SendArgs) -> CoreResult<Value> {
    let node = cx.call("node.get", json!({ "id": a.id })).await?;
    let scrub = mask::Scrub::for_request(&cx, &node).await?;
    let record = if a.environment_id.is_none() && a.variables.is_none() {
        cx.call(
            "request.send",
            json!({ "requestId": a.id, "promptValues": a.prompt_values }),
        )
        .await?
    } else {
        // A one-request run: its environment and data do not touch the
        // user's active environment.
        let mut input = json!({
            "runId": Id::new().to_string(),
            "collectionId": node["collectionId"],
            "requestIds": [a.id],
        });
        if let Some(env) = a.environment_id {
            input["environmentId"] = json!(env);
        }
        if let Some(vars) = a.variables {
            input["data"] = json!({ "format": "json", "text": Value::Array(vec![Value::Object(vars)]).to_string() });
        }
        let report = cx.call("runner.run", input).await?;
        let item = &report["items"][0];
        match item["responseId"].as_str() {
            Some(id) => cx.call("response.get", json!({ "id": id })).await?,
            None => return Err(error_of(&item["error"])),
        }
    };
    if record["status"].is_null() && record["error"].is_object() {
        return Err(error_of(&record["error"]));
    }
    response_view(&cx, &record, body_limit(a.max_body_bytes), &scrub).await
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

async fn render_request(cx: ToolContext<'_>, a: IdArg) -> CoreResult<Value> {
    let node = cx.call("node.get", json!({ "id": a.id })).await?;
    let data = &node["data"];
    if node["data"]["kind"] == "folder" {
        return Err(invalid("a folder cannot be rendered"));
    }
    let render = |text: Value| {
        let collection = node["collectionId"].clone();
        let id = node["id"].clone();
        async move {
            let text = text.as_str().unwrap_or("").to_string();
            if !text.contains("{{") && !text.contains("{%") {
                return Ok::<Value, CoreError>(Value::String(text));
            }
            let out = cx
                .call(
                    "template.preview",
                    json!({ "collectionId": collection, "nodeId": id, "text": text }),
                )
                .await?;
            Ok(out["value"].clone())
        }
    };
    let rows = |list: &Value| {
        let list: Vec<Value> = list
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["enabled"] != false)
            .cloned()
            .collect();
        list
    };
    let mut out = json!({
        "method": data["method"],
        "url": render(data["url"].clone()).await?,
    });
    for key in ["params", "headers"] {
        let mut rendered = Vec::new();
        for row in rows(&data[key]) {
            rendered.push(json!({
                "name": render(row["name"].clone()).await?,
                "value": render(row["value"].clone()).await?,
            }));
        }
        out[key] = Value::Array(rendered);
    }
    let body = &data["body"];
    out["body"] = match body["kind"].as_str() {
        None | Some("none") => Value::Null,
        Some("formUrlencoded" | "multipart") => {
            let mut fields = Vec::new();
            for row in rows(&body["fields"]) {
                fields.push(json!({
                    "name": render(row["name"].clone()).await?,
                    "value": render(row["value"].clone()).await?,
                }));
            }
            Value::Array(fields)
        }
        Some("graphql") => json!({
            "query": render(body["graphql"]["query"].clone()).await?,
            "variables": render(body["graphql"]["variables"].clone()).await?,
        }),
        Some("binary") => json!({ "file": body["file"] }),
        Some(_) => render(body["text"].clone()).await?,
    };
    out["auth"] = json!({ "type": data["auth"]["type"] });
    mask::Scrub::for_request(&cx, &node).await?.apply(&mut out);
    Ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunArgs {
    collection_id: String,
    folder_id: Option<String>,
    request_ids: Option<Vec<String>>,
    environment_id: Option<String>,
    iterations: Option<u32>,
    data: Option<Vec<Value>>,
    delay_ms: Option<u64>,
    stop_on_failure: Option<bool>,
}

fn item_passed(item: &Value) -> bool {
    let tests_ok = item["tests"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|t| t["passed"] == true || t["skipped"] == true);
    let asserts_ok = item["assertions"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|t| t["passed"] == true);
    item["error"].is_null() && tests_ok && asserts_ok
}

async fn run_collection(cx: ToolContext<'_>, a: RunArgs) -> CoreResult<Value> {
    let mut input = json!({
        "runId": Id::new().to_string(),
        "collectionId": a.collection_id,
    });
    let optional = [
        ("folderId", a.folder_id.map(Value::from)),
        ("requestIds", a.request_ids.map(Value::from)),
        ("environmentId", a.environment_id.map(Value::from)),
        ("iterations", a.iterations.map(Value::from)),
        ("delayMs", a.delay_ms.map(Value::from)),
        ("stopOnFailure", a.stop_on_failure.map(Value::from)),
        (
            "data",
            a.data
                .map(|rows| json!({ "format": "json", "text": Value::Array(rows).to_string() })),
        ),
    ];
    for (k, v) in optional {
        if let Some(v) = v {
            input[k] = v;
        }
    }
    let report = cx.call("runner.run", input).await?;
    let all = report["items"].as_array().cloned().unwrap_or_default();
    let mut tests_passed = 0;
    let mut tests_failed = 0;
    let mut failed = 0;
    let items: Vec<Value> = all
        .iter()
        .map(|item| {
            let passed = item_passed(item);
            if !passed {
                failed += 1;
            }
            for t in item["tests"].as_array().into_iter().flatten() {
                if t["passed"] == true {
                    tests_passed += 1;
                } else if t["skipped"] != true {
                    tests_failed += 1;
                }
            }
            let path: Vec<&str> = item["path"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .chain(item["name"].as_str())
                .collect();
            json!({
                "iteration": item["iteration"],
                "requestId": item["requestId"],
                "name": item["name"],
                "path": path.join("/"),
                "method": item["method"],
                "url": item["url"],
                "status": item["status"],
                "totalMs": item["totalMs"],
                "responseId": item["responseId"],
                "passed": passed,
                "error": item["error"],
                "tests": item["tests"],
                "failedAssertions": item["assertions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|t| t["passed"] != true)
                    .cloned()
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let truncated = items.len() > MAX_RUN_ITEMS;
    Ok(json!({
        "name": report["name"],
        "environment": report["environment"],
        "iterations": report["iterations"],
        "passed": failed == 0 && report["cancelled"] != true,
        "cancelled": report["cancelled"],
        "stopped": report["stopped"],
        "summary": {
            "requests": all.len(),
            "failed": failed,
            "testsPassed": tests_passed,
            "testsFailed": tests_failed,
        },
        "items": items.into_iter().take(MAX_RUN_ITEMS).collect::<Vec<_>>(),
        "itemsTruncated": truncated,
    }))
}
