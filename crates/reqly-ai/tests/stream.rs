//! Streaming parse of `chat/completions` (ADR-015, TASKS T14.2), through a
//! fake transport that replays recorded provider responses.

use std::sync::{Arc, Mutex};

use reqly_ai::{
    BoxFuture, ChatRequest, Chunk, Delta, HttpCall, Message, Provider, ToolCall, ToolSpec,
    Transport, chat, models,
};
use reqly_core::{CoreError, CoreResult};
use serde_json::{Value, json};

/// Answers every call with one status and body, cut into pieces of `split`
/// bytes, and keeps the calls it got.
struct Replay {
    status: u16,
    body: Vec<u8>,
    split: usize,
    calls: Mutex<Vec<HttpCall>>,
}

impl Replay {
    fn new(status: u16, body: impl Into<Vec<u8>>, split: usize) -> Self {
        Self {
            status,
            body: body.into(),
            split,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn last_body(&self) -> Value {
        let calls = self.calls.lock().unwrap();
        serde_json::from_slice(calls.last().unwrap().body.as_ref().unwrap()).unwrap()
    }
}

impl Transport for Replay {
    fn send(
        &self,
        call: HttpCall,
        sink: Arc<dyn Fn(Chunk<'_>) + Send + Sync>,
    ) -> BoxFuture<'_, CoreResult<()>> {
        self.calls.lock().unwrap().push(call);
        Box::pin(async move {
            sink(Chunk::Status(self.status));
            for piece in self.body.chunks(self.split.max(1)) {
                sink(Chunk::Data(piece));
            }
            Ok(())
        })
    }
}

struct Down;

impl Transport for Down {
    fn send(
        &self,
        _call: HttpCall,
        _sink: Arc<dyn Fn(Chunk<'_>) + Send + Sync>,
    ) -> BoxFuture<'_, CoreResult<()>> {
        Box::pin(async { Err(CoreError::new("http.connect", "connection refused")) })
    }
}

fn provider() -> Provider {
    Provider {
        base_url: "https://api.example.com/v1/".into(),
        api_key: Some("sk-test".into()),
        headers: vec![("X-Title".into(), "Reqly".into())],
    }
}

fn request() -> ChatRequest {
    ChatRequest {
        model: "gpt-test".into(),
        messages: vec![
            Message::System {
                content: "You help with HTTP.".into(),
            },
            Message::User {
                content: "Hi".into(),
            },
        ],
        tools: vec![],
        temperature: None,
    }
}

fn sse(chunks: &[Value]) -> String {
    let mut out = String::new();
    for c in chunks {
        out.push_str(&format!("data: {c}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}

fn text_chunk(text: &str) -> Value {
    json!({ "choices": [{ "index": 0, "delta": { "content": text }, "finish_reason": null }] })
}

/// Collects the deltas a call emits.
fn recorder() -> (
    Arc<Mutex<Vec<Delta>>>,
    impl Fn(Delta) + Send + Sync + 'static,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    (seen, move |d| sink.lock().unwrap().push(d))
}

#[tokio::test]
async fn streams_text_and_usage_in_any_split() {
    let body = sse(&[
        json!({ "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" } }] }),
        text_chunk("Hel"),
        text_chunk("lo, "),
        text_chunk("мир"),
        json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }] }),
        json!({ "choices": [], "usage": { "prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15 } }),
    ]);
    for split in [1, 2, 3, 7, 64, 10_000] {
        let t = Replay::new(200, body.clone(), split);
        let (seen, on_delta) = recorder();
        let done = chat(&t, &provider(), &request(), on_delta).await.unwrap();
        assert_eq!(done.content, "Hello, мир", "split {split}");
        assert_eq!(done.finish_reason.as_deref(), Some("stop"));
        assert!(done.tool_calls.is_empty());
        let usage = done.usage.unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (12, 3));
        let text: String = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|d| match d {
                Delta::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "Hello, мир", "split {split}");
    }
}

#[tokio::test]
async fn sends_an_openai_request() {
    let t = Replay::new(200, sse(&[text_chunk("ok")]), 1000);
    let mut req = request();
    req.temperature = Some(0.2);
    req.tools = vec![ToolSpec {
        name: "get_tree".into(),
        description: "The tree.".into(),
        parameters: json!({ "type": "object", "properties": {} }),
    }];
    req.messages.push(Message::Assistant {
        content: None,
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "get_tree".into(),
            arguments: "{}".into(),
        }],
    });
    req.messages.push(Message::Tool {
        tool_call_id: "call_1".into(),
        content: "{\"items\":[]}".into(),
    });
    chat(&t, &provider(), &req, |_| {}).await.unwrap();

    let calls = t.calls.lock().unwrap();
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "https://api.example.com/v1/chat/completions");
    let header = |name: &str| {
        call.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(header("authorization"), Some("Bearer sk-test"));
    assert_eq!(header("content-type"), Some("application/json"));
    assert_eq!(header("x-title"), Some("Reqly"));
    drop(calls);

    let body = t.last_body();
    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(
        body["messages"][0],
        json!({ "role": "system", "content": "You help with HTTP." })
    );
    assert_eq!(
        body["messages"][1],
        json!({ "role": "user", "content": "Hi" })
    );
    assert_eq!(
        body["messages"][2],
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "get_tree", "arguments": "{}" } }]
        })
    );
    assert_eq!(
        body["messages"][3],
        json!({ "role": "tool", "tool_call_id": "call_1", "content": "{\"items\":[]}" })
    );
    assert_eq!(
        body["tools"][0],
        json!({ "type": "function", "function": { "name": "get_tree", "description": "The tree.", "parameters": { "type": "object", "properties": {} } } })
    );
}

