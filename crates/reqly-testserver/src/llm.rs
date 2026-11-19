//! A fake OpenAI-compatible provider for the AI chat (ADR-015): scripted
//! `chat/completions` answers streamed as SSE, `GET /models`, an optional
//! required API key, and a record of every request.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

/// One scripted answer.
#[derive(Debug, Clone)]
pub enum LlmReply {
    /// Streamed word by word.
    Text(String),
    /// Like `Text`, waiting `delay` before each word.
    SlowText(String, Duration),
    /// Tool calls: name and arguments.
    ToolCalls(Vec<(String, Value)>),
    /// An HTTP error with this status and body.
    Error(u16, String),
}

/// What the fake received.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl LlmRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Default)]
struct Inner {
    replies: VecDeque<LlmReply>,
    requests: Vec<LlmRequest>,
    key: Option<String>,
    models: Vec<String>,
}

#[derive(Clone, Default)]
struct LlmState(Arc<Mutex<Inner>>);

pub struct FakeLlm {
    addr: SocketAddr,
    state: LlmState,
    task: JoinHandle<()>,
}

impl FakeLlm {
    pub async fn start() -> Self {
        let state = LlmState::default();
        state.0.lock().unwrap().models = vec!["fake-small".into(), "fake-large".into()];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let app = Router::new()
            .route("/v1/chat/completions", post(completions))
            .route("/v1/models", get(models))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        Self { addr, state, task }
    }

    /// The API root to configure as a provider's base URL.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.addr.port())
    }

    /// Queues the answer to the next `chat/completions` call (without one,
    /// the fake answers "ok").
    pub fn push(&self, reply: LlmReply) {
        self.state.0.lock().unwrap().replies.push_back(reply);
    }

    /// Requires `Authorization: Bearer <key>`.
    pub fn require_key(&self, key: &str) {
        self.state.0.lock().unwrap().key = Some(key.to_string());
    }

    pub fn set_models(&self, models: &[&str]) {
        self.state.0.lock().unwrap().models = models.iter().map(|m| m.to_string()).collect();
    }

    pub fn requests(&self) -> Vec<LlmRequest> {
        self.state.0.lock().unwrap().requests.clone()
    }

    /// Bodies of the `chat/completions` calls.
    pub fn chats(&self) -> Vec<Value> {
        self.requests()
            .into_iter()
            .filter(|r| r.path.ends_with("/chat/completions"))
            .map(|r| r.body)
            .collect()
    }
}

impl Drop for FakeLlm {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn record(state: &LlmState, path: &str, headers: &HeaderMap, body: Value) -> Option<Response> {
    let mut inner = state.0.lock().unwrap();
    inner.requests.push(LlmRequest {
        path: path.to_string(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect(),
        body,
    });
    let key = inner.key.clone()?;
    let sent = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    (sent != Some(key.as_str())).then(|| {
        error(
            401,
            r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#,
        )
    })
}

fn error(status: u16, body: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

async fn models(State(state): State<LlmState>, headers: HeaderMap) -> Response {
    if let Some(denied) = record(&state, "/v1/models", &headers, Value::Null) {
        return denied;
    }
    let models = state.0.lock().unwrap().models.clone();
    let data: Vec<Value> = models
        .iter()
        .map(|m| json!({ "id": m, "object": "model", "owned_by": "fake" }))
        .collect();
    axum::Json(json!({ "object": "list", "data": data })).into_response()
}

fn chunk(model: &str, delta: Value, finish: Option<&str>) -> Bytes {
    let value = json!({
        "id": "chatcmpl-fake",
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
    });
    Bytes::from(format!("data: {value}\n\n"))
}

async fn completions(State(state): State<LlmState>, headers: HeaderMap, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let model = body["model"].as_str().unwrap_or("fake").to_string();
    if let Some(denied) = record(&state, "/v1/chat/completions", &headers, body) {
        return denied;
    }
    let reply = state
        .0
        .lock()
        .unwrap()
        .replies
        .pop_front()
        .unwrap_or_else(|| LlmReply::Text("ok".into()));
    let (words, delay, calls) = match reply {
        LlmReply::Error(status, body) => return error(status, &body),
        LlmReply::Text(text) => (split_words(&text), Duration::ZERO, Vec::new()),
        LlmReply::SlowText(text, delay) => (split_words(&text), delay, Vec::new()),
        LlmReply::ToolCalls(calls) => (Vec::new(), Duration::ZERO, calls),
    };
    let mut parts: Vec<(Duration, Bytes)> = vec![(
        Duration::ZERO,
        chunk(&model, json!({ "role": "assistant", "content": "" }), None),
    )];
    for w in words {
        parts.push((delay, chunk(&model, json!({ "content": w }), None)));
    }
    for (i, (name, args)) in calls.iter().enumerate() {
        let id = format!("call_{}", i + 1);
        // Name first, arguments in two pieces, like OpenAI.
        let text = args.to_string();
        let (a, b) = text.split_at(text.len() / 2);
        parts.push((
            Duration::ZERO,
            chunk(
                &model,
                json!({ "tool_calls": [{ "index": i, "id": id, "type": "function", "function": { "name": name, "arguments": "" } }] }),
                None,
            ),
        ));
        for piece in [a, b] {
            parts.push((
                Duration::ZERO,
                chunk(
                    &model,
                    json!({ "tool_calls": [{ "index": i, "function": { "arguments": piece } }] }),
                    None,
                ),
            ));
        }
    }
    let finish = if calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    parts.push((Duration::ZERO, chunk(&model, json!({}), Some(finish))));
    let usage = json!({ "id": "chatcmpl-fake", "object": "chat.completion.chunk", "model": model, "choices": [],
        "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 } });
    parts.push((Duration::ZERO, Bytes::from(format!("data: {usage}\n\n"))));
    parts.push((Duration::ZERO, Bytes::from_static(b"data: [DONE]\n\n")));

    let stream = futures::stream::unfold(parts.into_iter(), |mut it| async move {
        let (delay, bytes) = it.next()?;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        Some((Ok::<_, Infallible>(bytes), it))
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Words with their following space, so they join back to the text.
fn split_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if c == ' ' {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
