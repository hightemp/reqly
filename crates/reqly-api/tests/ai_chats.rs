//! AI chats in the core (ADR-015, TASKS T14.4).

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, Platform, dispatch_as, registry};
use reqly_storage::ai::{AiMessageStatus, AiRepo, AiRole, NewAiMessage};
use reqly_storage::users::{MemberRepo, Role, UserRepo};
use serde_json::{Value, json};

async fn call_as(ctx: &AppContext, caller: Caller, name: &str, input: Value) -> Value {
    dispatch_as(&registry(), ctx, caller, name, input)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Value {
    call_as(ctx, Caller::Local, name, input).await
}

async fn call_err(ctx: &AppContext, caller: Caller, name: &str, input: Value) -> String {
    dispatch_as(&registry(), ctx, caller, name, input)
        .await
        .unwrap_err()
        .code
}

async fn workspace(ctx: &AppContext, caller: Caller) -> String {
    call_as(ctx, caller, "workspace.create", json!({ "name": "W" })).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn enable(ctx: &AppContext, caller: Caller) {
    call_as(
        ctx,
        caller,
        "ai.setConfig",
        json!({
            "enabled": true,
            "providers": [{ "id": "p1", "name": "P", "preset": "ollama", "baseUrl": "http://localhost:11434/v1", "headers": [] }],
            "defaultModel": { "providerId": "p1", "model": "llama3.2" },
        }),
    )
    .await;
}

#[tokio::test]
async fn create_list_rename_and_delete() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    enable(&ctx, Caller::Local).await;
    let ws = workspace(&ctx, Caller::Local).await;
    let mut events = ctx.events.subscribe();

    let chat = call(&ctx, "ai.createChat", json!({ "workspaceId": ws })).await;
    // The default model is preselected; the title comes with the first message.
    assert_eq!(chat["providerId"], "p1");
    assert_eq!(chat["model"], "llama3.2");
    assert_eq!(chat["title"], "");
    assert_eq!(events.recv().await.unwrap().topic, "ai.chatsChanged");
    let id = chat["id"].as_str().unwrap().to_string();

    let second = call(&ctx, "ai.createChat", json!({ "workspaceId": ws })).await;
    let list = call(&ctx, "ai.chats", json!({ "workspaceId": ws })).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["id"], second["id"], "newest first");

    let renamed = call(
        &ctx,
        "ai.renameChat",
        json!({ "chatId": id, "title": "  Debug 500  " }),
    )
    .await;
    assert_eq!(renamed["title"], "Debug 500");
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.renameChat",
        json!({ "chatId": id, "title": " " }),
    )
    .await;
    assert_eq!(code, "ai.invalid_title");

    let moved = call(
        &ctx,
        "ai.setChatModel",
        json!({ "chatId": id, "providerId": "p1", "model": "qwen3" }),
    )
    .await;
    assert_eq!(moved["model"], "qwen3");
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.setChatModel",
        json!({ "chatId": id, "providerId": "ghost", "model": "m" }),
    )
    .await;
    assert_eq!(code, "ai.unknown_provider");

    let full = call(&ctx, "ai.chat", json!({ "chatId": id })).await;
    assert_eq!(full["chat"]["title"], "Debug 500");
    assert_eq!(full["messages"], json!([]));
    assert_eq!(full["running"], false);

    call(&ctx, "ai.deleteChat", json!({ "chatId": id })).await;
    let list = call(&ctx, "ai.chats", json!({ "workspaceId": ws })).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(
        call_err(&ctx, Caller::Local, "ai.chat", json!({ "chatId": id })).await,
        "not_found"
    );
}

#[tokio::test]
async fn chats_need_ai_turned_on() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    let ws = workspace(&ctx, Caller::Local).await;
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.createChat",
        json!({ "workspaceId": ws }),
    )
    .await;
    assert_eq!(code, "ai.disabled");
}

