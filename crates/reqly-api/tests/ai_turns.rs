//! A chat turn: streaming, tools, confirmations, cancel (ADR-015, TASKS
//! T14.5), against a fake OpenAI-compatible provider.

use std::time::Duration;

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use reqly_testserver::{FakeLlm, LlmReply};
use serde_json::{Value, json};

struct Fx {
    ctx: AppContext,
    llm: FakeLlm,
    caller: Caller,
    ws: String,
    col: String,
}

impl Fx {
    async fn new() -> Self {
        Self::on(Platform::Desktop, Caller::Local).await
    }

    async fn on(platform: Platform, caller: Caller) -> Self {
        let ctx = AppContext::for_tests_on(platform).await;
        let llm = FakeLlm::start().await;
        let mut fx = Self {
            ctx,
            llm,
            caller,
            ws: String::new(),
            col: String::new(),
        };
        fx.configure(json!({})).await;
        fx.ws = fx.cmd("workspace.create", json!({ "name": "Shop" })).await["id"]
            .as_str()
            .unwrap()
            .to_string();
        fx.col = fx
            .cmd(
                "collection.create",
                json!({ "workspaceId": fx.ws, "name": "Orders API" }),
            )
            .await["id"]
            .as_str()
            .unwrap()
            .to_string();
        fx
    }

    /// AI settings as an administrator: one provider (the fake), plus `extra`.
    async fn configure(&self, extra: Value) {
        let mut config = json!({
            "enabled": true,
            "providers": [{ "id": "fake", "name": "Fake", "preset": "custom", "baseUrl": self.llm.base_url(), "headers": [] }],
            "defaultModel": { "providerId": "fake", "model": "fake-small" },
        });
        for (k, v) in extra.as_object().unwrap() {
            config[k] = v.clone();
        }
        dispatch_as(
            &registry(),
            &self.ctx,
            Caller::Local,
            "ai.setConfig",
            config,
        )
        .await
        .unwrap();
    }

    async fn try_cmd(&self, name: &str, input: Value) -> Result<Value, reqly_core::CoreError> {
        dispatch_as(&registry(), &self.ctx, self.caller, name, input).await
    }

    async fn cmd(&self, name: &str, input: Value) -> Value {
        self.try_cmd(name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    async fn chat(&self) -> String {
        self.cmd("ai.createChat", json!({ "workspaceId": self.ws }))
            .await["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn send(&self, chat: &str, text: &str) -> Value {
        self.cmd("ai.send", json!({ "chatId": chat, "text": text }))
            .await
    }

    async fn messages(&self, chat: &str) -> Vec<Value> {
        self.cmd("ai.chat", json!({ "chatId": chat })).await["messages"]
            .as_array()
            .unwrap()
            .clone()
    }

    /// Answers the next tool call that waits with `decision`.
    async fn decide_next(
        &self,
        events: &mut tokio::sync::broadcast::Receiver<reqly_api::Event>,
        chat: &str,
        decision: &str,
    ) -> Value {
        loop {
            let e = events.recv().await.unwrap();
            if e.topic == "ai.confirm" {
                let call_id = e.payload["callId"].clone();
                self.cmd(
                    "ai.confirm",
                    json!({ "chatId": chat, "callId": call_id, "decision": decision }),
                )
                .await;
                return e.payload;
            }
        }
    }
}

fn roles(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .map(|m| {
            format!(
                "{}:{}",
                m["role"].as_str().unwrap(),
                m["status"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test]
async fn streams_an_answer_and_titles_the_chat() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::Text(
        "Use the Params tab for query strings.".into(),
    ));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();

    let out = fx
        .send(&chat, "How do I add a query parameter to a request?")
        .await;
    assert_eq!(out["status"], "done");

    let messages = fx.messages(&chat).await;
    assert_eq!(roles(&messages), ["user:done", "assistant:done"]);
    assert_eq!(
        messages[1]["content"],
        "Use the Params tab for query strings."
    );
    assert_eq!(messages[1]["usage"]["completionTokens"], 5);

    let view = fx.cmd("ai.chat", json!({ "chatId": chat })).await;
    assert_eq!(
        view["chat"]["title"],
        "How do I add a query parameter to a request?"
    );
    assert_eq!(view["running"], false);

    let mut text = String::new();
    let mut topics = Vec::new();
    while let Ok(e) = events.try_recv() {
        if e.topic == "ai.delta" {
            assert_eq!(e.payload["chatId"], chat.as_str());
            text.push_str(e.payload["text"].as_str().unwrap_or(""));
        }
        topics.push(e.topic);
    }
    assert_eq!(text, "Use the Params tab for query strings.");
    assert!(topics.iter().filter(|t| *t == "ai.delta").count() >= 3);
    assert!(topics.contains(&"ai.message".to_string()));
    assert_eq!(topics.last().unwrap(), "ai.done");

    // The provider got the system prompt with the workspace and the question.
    let body = &fx.llm.chats()[0];
    assert_eq!(body["model"], "fake-small");
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    assert!(system.contains("Reqly"), "{system}");
    assert!(
        system.contains("Shop") && system.contains(&fx.ws),
        "{system}"
    );
    assert!(
        system.contains("Orders API") && system.contains(&fx.col),
        "{system}"
    );
    assert_eq!(
        body["messages"][1],
        json!({ "role": "user", "content": "How do I add a query parameter to a request?" })
    );
    let tools: Vec<_> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect();
    assert!(tools.contains(&"get_tree".to_string()));
    assert!(tools.contains(&"create_request".to_string()));
}

#[tokio::test]
async fn reading_tools_run_at_once_and_the_model_continues() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "list_collections".into(),
        json!({ "workspaceId": fx.ws }),
    )]));
    fx.llm.push(LlmReply::Text("You have Orders API.".into()));
    let chat = fx.chat().await;
    let out = fx.send(&chat, "Which collections do I have?").await;
    assert_eq!(out["status"], "done");

