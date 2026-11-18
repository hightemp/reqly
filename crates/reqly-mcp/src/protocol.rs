//! JSON-RPC messages of MCP: lifecycle, ping and tools.

use serde_json::{Value, json};

use crate::Session;
use crate::tools::{ToolContext, Tools};

/// The newest MCP revision this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26"];

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "Reqly is an API client. Collections hold folders and requests \
(HTTP, GraphQL, WebSocket, gRPC…), organized in workspaces; environments hold variables used \
as {{name}} in requests. Find ids with the list and search tools before changing or sending \
anything. Deleted items go to the trash and can be restored in the app.";

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

pub async fn handle(tools: &Tools, cx: ToolContext, session: &Session, message: Value) -> Reply {
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
    cx: ToolContext,
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
    let outcome = tool.run(cx, args).await;
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