#[tokio::test]
async fn no_key_no_authorization_and_no_tools_field() {
    let t = Replay::new(200, sse(&[text_chunk("ok")]), 1000);
    let p = Provider {
        base_url: "http://localhost:11434/v1".into(),
        api_key: None,
        headers: vec![],
    };
    chat(&t, &p, &request(), |_| {}).await.unwrap();
    let calls = t.calls.lock().unwrap();
    assert_eq!(calls[0].url, "http://localhost:11434/v1/chat/completions");
    assert!(
        !calls[0]
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
    );
    drop(calls);
    let body = t.last_body();
    assert!(body.get("tools").is_none(), "{body}");
    assert!(body.get("temperature").is_none(), "{body}");
}

#[tokio::test]
async fn assembles_parallel_tool_calls_by_index() {
    let body = sse(&[
        json!({ "choices": [{ "index": 0, "delta": { "role": "assistant", "content": null, "tool_calls": [
            { "index": 0, "id": "call_a", "type": "function", "function": { "name": "get_tree", "arguments": "" } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
            { "index": 0, "function": { "arguments": "{\"collec" } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
            { "index": 1, "id": "call_b", "type": "function", "function": { "name": "get_request", "arguments": "{\"id\":" } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
            { "index": 0, "function": { "arguments": "tionId\":\"c1\"}" } },
            { "index": 1, "function": { "arguments": "\"r1\"}" } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }] }),
    ]);
    let t = Replay::new(200, body, 5);
    let (seen, on_delta) = recorder();
    let done = chat(&t, &provider(), &request(), on_delta).await.unwrap();
    assert_eq!(done.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(
        done.tool_calls,
        vec![
            ToolCall {
                id: "call_a".into(),
                name: "get_tree".into(),
                arguments: "{\"collectionId\":\"c1\"}".into(),
            },
            ToolCall {
                id: "call_b".into(),
                name: "get_request".into(),
                arguments: "{\"id\":\"r1\"}".into(),
            },
        ]
    );
    let started: Vec<_> = seen
        .lock()
        .unwrap()
        .iter()
        .filter_map(|d| match d {
            Delta::ToolCall { name } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(started, ["get_tree", "get_request"]);
}

#[tokio::test]
async fn ollama_style_tool_calls_in_one_chunk_without_index() {
    // Ollama sends a whole call at once and may omit `index`.
    let body = sse(&[
        json!({ "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "", "tool_calls": [
            { "id": "call_x", "type": "function", "function": { "name": "list_workspaces", "arguments": "{}" } },
            { "id": "call_y", "type": "function", "function": { "name": "get_tree", "arguments": "{\"collectionId\":\"c\"}" } }
        ] }, "finish_reason": null }] }),
        json!({ "choices": [{ "index": 0, "delta": { "role": "assistant", "content": "" }, "finish_reason": "tool_calls" }] }),
    ]);
    let t = Replay::new(200, body, 9);
    let done = chat(&t, &provider(), &request(), |_| {}).await.unwrap();
    let names: Vec<_> = done.tool_calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["list_workspaces", "get_tree"]);
    assert_eq!(done.tool_calls[1].arguments, "{\"collectionId\":\"c\"}");
}

#[tokio::test]
async fn tool_call_without_id_gets_one() {
    let body = sse(
        &[json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
        { "index": 0, "function": { "name": "get_tree", "arguments": "{}" } }
    ] }, "finish_reason": "tool_calls" }] })],
    );
    let t = Replay::new(200, body, 100);
    let done = chat(&t, &provider(), &request(), |_| {}).await.unwrap();
    assert!(!done.tool_calls[0].id.is_empty());
}

#[tokio::test]
async fn reasoning_deltas_are_kept_apart() {
    let body = sse(&[
        json!({ "choices": [{ "index": 0, "delta": { "reasoning": "Let me think" } }] }),
        json!({ "choices": [{ "index": 0, "delta": { "reasoning_content": "..." } }] }),
        text_chunk("Answer"),
    ]);
    let t = Replay::new(200, body, 4);
    let (seen, on_delta) = recorder();
    let done = chat(&t, &provider(), &request(), on_delta).await.unwrap();
    assert_eq!(done.content, "Answer");
    assert_eq!(done.reasoning, "Let me think...");
    assert!(
        seen.lock()
            .unwrap()
            .contains(&Delta::Reasoning("Let me think".into()))
    );
}

