//! JSON-RPC messages of MCP: lifecycle, ping and tools.

use std::time::Instant;

use reqly_api::commands::mcp::McpCall;

use reqly_api::agent::secret_name;
use serde_json::{Map, Value, json};

use crate::Session;
use crate::{ToolContext, Tools};

/// The newest MCP revision this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26"];

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "Reqly is an API client (like Insomnia or Postman); this server \
works on the user's own requests. You do not need Reqly's source code: everything is available \
through these tools, and get_docs returns Reqly's documentation by topic (call it without \
arguments for the list; `variables` covers templates, `scripts` and `script-api` cover scripts). \
Workspaces hold collections; collections hold folders and requests (HTTP, GraphQL, WebSocket, \
gRPC). Find ids with list_collections, get_tree and search_requests, then read a request with \
get_request {\"id\": ...}; users may paste a block with a request's path and ID. Templates: \
{{name}} inserts a variable (environments hold them), {{ $uuid }} or {{ $timestamp }} call a \
function, {% tag args %} runs a tag, e.g. {% response 'body', '<request id>', '$.token' %} \
takes a value from the latest response of another request. Credentials are masked as ••••. \
Deleted items go to the trash and can be restored in the app.";

/// What to answer to one message.
pub enum Reply {
    /// A response object (result or error).
    Message(Value),
    /// A JSON-RPC error that is the client's fault before any method ran
    /// (`400 Bad Request`).
    Invalid(Value),
    /// Notifications and client responses: nothing to send (`202`).
    Accepted,
}

pub fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub async fn handle(
    tools: &Tools,
    cx: ToolContext<'_>,
    session: &Session,
    message: Value,
) -> Reply {
    let Value::Object(msg) = message else {
        return Reply::Invalid(error(
            Value::Null,
            INVALID_REQUEST,
            "expected one JSON-RPC message object",
        ));
    };
    let id = msg.get("id").cloned();
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Reply::Invalid(error(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "jsonrpc must be \"2.0\"",
        ));
    }
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // A response to a server request; this server sends none.
        return Reply::Accepted;
    };
    let Some(id) = id else {
        return Reply::Accepted;
    };
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    Reply::Message(match method {
        "initialize" => result(id, initialize(&params)),
        "ping" => result(id, json!({})),
        "tools/list" => result(
            id,
            json!({
                "tools": tools
                    .iter()
                    .filter(|t| session.allowed.permits(t.name))
                    .map(|t| t.describe())
                    .collect::<Vec<_>>()
            }),
        ),
        "tools/call" => call_tool(tools, cx, session, id, &params).await,
        _ => error(id, METHOD_NOT_FOUND, &format!("method not found: {method}")),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| SUPPORTED_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "reqly",
            "title": "Reqly",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

async fn call_tool(
    tools: &Tools,
    cx: ToolContext<'_>,
    session: &Session,
    id: Value,
    params: &Value,
) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(tool) = tools.get(name).filter(|_| session.allowed.permits(name)) else {
        return error(id, INVALID_PARAMS, &format!("unknown tool: {name}"));
    };
    let args = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(a) => a.clone(),
    };
    let started = Instant::now();
    let at = cx.ctx.clock.now().0;
    let ctx = cx.ctx;
    let user = cx.caller.user_id();
    let arguments = summary(&args);
    let outcome = tool.run(cx, args).await;
    ctx.mcp_log.record(
        &ctx.events,
        McpCall {
            at,
            tool: name.to_string(),
            arguments,
            ok: outcome.is_ok(),
            error: outcome
                .as_ref()
                .err()
                .map(|e| format!("{}: {}", e.code, e.message)),
            duration_ms: started.elapsed().as_millis() as u64,
            user,
        },
    );
    result(
        id,
        match outcome {
            Ok(value) => {
                let structured = if value.is_object() {
                    value
                } else {
                    json!({ "result": value })
                };
                json!({
                    "content": [{ "type": "text", "text": structured.to_string() }],
                    "structuredContent": structured,
                    "isError": false,
                })
            }
            Err(e) => {
                let mut text = format!("{}: {}", e.code, e.message);
                if !e.params.is_empty() {
                    text.push_str(&format!(" {}", Value::Object(e.params)));
                }
                json!({ "content": [{ "type": "text", "text": text }], "isError": true })
            }
        },
    )
}

const SUMMARY_CHARS: usize = 300;
const HIDDEN: &str = "••••";

/// Arguments for the call log: secrets hidden, cut to a few hundred chars.
fn summary(args: &Value) -> String {
    let text = redact(args).to_string();
    if text.chars().count() <= SUMMARY_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(SUMMARY_CHARS).collect();
    cut.push('…');
    cut
}

/// Variable values (`values`) and anything named like a credential.
fn redact(value: &Value) -> Value {
    match value {
        // A header or parameter row: `{ "key": "Authorization", "value": … }`.
        Value::Object(map)
            if map
                .get("key")
                .or_else(|| map.get("name"))
                .and_then(Value::as_str)
                .is_some_and(secret_name)
                && map.contains_key("value") =>
        {
            let mut map = map.clone();
            map.insert("value".into(), Value::String(HIDDEN.into()));
            Value::Object(map)
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = if k == "values" {
                        hide_all(v)
                    } else if secret_name(k) {
                        Value::String(HIDDEN.into())
                    } else {
                        redact(v)
                    };
                    (k.clone(), v)
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

/// Keeps the keys of an object, hides every value.
fn hide_all(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.keys()
                .map(|k| (k.clone(), Value::String(HIDDEN.into())))
                .collect(),
        ),
        _ => Value::String(HIDDEN.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_and_variable_values() {
        let out = redact(&json!({
            "environmentId": "e1",
            "values": { "base_url": "https://x", "token": "t" },
            "request": { "auth": { "type": "bearer", "token": "abc", "username": "u" } },
            "headers": [{ "key": "X-Api-Key", "value": "v" }],
            "clientSecret": "s",
        }));
        assert_eq!(out["environmentId"], "e1");
        assert_eq!(
            out["values"],
            json!({ "base_url": HIDDEN, "token": HIDDEN })
        );
        assert_eq!(out["request"]["auth"]["token"], HIDDEN);
        assert_eq!(out["request"]["auth"]["username"], "u");
        assert_eq!(out["request"]["auth"]["type"], "bearer");
        assert_eq!(out["clientSecret"], HIDDEN);
        assert_eq!(
            out["headers"][0],
            json!({ "key": "X-Api-Key", "value": HIDDEN })
        );
        let row = redact(&json!({ "key": "Accept", "value": "*/*" }));
        assert_eq!(row["value"], "*/*");
    }

    #[test]
    fn cuts_long_summaries() {
        let s = summary(&json!({ "text": "é".repeat(500) }));
        assert_eq!(s.chars().count(), SUMMARY_CHARS + 1);
        assert!(s.ends_with('…'));
    }
}
