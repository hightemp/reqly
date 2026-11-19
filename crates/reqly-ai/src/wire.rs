//! The Chat Completions wire format: request JSON, the streamed answer and
//! error bodies.

use reqly_core::{CoreError, CoreResult};
use reqly_protocols::sse::SseParser;
use serde_json::{Map, Value, json};

use crate::{ChatRequest, Chunk, Completion, Delta, MAX_BUFFER, Message, ToolCall, Usage};

pub(crate) fn request_body(request: &ChatRequest) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message).collect();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
    }
    if let Some(t) = request.temperature {
        // Through f64 text so 0.2 stays 0.2, not 0.20000000298.
        let t: f64 = format!("{t}").parse().unwrap_or(t as f64);
        body["temperature"] = json!(t);
    }
    body
}

fn message(m: &Message) -> Value {
    match m {
        Message::System { content } => json!({ "role": "system", "content": content }),
        Message::User { content } => json!({ "role": "user", "content": content }),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut v = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                v["tool_calls"] = tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": { "name": c.name, "arguments": c.arguments },
                        })
                    })
                    .collect();
            }
            v
        }
        Message::Tool {
            tool_call_id,
            content,
        } => json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content }),
    }
}

/// A tool call whose parts are still arriving.
#[derive(Default)]
struct Partial {
    id: String,
    name: String,
    arguments: String,
    announced: bool,
}

pub(crate) struct StreamState {
    on_delta: Box<dyn Fn(Delta) + Send + Sync>,
    status: u16,
    parser: SseParser,
    /// Raw bytes, for an error body or an answer that is not a stream.
    raw: Vec<u8>,
    events: usize,
    done: bool,
    out: Completion,
    calls: Vec<Partial>,
    error: Option<CoreError>,
}

