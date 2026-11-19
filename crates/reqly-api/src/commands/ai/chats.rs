//! Chat commands: list, open, create, rename, delete, pick a model.

use reqly_core::{CoreError, CoreResult, Id};
use reqly_storage::ai::{AiChat, AiChatMessage, AiMessageStatus, AiRepo};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::config::{load_config, provider_client};
use crate::access;
use crate::commands::AppContext;

fn not_found() -> CoreError {
    CoreError::new("not_found", "chat not found")
}

/// The chat, if it belongs to the caller (others' chats do not exist for
/// them, administrators included).
pub(crate) async fn own_chat(ctx: &AppContext, id: Id) -> CoreResult<AiChat> {
    let chat = AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .get_chat(id)
        .await?;
    if chat.owner != access::current().user_id() {
        return Err(not_found());
    }
    Ok(chat)
}

pub(crate) fn chats_changed(ctx: &AppContext, workspace_id: Id) {
    ctx.events.publish(crate::Event::new(
        "ai.chatsChanged",
        json!({ "workspaceId": workspace_id }),
    ));
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiWorkspaceInput {
    pub workspace_id: Id,
}

pub(crate) async fn list(ctx: &AppContext, workspace_id: Id) -> CoreResult<Vec<AiChat>> {
    AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .list_chats(workspace_id, access::current().user_id())
        .await
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiCreateChatInput {
    pub workspace_id: Id,
    /// The default model when left out.
    #[ts(optional)]
    pub provider_id: Option<String>,
    #[ts(optional)]
    pub model: Option<String>,
}

pub(crate) async fn create(ctx: &AppContext, input: AiCreateChatInput) -> CoreResult<AiChat> {
    let config = load_config(&ctx.storage).await?;
    if !config.enabled {
        return Err(CoreError::new("ai.disabled", "the AI chat is turned off"));
    }
    let (provider, model) = match (input.provider_id, input.model) {
        (Some(p), Some(m)) => (Some(p), Some(m)),
        _ => match &config.default_model {
            Some(d) => (Some(d.provider_id.clone()), Some(d.model.clone())),
            None => (config.providers.first().map(|p| p.id.clone()), None),
        },
    };
    if let Some(p) = &provider {
        provider_client(ctx, &config, p).await?;
    }
    let chat = AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .create_chat(
            input.workspace_id,
            access::current().user_id(),
            "",
            provider.as_deref(),
            model.as_deref(),
        )
        .await?;
    chats_changed(ctx, chat.workspace_id);
    Ok(chat)
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiChatInput {
    pub chat_id: Id,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiChatView {
    pub chat: AiChat,
    pub messages: Vec<AiChatMessage>,
    /// A turn is in progress (its events keep coming).
    pub running: bool,
    /// Tool calls (ids) waiting for the user's answer.
    pub waiting: Vec<String>,
}

pub(crate) async fn open(ctx: &AppContext, id: Id) -> CoreResult<AiChatView> {
    let chat = own_chat(ctx, id).await?;
    let repo = AiRepo::new(&ctx.storage, ctx.clock.as_ref());
    let mut messages = repo.messages(id).await?;
    let running = ctx.ai_turns.running(id);
    if !running {
        // Left pending by a turn that ended with the app.
        for m in messages
            .iter_mut()
            .filter(|m| m.status == AiMessageStatus::Pending)
        {
            m.status = AiMessageStatus::Cancelled;
            repo.save_message(m).await?;
        }
    }
    Ok(AiChatView {
        chat,
        messages,
        running,
        waiting: ctx.ai_turns.waiting(id),
    })
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiRenameChatInput {
    pub chat_id: Id,
    pub title: String,
}

pub(crate) async fn rename(ctx: &AppContext, input: AiRenameChatInput) -> CoreResult<AiChat> {
    own_chat(ctx, input.chat_id).await?;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(CoreError::new("ai.invalid_title", "the title is empty"));
    }
    let chat = AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .rename_chat(input.chat_id, title)
        .await?;
    chats_changed(ctx, chat.workspace_id);
    Ok(chat)
}

pub(crate) async fn delete(ctx: &AppContext, id: Id) -> CoreResult<()> {
    let chat = own_chat(ctx, id).await?;
    ctx.ai_turns.cancel(id);
    AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .delete_chat(id)
        .await?;
    chats_changed(ctx, chat.workspace_id);
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiSetChatModelInput {
    pub chat_id: Id,
    pub provider_id: String,
    pub model: String,
}

pub(crate) async fn set_model(ctx: &AppContext, input: AiSetChatModelInput) -> CoreResult<AiChat> {
    own_chat(ctx, input.chat_id).await?;
    let config = load_config(&ctx.storage).await?;
    provider_client(ctx, &config, &input.provider_id).await?;
    let model = input.model.trim();
    if model.is_empty() {
        return Err(CoreError::new("ai.no_model", "no model chosen"));
    }
    let chat = AiRepo::new(&ctx.storage, ctx.clock.as_ref())
        .set_chat_model(input.chat_id, &input.provider_id, model)
        .await?;
    chats_changed(ctx, chat.workspace_id);
    Ok(chat)
}
