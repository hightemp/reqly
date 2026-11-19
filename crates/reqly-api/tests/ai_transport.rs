//! Provider calls through the `reqly-http` engine (ADR-015, TASKS T14.2).

use std::time::{Duration, Instant};

use reqly_ai::{ChatRequest, Delta, Message, Provider, chat, models};
use reqly_api::commands::ai::transport::HttpTransport;
use reqly_api::commands::{AppContext, Platform};
use reqly_http::engine::CancelToken;
use reqly_testserver::{FakeLlm, LlmReply};
use serde_json::json;

fn provider(llm: &FakeLlm, key: Option<&str>) -> Provider {
    Provider {
        base_url: llm.base_url(),
        api_key: key.map(str::to_string),
        headers: vec![("X-Title".into(), "Reqly".into())],
    }
}

fn request(text: &str) -> ChatRequest {
    ChatRequest {
        model: "fake-small".into(),
        messages: vec![Message::User {
            content: text.into(),
        }],
        tools: vec![],
        temperature: None,
    }
}

#[tokio::test]
async fn streams_an_answer_from_a_provider() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    llm.require_key("sk-1");
    llm.push(LlmReply::Text("Hello from the fake".into()));
    let t = HttpTransport::new(&ctx, CancelToken::new());
    let deltas = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = deltas.clone();
    let done = chat(
        &t,
        &provider(&llm, Some("sk-1")),
        &request("Hi"),
        move |d| seen.lock().unwrap().push(d),
    )
    .await
    .unwrap();
    assert_eq!(done.content, "Hello from the fake");
    assert_eq!(done.usage.unwrap().total_tokens, 15);
    // Streamed in several pieces.
    let texts = deltas
        .lock()
        .unwrap()
        .iter()
        .filter(|d| matches!(d, Delta::Text(_)))
        .count();
    assert!(texts >= 3, "{texts}");
    let req = &llm.requests()[0];
    assert_eq!(req.header("x-title"), Some("Reqly"));
    assert_eq!(
        req.body["messages"][0],
        json!({ "role": "user", "content": "Hi" })
    );
    // No cookies go to a provider.
    assert!(req.header("cookie").is_none());
}

#[tokio::test]
async fn tool_calls_and_models_through_the_engine() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    llm.push(LlmReply::ToolCalls(vec![(
        "get_tree".into(),
        json!({ "collectionId": "c1" }),
    )]));
    let t = HttpTransport::new(&ctx, CancelToken::new());
    let done = chat(&t, &provider(&llm, None), &request("tree?"), |_| {})
        .await
        .unwrap();
    assert_eq!(done.tool_calls[0].name, "get_tree");
    assert_eq!(done.tool_calls[0].arguments, r#"{"collectionId":"c1"}"#);

    llm.set_models(&["b-model", "a-model"]);
    let list = models(&t, &provider(&llm, None)).await.unwrap();
    assert_eq!(list, ["a-model", "b-model"]);
}

#[tokio::test]
async fn wrong_key_is_an_auth_error() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    llm.require_key("sk-right");
    let t = HttpTransport::new(&ctx, CancelToken::new());
    let e = chat(
        &t,
        &provider(&llm, Some("sk-wrong")),
        &request("Hi"),
        |_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, "ai.auth");
    assert_eq!(e.params["message"], "Incorrect API key provided");
    assert_eq!(
        models(&t, &provider(&llm, None)).await.unwrap_err().code,
        "ai.auth"
    );
}

#[tokio::test]
async fn provider_errors_and_an_unreachable_provider() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    llm.push(LlmReply::Error(
        429,
        r#"{"error":{"message":"Rate limit reached"}}"#.into(),
    ));
    let t = HttpTransport::new(&ctx, CancelToken::new());
    let e = chat(&t, &provider(&llm, None), &request("Hi"), |_| {})
        .await
        .unwrap_err();
    assert_eq!(e.code, "ai.rate_limit");

    let down = Provider {
        base_url: "http://127.0.0.1:9/v1".into(),
        api_key: None,
        headers: vec![],
    };
    let e = chat(&t, &down, &request("Hi"), |_| {}).await.unwrap_err();
    assert_eq!(e.code, "ai.network");
}

#[tokio::test]
async fn cancel_stops_the_stream() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let llm = FakeLlm::start().await;
    let words = "one two three four five six seven eight nine ten ".repeat(3);
    llm.push(LlmReply::SlowText(words, Duration::from_millis(100)));
    let cancel = CancelToken::new();
    let t = HttpTransport::new(&ctx, cancel.clone());
    let got = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = got.clone();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        stopper.cancel();
    });
    let started = Instant::now();
    let e = chat(&t, &provider(&llm, None), &request("long"), move |d| {
        if let Delta::Text(t) = d {
            sink.lock().unwrap().push_str(&t);
        }
    })
    .await
    .unwrap_err();
    assert_eq!(e.code, "ai.cancelled");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    let got = got.lock().unwrap().clone();
    assert!(got.starts_with("one two"), "{got}");
    assert!(!got.contains("ten"), "{got}");
}