    let messages = fx.messages(&chat).await;
    assert_eq!(
        roles(&messages),
        ["user:done", "assistant:done", "tool:done", "assistant:done"]
    );
    assert_eq!(messages[1]["toolCalls"][0]["name"], "list_collections");
    assert_eq!(messages[2]["toolName"], "list_collections");
    assert!(
        messages[2]["content"]
            .as_str()
            .unwrap()
            .contains("Orders API")
    );

    // The second request carries the call and its result.
    let second = &fx.llm.chats()[1];
    let msgs = second["messages"].as_array().unwrap();
    let n = msgs.len();
    assert_eq!(msgs[n - 2]["role"], "assistant");
    assert_eq!(
        msgs[n - 2]["tool_calls"][0]["function"]["name"],
        "list_collections"
    );
    assert_eq!(msgs[n - 1]["role"], "tool");
    assert_eq!(
        msgs[n - 1]["tool_call_id"],
        msgs[n - 2]["tool_calls"][0]["id"]
    );
    assert!(msgs[n - 1]["content"].as_str().unwrap().contains(&fx.col));
}

#[tokio::test]
async fn changing_tools_wait_for_the_user() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "create_request".into(),
        json!({ "collectionId": fx.col, "name": "List orders", "method": "GET", "url": "https://shop.test/orders" }),
    )]));
    fx.llm.push(LlmReply::Text("Created.".into()));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();

    let (out, asked) = tokio::join!(
        fx.send(&chat, "Add a request listing orders"),
        fx.decide_next(&mut events, &chat, "allow"),
    );
    assert_eq!(out["status"], "done");
    assert_eq!(asked["toolName"], "create_request");
    let tree = fx.cmd("tree.list", json!({ "collectionId": fx.col })).await;
    assert!(tree.to_string().contains("List orders"), "{tree}");
    let messages = fx.messages(&chat).await;
    assert_eq!(messages[2]["status"], "done");
}

#[tokio::test]
async fn a_declined_call_is_reported_to_the_model() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "create_folder".into(),
        json!({ "collectionId": fx.col, "name": "Junk" }),
    )]));
    fx.llm.push(LlmReply::Text("Okay, I won't.".into()));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();
    let (out, _) = tokio::join!(
        fx.send(&chat, "Make a folder"),
        fx.decide_next(&mut events, &chat, "deny"),
    );
    assert_eq!(out["status"], "done");
    let tree = fx.cmd("tree.list", json!({ "collectionId": fx.col })).await;
    assert!(!tree.to_string().contains("Junk"));
    let messages = fx.messages(&chat).await;
    assert_eq!(messages[2]["status"], "denied");
    let second = &fx.llm.chats()[1];
    let last = second["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["role"], "tool");
    assert!(
        last["content"].as_str().unwrap().contains("declined"),
        "{last}"
    );
}

#[tokio::test]
async fn allow_always_stops_asking_in_this_chat() {
    let fx = Fx::new().await;
    for name in ["One", "Two"] {
        fx.llm.push(LlmReply::ToolCalls(vec![(
            "create_folder".into(),
            json!({ "collectionId": fx.col, "name": name }),
        )]));
    }
    fx.llm.push(LlmReply::Text("Both made.".into()));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();
    let (out, _) = tokio::join!(
        fx.send(&chat, "Make two folders"),
        fx.decide_next(&mut events, &chat, "allowAlways"),
    );
    assert_eq!(out["status"], "done");
    let tree = fx.cmd("tree.list", json!({ "collectionId": fx.col })).await;
    assert!(tree.to_string().contains("One") && tree.to_string().contains("Two"));
    let view = fx.cmd("ai.chat", json!({ "chatId": chat })).await;
    assert_eq!(view["chat"]["autoApprove"], true);
}