#[tokio::test]
async fn messages_come_with_the_chat_and_unfinished_answers_are_cancelled() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    enable(&ctx, Caller::Local).await;
    let ws = workspace(&ctx, Caller::Local).await;
    let chat = call(&ctx, "ai.createChat", json!({ "workspaceId": ws })).await;
    let id: reqly_core::Id = chat["id"].as_str().unwrap().parse().unwrap();

    // What a turn interrupted by a restart leaves behind.
    let repo = AiRepo::new(&ctx.storage, ctx.clock.as_ref());
    repo.add_message(id, NewAiMessage::new(AiRole::User, "hi"))
        .await
        .unwrap();
    repo.add_message(
        id,
        NewAiMessage {
            status: AiMessageStatus::Pending,
            ..NewAiMessage::new(AiRole::Assistant, "Hel")
        },
    )
    .await
    .unwrap();

    let full = call(&ctx, "ai.chat", json!({ "chatId": id.to_string() })).await;
    let messages = full["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["content"], "Hel");
    assert_eq!(messages[1]["status"], "cancelled");
    // The hidden context is not part of the answer.
    assert!(messages[0].get("context").is_none());
    let again = repo.messages(id).await.unwrap();
    assert_eq!(again[1].status, AiMessageStatus::Cancelled);
}

#[tokio::test]
async fn each_user_sees_only_their_own_chats() {
    let ctx = AppContext::for_tests_on(Platform::Server).await;
    let users = UserRepo::new(&ctx.storage, ctx.clock.as_ref());
    let ann = users.create("ann", "", "password1", false).await.unwrap();
    let bob = users.create("bob", "", "password1", false).await.unwrap();
    let root = users.create("root", "", "password1", true).await.unwrap();
    let ann_c = Caller::User {
        id: ann.id,
        admin: false,
    };
    let bob_c = Caller::User {
        id: bob.id,
        admin: false,
    };
    let root_c = Caller::User {
        id: root.id,
        admin: true,
    };
    enable(&ctx, root_c).await;
    let ws = workspace(&ctx, ann_c).await;
    let ws_id: reqly_core::Id = ws.parse().unwrap();
    MemberRepo::new(&ctx.storage, ctx.clock.as_ref())
        .set(ws_id, bob.id, Role::Viewer)
        .await
        .unwrap();

    let chat = call_as(&ctx, ann_c, "ai.createChat", json!({ "workspaceId": ws })).await;
    let id = chat["id"].as_str().unwrap();
    assert_eq!(chat["owner"], ann.id.to_string());

    // A viewer can chat, but not in someone else's chat.
    let bobs = call_as(&ctx, bob_c, "ai.createChat", json!({ "workspaceId": ws })).await;
    let list = call_as(&ctx, bob_c, "ai.chats", json!({ "workspaceId": ws })).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], bobs["id"]);
    for (name, input) in [
        ("ai.chat", json!({ "chatId": id })),
        ("ai.renameChat", json!({ "chatId": id, "title": "mine" })),
        ("ai.deleteChat", json!({ "chatId": id })),
        (
            "ai.forkChat",
            json!({ "chatId": id, "messageId": reqly_core::Id::new().to_string() }),
        ),
        (
            "ai.setChatModel",
            json!({ "chatId": id, "providerId": "p1", "model": "m" }),
        ),
    ] {
        assert_eq!(
            call_err(&ctx, bob_c, name, input).await,
            "not_found",
            "{name}"
        );
    }
    // Not even an administrator reads another user's chat.
    assert_eq!(
        call_err(&ctx, root_c, "ai.chat", json!({ "chatId": id })).await,
        "not_found"
    );

    // Outside the workspace: nothing.
    let carl = users.create("carl", "", "password1", false).await.unwrap();
    let carl_c = Caller::User {
        id: carl.id,
        admin: false,
    };
    assert_eq!(
        call_err(&ctx, carl_c, "ai.chats", json!({ "workspaceId": ws })).await,
        "not_found"
    );
    assert_eq!(
        call_err(&ctx, carl_c, "ai.createChat", json!({ "workspaceId": ws })).await,
        "not_found"
    );
}

