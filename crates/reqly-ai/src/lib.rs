//! `reqly-ai`: a client for OpenAI-compatible chat completions (ADR-015).
//!
//! Builds `chat/completions` requests with tools, parses the streamed answer
//! (text, reasoning, tool calls, usage) and lists models. It does no I/O of
//! its own: the app gives it a [`Transport`] (the `reqly-http` engine with
//! the network settings; a replay in tests).

mod wire;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where a provider lives and how to authenticate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// The API root, e.g. `https://api.openai.com/v1`.
    pub base_url: String,
    /// Sent as `Authorization: Bearer`; none for a local Ollama.
    pub api_key: Option<String>,
    /// Extra headers (OpenRouter's `X-Title`, a proxy's own key…).
    pub headers: Vec<(String, String)>,
}

impl Provider {
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url.trim_end_matches('/'))
    }

    fn headers(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(key) = self.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            out.push(("Authorization".into(), format!("Bearer {}", key.trim())));
        }
        out.extend(self.headers.iter().cloned());
        out
    }
}

/// One HTTP exchange the client asks the transport for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// What a transport reports while a response arrives.
pub enum Chunk<'a> {
    /// The status code, before any data (again after a redirect).
    Status(u16),
    Data(&'a [u8]),
}

pub type ChunkSink = Arc<dyn Fn(Chunk<'_>) + Send + Sync>;

/// Performs HTTP calls. The implementation owns cancellation: it ends the
/// call with an `ai.cancelled` error when its turn is stopped.
pub trait Transport: Send + Sync {
    fn send(&self, call: HttpCall, sink: ChunkSink) -> BoxFuture<'_, CoreResult<()>>;
}

/// A chat message, in the roles of the Chat Completions API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON text, as the model wrote it (it may be invalid).
    pub arguments: String,
}

/// A function the model may call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments.
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// What the model produced in one call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Completion {
    pub content: String,
    /// "Thinking" text of reasoning models, kept apart from the answer.
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
}

/// Progress of a streamed answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Text(String),
    Reasoning(String),
    /// The model started a tool call (its arguments are still arriving).
    ToolCall {
        name: String,
    },
}

/// Bytes of a response kept to report an error or parse a plain JSON answer.
const MAX_BUFFER: usize = 4 * 1024 * 1024;

/// Sends `request` with streaming and returns the full answer; `on_delta`
/// sees the text as it arrives.
pub async fn chat(
    transport: &dyn Transport,
    provider: &Provider,
    request: &ChatRequest,
    on_delta: impl Fn(Delta) + Send + Sync + 'static,
) -> CoreResult<Completion> {
    let body = wire::request_body(request);
    let mut headers = provider.headers();
    headers.push(("Content-Type".into(), "application/json".into()));
    headers.push(("Accept".into(), "text/event-stream".into()));
    let call = HttpCall {
        method: "POST",
        url: provider.url("chat/completions"),
        headers,
        body: Some(body.to_string().into_bytes()),
    };
    let state = Arc::new(Mutex::new(wire::StreamState::new(Box::new(on_delta))));
    let sink_state = state.clone();
    let sink: ChunkSink = Arc::new(move |chunk| {
        if let Ok(mut s) = sink_state.lock() {
            s.chunk(chunk);
        }
    });
    let sent = transport.send(call, sink).await;
    let mut state = state
        .lock()
        .map_err(|_| CoreError::internal("stream state poisoned"))?;
    sent.map_err(network)?;
    state.finish()
}

/// Model ids the provider lists (`GET /models`), sorted.
pub async fn models(transport: &dyn Transport, provider: &Provider) -> CoreResult<Vec<String>> {
    let collected = Arc::new(Mutex::new((0u16, Vec::<u8>::new())));
    let sink_collected = collected.clone();
    let sink: ChunkSink = Arc::new(move |chunk| {
        if let Ok(mut c) = sink_collected.lock() {
            match chunk {
                Chunk::Status(s) => {
                    c.0 = s;
                    c.1.clear();
                }
                Chunk::Data(d) if c.1.len() < MAX_BUFFER => c.1.extend_from_slice(d),
                Chunk::Data(_) => {}
            }
        }
    });
    let mut headers = provider.headers();
    headers.push(("Accept".into(), "application/json".into()));
    transport
        .send(
            HttpCall {
                method: "GET",
                url: provider.url("models"),
                headers,
                body: None,
            },
            sink,
        )
        .await
        .map_err(network)?;
    let (status, body) = collected
        .lock()
        .map(|c| c.clone())
        .map_err(|_| CoreError::internal("models state poisoned"))?;
    if !(200..300).contains(&status) {
        return Err(wire::http_error(status, &body));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| provider_error(status, "the provider did not answer with a model list"))?;
    let mut ids: Vec<String> = value["data"]
        .as_array()
        .or_else(|| value["models"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
        .map(str::to_string)
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// A transport failure; `ai.*` errors of the transport (`ai.cancelled`)
/// pass through.
fn network(e: CoreError) -> CoreError {
    if e.code.starts_with("ai.") {
        return e;
    }
    CoreError::new("ai.network", e.message.clone())
        .with_param("message", e.message)
        .with_param("cause", e.code)
}

fn provider_error(status: u16, message: &str) -> CoreError {
    CoreError::new("ai.provider", message.to_string())
        .with_param("status", status)
        .with_param("message", message)
}

/// The JSON of a tool's result for a `tool` message.
pub fn tool_result_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A short error payload for the model when a tool fails.
pub fn tool_error_text(code: &str, message: &str) -> String {
    json!({ "error": { "code": code, "message": message } }).to_string()
}