impl StreamState {
    pub(crate) fn new(on_delta: Box<dyn Fn(Delta) + Send + Sync>) -> Self {
        Self {
            on_delta,
            status: 0,
            parser: SseParser::new(),
            raw: Vec::new(),
            events: 0,
            done: false,
            out: Completion::default(),
            calls: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn chunk(&mut self, chunk: Chunk<'_>) {
        match chunk {
            Chunk::Status(s) => {
                self.status = s;
                self.raw.clear();
                self.parser = SseParser::new();
            }
            Chunk::Data(bytes) => {
                if self.raw.len() < MAX_BUFFER && (self.events == 0 || !self.ok()) {
                    self.raw.extend_from_slice(bytes);
                }
                if !self.ok() || self.done || self.error.is_some() {
                    return;
                }
                for event in self.parser.feed(bytes) {
                    self.events += 1;
                    let data = event.data.trim();
                    if data == "[DONE]" {
                        self.done = true;
                        break;
                    }
                    if let Ok(value) = serde_json::from_str::<Value>(data) {
                        self.apply(&value, false);
                    }
                }
            }
        }
    }

    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Applies one streamed chunk, or (`whole`) a complete non-stream answer.
    fn apply(&mut self, value: &Value, whole: bool) {
        if let Some(err) = value.get("error").filter(|e| !e.is_null()) {
            let message = error_message(err).unwrap_or_else(|| err.to_string());
            self.error = Some(crate::provider_error(self.status, &message));
            return;
        }
        if let Some(usage) = value.get("usage").filter(|u| u.is_object()) {
            let n = |k: &str| usage[k].as_u64().unwrap_or(0);
            self.out.usage = Some(Usage {
                prompt_tokens: n("prompt_tokens"),
                completion_tokens: n("completion_tokens"),
                total_tokens: n("total_tokens"),
            });
        }
        let Some(choice) = value["choices"].as_array().and_then(|c| c.first()) else {
            return;
        };
        let delta = if whole {
            &choice["message"]
        } else {
            &choice["delta"]
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.out.finish_reason = Some(reason.to_string());
        }
        for key in ["reasoning", "reasoning_content"] {
            if let Some(text) = delta[key].as_str().filter(|t| !t.is_empty()) {
                self.out.reasoning.push_str(text);
                (self.on_delta)(Delta::Reasoning(text.to_string()));
            }
        }
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            self.out.content.push_str(text);
            (self.on_delta)(Delta::Text(text.to_string()));
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            self.tool_call(call);
        }
    }

    fn tool_call(&mut self, call: &Value) {
        let function = &call["function"];
        let id = call["id"].as_str().unwrap_or("");
        let name = function["name"].as_str().unwrap_or("");
        let slot = match call["index"].as_u64() {
            Some(i) => i as usize,
            // Without an index (Ollama): a call with an id or a name is new,
            // anything else continues the last one.
            None if !id.is_empty() || !name.is_empty() || self.calls.is_empty() => self.calls.len(),
            None => self.calls.len() - 1,
        };
        while self.calls.len() <= slot {
            self.calls.push(Partial::default());
        }
        let partial = &mut self.calls[slot];
        if !id.is_empty() {
            partial.id = id.to_string();
        }
        if !name.is_empty() {
            partial.name.push_str(name);
        }
        match &function["arguments"] {
            Value::String(s) => partial.arguments.push_str(s),
            // Some servers send the arguments as an object.
            Value::Object(_) => partial.arguments = function["arguments"].to_string(),
            _ => {}
        }
        if !partial.announced && !partial.name.is_empty() {
            partial.announced = true;
            (self.on_delta)(Delta::ToolCall {
                name: partial.name.clone(),
            });
        }
    }

    pub(crate) fn finish(&mut self) -> CoreResult<Completion> {
        if !self.ok() {
            return Err(http_error(self.status, &self.raw));
        }
        if self.events == 0 && !self.raw.is_empty() {
            // Not a stream: the provider answered with one JSON object.
            let raw = std::mem::take(&mut self.raw);
            match serde_json::from_slice::<Value>(&raw) {
                Ok(value) => self.apply(&value, true),
                Err(_) => {
                    return Err(crate::provider_error(
                        self.status,
                        &format!("unexpected answer: {}", snippet(&raw)),
                    ));
                }
            }
        }
        if let Some(e) = self.error.take() {
            return Err(e);
        }
        let mut out = std::mem::take(&mut self.out);
        out.tool_calls = std::mem::take(&mut self.calls)
            .into_iter()
            .filter(|p| !p.name.is_empty())
            .enumerate()
            .map(|(i, p)| ToolCall {
                id: if p.id.is_empty() {
                    format!("call_{i}")
                } else {
                    p.id
                },
                name: p.name,
                arguments: if p.arguments.trim().is_empty() {
                    "{}".into()
                } else {
                    p.arguments
                },
            })
            .collect();
        Ok(out)
    }
}

/// The message of an `error` member: an object with `message`, or a string.
fn error_message(err: &Value) -> Option<String> {
    match err {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("message").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn snippet(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    match text.char_indices().nth(500) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// A failed HTTP status as an `ai.*` error with the provider's message.
pub(crate) fn http_error(status: u16, body: &[u8]) -> CoreError {
    let parsed: Option<Map<String, Value>> = serde_json::from_slice(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|o| {
            o.get("error")
                .and_then(error_message)
                .or_else(|| o.get("message").and_then(Value::as_str).map(str::to_string))
                .or_else(|| o.get("detail").and_then(Value::as_str).map(str::to_string))
        })
        .unwrap_or_else(|| {
            let s = snippet(body);
            if s.is_empty() {
                format!("HTTP {status}")
            } else {
                s
            }
        });
    let code = match status {
        401 | 403 => "ai.auth",
        429 => "ai.rate_limit",
        404 => "ai.model",
        _ => "ai.provider",
    };
    CoreError::new(code, message.clone())
        .with_param("status", status)
        .with_param("message", message)
}