#[tokio::test]
async fn auto_approved_groups_and_disabled_tools() {
    let fx = Fx::new().await;
    fx.configure(json!({ "autoApprove": { "edit": true }, "tools": { "delete_item": false } }))
        .await;
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "create_folder".into(),
        json!({ "collectionId": fx.col, "name": "Auto" }),
    )]));
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "delete_item".into(),
        json!({ "id": fx.col }),
    )]));
    fx.llm.push(LlmReply::Text("Done.".into()));
    let chat = fx.chat().await;
    let out = fx.send(&chat, "Go").await;
    assert_eq!(out["status"], "done");
    let tree = fx.cmd("tree.list", json!({ "collectionId": fx.col })).await;
    assert!(tree.to_string().contains("Auto"));

    let tools: Vec<_> = fx.llm.chats()[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!tools.contains(&"delete_item".to_string()));
    // A model that calls it anyway gets an error.
    let messages = fx.messages(&chat).await;
    let refused = messages
        .iter()
        .find(|m| m["toolName"] == "delete_item")
        .unwrap();
    assert_eq!(refused["status"], "error");
    assert_eq!(refused["error"]["code"], "ai.tool_unavailable");
}

#[tokio::test]
async fn a_turn_ends_after_twenty_steps() {
    let fx = Fx::new().await;
    for _ in 0..25 {
        fx.llm.push(LlmReply::ToolCalls(vec![(
            "list_workspaces".into(),
            json!({}),
        )]));
    }
    let chat = fx.chat().await;
    let out = fx.send(&chat, "Loop").await;
    assert_eq!(out["status"], "error");
    assert_eq!(fx.llm.chats().len(), 20);
    let messages = fx.messages(&chat).await;
    let last = messages.last().unwrap();
    assert_eq!(last["role"], "assistant");
    assert_eq!(last["error"]["code"], "ai.step_limit");
}

#[tokio::test]
async fn cancel_keeps_what_arrived() {
    let fx = Fx::new().await;
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa ".repeat(4);
    fx.llm
        .push(LlmReply::SlowText(words, Duration::from_millis(60)));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();
    let stop = async {
        loop {
            let e = events.recv().await.unwrap();
            if e.topic == "ai.delta" && e.payload["text"] == "beta " {
                break;
            }
        }
        // A second send while the turn runs is refused.
        let busy = fx
            .try_cmd("ai.send", json!({ "chatId": chat, "text": "again" }))
            .await
            .unwrap_err();
        assert_eq!(busy.code, "ai.busy");
        assert_eq!(
            fx.cmd("ai.chat", json!({ "chatId": chat })).await["running"],
            true
        );
        fx.cmd("ai.cancel", json!({ "chatId": chat })).await;
    };
    let (out, ()) = tokio::join!(fx.send(&chat, "Count"), stop);
    assert_eq!(out["status"], "cancelled");
    let messages = fx.messages(&chat).await;
    let answer = &messages[1];
    assert_eq!(answer["status"], "cancelled");
    let text = answer["content"].as_str().unwrap();
    assert!(text.starts_with("alpha beta"), "{text}");
    assert!(!text.contains("kappa"), "{text}");
    // The chat is usable again.
    fx.llm.push(LlmReply::Text("Next.".into()));
    assert_eq!(fx.send(&chat, "Go on").await["status"], "done");
}

#[tokio::test]
async fn cancel_while_waiting_for_the_user() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "create_folder".into(),
        json!({ "collectionId": fx.col, "name": "Never" }),
    )]));
    let chat = fx.chat().await;
    let mut events = fx.ctx.events.subscribe();
    let stop = async {
        loop {
            let e = events.recv().await.unwrap();
            if e.topic == "ai.confirm" {
                let view = fx.cmd("ai.chat", json!({ "chatId": chat })).await;
                assert_eq!(view["waiting"], json!([e.payload["callId"]]));
                break;
            }
        }
        fx.cmd("ai.cancel", json!({ "chatId": chat })).await;
    };
    let (out, ()) = tokio::join!(fx.send(&chat, "Folder"), stop);
    assert_eq!(out["status"], "cancelled");
    let tree = fx.cmd("tree.list", json!({ "collectionId": fx.col })).await;
    assert!(!tree.to_string().contains("Never"));
    // A later turn still sends a valid history: the call has a result.
    fx.llm.push(LlmReply::Text("Fine.".into()));
    assert_eq!(fx.send(&chat, "ok").await["status"], "done");
    let last = fx.llm.chats().last().unwrap().clone();
    let msgs = last["messages"].as_array().unwrap();
    let call_id = msgs
        .iter()
        .find(|m| m["role"] == "assistant" && m["tool_calls"].is_array())
        .unwrap()["tool_calls"][0]["id"]
        .clone();
    assert!(
        msgs.iter()
            .any(|m| m["role"] == "tool" && m["tool_call_id"] == call_id)
    );
}