#[tokio::test]
async fn a_chat_branches_off_at_a_message() {
    let ctx = AppContext::for_tests_on(Platform::Desktop).await;
    enable(&ctx, Caller::Local).await;
    let ws = workspace(&ctx, Caller::Local).await;
    let chat = call(&ctx, "ai.createChat", json!({ "workspaceId": ws })).await;
    let id: reqly_core::Id = chat["id"].as_str().unwrap().parse().unwrap();
    call(
        &ctx,
        "ai.renameChat",
        json!({ "chatId": id.to_string(), "title": "Origin" }),
    )
    .await;

    let repo = AiRepo::new(&ctx.storage, ctx.clock.as_ref());
    let add = |m: NewAiMessage| repo.add_message(id, m);
    add(NewAiMessage {
        context: "attached request".into(),
        ..NewAiMessage::new(AiRole::User, "q1")
    })
    .await
    .unwrap();
    let a1 = add(NewAiMessage {
        tool_calls: vec![reqly_storage::ai::AiToolCall {
            id: "call_1".into(),
            name: "list_collections".into(),
            arguments: "{}".into(),
        }],
        ..NewAiMessage::new(AiRole::Assistant, "a1")
    })
    .await
    .unwrap();
    add(NewAiMessage {
        tool_call_id: Some("call_1".into()),
        tool_name: Some("list_collections".into()),
        ..NewAiMessage::new(AiRole::Tool, "{}")
    })
    .await
    .unwrap();
    let a2 = add(NewAiMessage::new(AiRole::Assistant, "a2"))
        .await
        .unwrap();
    add(NewAiMessage::new(AiRole::User, "q2")).await.unwrap();
    let pending = add(NewAiMessage {
        status: AiMessageStatus::Pending,
        ..NewAiMessage::new(AiRole::Assistant, "a3")
    })
    .await
    .unwrap();

    let mut events = ctx.events.subscribe();
    // At a message with tool calls: their results come along.
    let branch = call(
        &ctx,
        "ai.forkChat",
        json!({ "chatId": id.to_string(), "messageId": a1.id.to_string() }),
    )
    .await;
    assert_eq!(events.recv().await.unwrap().topic, "ai.chatsChanged");
    assert_ne!(branch["id"], json!(id.to_string()));
    assert_eq!(branch["title"], "Origin");
    assert_eq!(branch["model"], "llama3.2");
    let branch_id: reqly_core::Id = branch["id"].as_str().unwrap().parse().unwrap();
    let copied = repo.messages(branch_id).await.unwrap();
    let texts: Vec<_> = copied.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(texts, ["q1", "a1", "{}"]);
    assert_eq!(copied[0].context, "attached request");
    assert_eq!(copied[1].tool_calls[0].id, "call_1");
    assert_eq!(copied[2].tool_call_id.as_deref(), Some("call_1"));
    assert!(copied.iter().all(|m| m.chat_id == branch_id));
    assert_ne!(copied[1].id, a1.id);

    // A title of its own; the source chat stays as it was.
    let named = call(
        &ctx,
        "ai.forkChat",
        json!({ "chatId": id.to_string(), "messageId": a2.id.to_string(), "title": "Branch" }),
    )
    .await;
    assert_eq!(named["title"], "Branch");
    let named_id: reqly_core::Id = named["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(repo.messages(named_id).await.unwrap().len(), 4);
    assert_eq!(repo.messages(id).await.unwrap().len(), 6);
    let list = call(&ctx, "ai.chats", json!({ "workspaceId": ws })).await;
    assert_eq!(list.as_array().unwrap().len(), 3);

    // Not at an unfinished answer, nor at a message of another chat.
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.forkChat",
        json!({ "chatId": id.to_string(), "messageId": pending.id.to_string() }),
    )
    .await;
    assert_eq!(code, "ai.busy");
    let code = call_err(
        &ctx,
        Caller::Local,
        "ai.forkChat",
        json!({ "chatId": branch_id.to_string(), "messageId": a2.id.to_string() }),
    )
    .await;
    assert_eq!(code, "not_found");
}