#[tokio::test]
async fn crlf_lines_comments_and_no_done_marker() {
    let body = ": OPENROUTER PROCESSING\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"a\"}}]}\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"b\"},\"finish_reason\":\"stop\"}]}\r\n\r\n";
    let t = Replay::new(200, body, 3);
    let done = chat(&t, &provider(), &request(), |_| {}).await.unwrap();
    assert_eq!(done.content, "ab");
}

#[tokio::test]
async fn a_plain_json_answer_is_accepted() {
    // A provider that ignores `stream`.
    let body = json!({
        "choices": [{ "index": 0, "message": {
            "role": "assistant", "content": "full",
            "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "get_tree", "arguments": "{}" } }]
        }, "finish_reason": "tool_calls" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3 }
    })
    .to_string();
    let t = Replay::new(200, body, 10);
    let (seen, on_delta) = recorder();
    let done = chat(&t, &provider(), &request(), on_delta).await.unwrap();
    assert_eq!(done.content, "full");
    assert_eq!(done.tool_calls.len(), 1);
    assert_eq!(done.usage.unwrap().total_tokens, 3);
    assert!(seen.lock().unwrap().contains(&Delta::Text("full".into())));
}

async fn error_of(status: u16, body: &str) -> CoreError {
    let t = Replay::new(status, body, 7);
    chat(&t, &provider(), &request(), |_| {}).await.unwrap_err()
}

#[tokio::test]
async fn http_errors_map_to_codes_with_the_provider_message() {
    let e = error_of(
        401,
        r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#,
    )
    .await;
    assert_eq!(e.code, "ai.auth");
    assert_eq!(e.params["message"], "Incorrect API key provided");
    assert_eq!(e.params["status"], 401);

    assert_eq!(error_of(403, "{}").await.code, "ai.auth");
    assert_eq!(
        error_of(429, r#"{"error":{"message":"slow down"}}"#)
            .await
            .code,
        "ai.rate_limit"
    );

    // Ollama: `error` is a string.
    let e = error_of(
        404,
        r#"{"error":"model \"llama9\" not found, try pulling it first"}"#,
    )
    .await;
    assert_eq!(e.code, "ai.model");
    assert!(e.params["message"].as_str().unwrap().contains("llama9"));

    let e = error_of(500, "<html>Bad gateway</html>").await;
    assert_eq!(e.code, "ai.provider");
    assert_eq!(e.params["message"], "<html>Bad gateway</html>");
    assert_eq!(e.params["status"], 500);
}

#[tokio::test]
async fn an_error_inside_the_stream_fails_the_call() {
    let body = format!(
        "data: {}\n\ndata: {}\n\n",
        text_chunk("partial"),
        json!({ "error": { "message": "upstream overloaded", "code": 502 } })
    );
    let e = error_of(200, &body).await;
    assert_eq!(e.code, "ai.provider");
    assert_eq!(e.params["message"], "upstream overloaded");
}

#[tokio::test]
async fn transport_failures_are_network_errors() {
    let e = chat(&Down, &provider(), &request(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(e.code, "ai.network");
    assert_eq!(e.params["message"], "connection refused");
}

struct Stopped;

impl Transport for Stopped {
    fn send(
        &self,
        _call: HttpCall,
        sink: Arc<dyn Fn(Chunk<'_>) + Send + Sync>,
    ) -> BoxFuture<'_, CoreResult<()>> {
        sink(Chunk::Status(200));
        sink(Chunk::Data(
            format!("data: {}\n\n", text_chunk("par")).as_bytes(),
        ));
        Box::pin(async { Err(CoreError::new("ai.cancelled", "stopped")) })
    }
}

#[tokio::test]
async fn a_stopped_transport_reports_cancelled() {
    let e = chat(&Stopped, &provider(), &request(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(e.code, "ai.cancelled");
}

#[tokio::test]
async fn models_are_listed_sorted() {
    let t = Replay::new(
        200,
        json!({ "object": "list", "data": [
            { "id": "gpt-b", "object": "model" }, { "id": "gpt-a", "object": "model" }
        ] })
        .to_string(),
        6,
    );
    let list = models(&t, &provider()).await.unwrap();
    assert_eq!(list, ["gpt-a", "gpt-b"]);
    let calls = t.calls.lock().unwrap();
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].url, "https://api.example.com/v1/models");
    assert!(calls[0].body.is_none());
}

#[tokio::test]
async fn models_errors_use_the_same_codes() {
    let t = Replay::new(401, r#"{"error":{"message":"no key"}}"#, 100);
    assert_eq!(models(&t, &provider()).await.unwrap_err().code, "ai.auth");
}