#[tokio::test]
async fn provider_errors_are_kept_in_the_chat() {
    let fx = Fx::new().await;
    fx.llm.push(LlmReply::Error(
        500,
        r#"{"error":{"message":"model overloaded"}}"#.into(),
    ));
    let chat = fx.chat().await;
    let out = fx.send(&chat, "Hi").await;
    assert_eq!(out["status"], "error");
    let messages = fx.messages(&chat).await;
    assert_eq!(messages[1]["status"], "error");
    assert_eq!(messages[1]["error"]["code"], "ai.provider");
    assert_eq!(
        messages[1]["error"]["params"]["message"],
        "model overloaded"
    );
    // The failed answer is not sent back to the model next time.
    fx.llm.push(LlmReply::Text("Better now.".into()));
    fx.send(&chat, "Again").await;
    let last = fx.llm.chats().last().unwrap().clone();
    let roles: Vec<_> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(roles, ["system", "user", "user"]);
}

#[tokio::test]
async fn attachments_are_masked_context() {
    let fx = Fx::new().await;
    let req = fx
        .cmd(
            "node.create",
            json!({
                "collectionId": fx.col, "parentId": null, "name": "Get order",
                "data": { "kind": "request", "method": "GET", "url": "https://shop.test/orders/1",
                          "auth": { "type": "bearer", "token": "s3cr3t-tok3n", "prefix": "Bearer" } },
                "placement": { "type": "end" }
            }),
        )
        .await;
    fx.llm.push(LlmReply::Text("It fetches order 1.".into()));
    let chat = fx.chat().await;
    let out = fx
        .cmd(
            "ai.send",
            json!({ "chatId": chat, "text": "What does this do?",
                    "attachments": [{ "kind": "request", "id": req["id"] }] }),
        )
        .await;
    assert_eq!(out["status"], "done");
    let messages = fx.messages(&chat).await;
    assert_eq!(messages[0]["content"], "What does this do?");
    assert_eq!(messages[0]["attachments"][0]["label"], "GET Get order");
    let body = fx.llm.chats()[0].to_string();
    assert!(body.contains("orders/1"), "{body}");
    assert!(!body.contains("s3cr3t-tok3n"), "{body}");
}

#[tokio::test]
async fn tools_run_with_the_users_role() {
    let mut fx = Fx::on(Platform::Server, Caller::Local).await;
    let vic = UserRepo::new(&fx.ctx.storage, fx.ctx.clock.as_ref())
        .create("vic", "", "password1", false)
        .await
        .unwrap();
    MemberRepo::new(&fx.ctx.storage, fx.ctx.clock.as_ref())
        .set(fx.ws.parse().unwrap(), vic.id, Role::Viewer)
        .await
        .unwrap();
    fx.configure(json!({ "autoApprove": { "edit": true } }))
        .await;
    fx.caller = Caller::User {
        id: vic.id,
        admin: false,
    };
    fx.llm.push(LlmReply::ToolCalls(vec![(
        "create_folder".into(),
        json!({ "collectionId": fx.col, "name": "Viewer's" }),
    )]));
    fx.llm.push(LlmReply::Text("I can't.".into()));
    let chat = fx.chat().await;
    assert_eq!(fx.send(&chat, "Make a folder").await["status"], "done");
    let messages = fx.messages(&chat).await;
    assert_eq!(messages[2]["status"], "error");
    assert_eq!(messages[2]["error"]["code"], "access.denied");
}

#[tokio::test]
async fn confirm_without_a_waiting_call_fails() {
    let fx = Fx::new().await;
    let chat = fx.chat().await;
    let e = fx
        .try_cmd(
            "ai.confirm",
            json!({ "chatId": chat, "callId": "call_9", "decision": "allow" }),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "ai.not_waiting");
    let e = fx
        .try_cmd("ai.send", json!({ "chatId": chat, "text": "  " }))
        .await
        .unwrap_err();
    assert_eq!(e.code, "ai.empty_message");
}
