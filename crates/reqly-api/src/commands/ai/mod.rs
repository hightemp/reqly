//! The AI chat (ADR-015): providers, chats and turns. The provider client
//! is `reqly-ai`; tools are [`crate::agent`].

pub mod chats;
pub mod config;
pub mod transport;
pub mod turn;
pub mod turns;

pub use turns::Turns;

use reqly_core::CoreResult;
use reqly_http::engine::CancelToken;
use serde::Deserialize;
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::registry::{Command, Registry};
use chats::{
    AiChatInput, AiChatView, AiCreateChatInput, AiRenameChatInput, AiSetChatModelInput,
    AiWorkspaceInput,
};
use config::{AiConfig, AiStatus};
use reqly_storage::ai::AiChat;
use turn::{AiConfirmInput, AiSendInput, AiSendResult};

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

command!(
    /// Providers (without keys), tools and presets.
    AiConfigGet,
    "ai.config",
    Empty,
    AiStatus,
    |ctx, _input| { config::status(ctx).await }
);

command!(
    /// Saves the AI settings (administrators on a multi-user server).
    AiSetConfig,
    "ai.setConfig",
    AiConfig,
    AiStatus,
    |ctx, input| { config::set_config(ctx, input).await }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiSetKeyInput {
    pub provider_id: String,
    /// `null` or empty clears the key.
    pub key: Option<String>,
}

command!(
    /// Stores or clears a provider's API key; no command returns it.
    AiSetKey,
    "ai.setKey",
    AiSetKeyInput,
    Empty,
    |ctx, input| {
        config::set_key(ctx, &input.provider_id, input.key).await?;
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiModelsInput {
    pub provider_id: String,
}

command!(
    /// The models a provider lists (`GET /models`); also checks the connection.
    AiModels,
    "ai.models",
    AiModelsInput,
    Vec<String>,
    |ctx, input| {
        let config = config::load_config(&ctx.storage).await?;
        let provider = config::provider_client(ctx, &config, &input.provider_id).await?;
        let transport = transport::HttpTransport::new(ctx, CancelToken::new());
        reqly_ai::models(&transport, &provider).await
    }
);

command!(
    /// The caller's chats in a workspace, most recently active first.
    AiChats,
    "ai.chats",
    AiWorkspaceInput,
    Vec<AiChat>,
    |ctx, input| { chats::list(ctx, input.workspace_id).await }
);

command!(
    /// A chat with its messages.
    AiChatGet,
    "ai.chat",
    AiChatInput,
    AiChatView,
    |ctx, input| { chats::open(ctx, input.chat_id).await }
);

command!(
    /// Starts a chat with the given or the default model.
    AiCreateChat,
    "ai.createChat",
    AiCreateChatInput,
    AiChat,
    |ctx, input| { chats::create(ctx, input).await }
);

command!(
    AiRenameChat,
    "ai.renameChat",
    AiRenameChatInput,
    AiChat,
    |ctx, input| { chats::rename(ctx, input).await }
);

command!(
    /// Deletes a chat and stops its turn.
    AiDeleteChat,
    "ai.deleteChat",
    AiChatInput,
    Empty,
    |ctx, input| {
        chats::delete(ctx, input.chat_id).await?;
        Ok(Empty {})
    }
);

command!(
    AiSetChatModel,
    "ai.setChatModel",
    AiSetChatModelInput,
    AiChat,
    |ctx, input| { chats::set_model(ctx, input).await }
);

command!(
    /// Sends a message and runs the turn; progress comes as `ai.*` events
    /// and the command returns when the turn ends.
    AiSend,
    "ai.send",
    AiSendInput,
    AiSendResult,
    |ctx, input| { turn::send(ctx, input).await }
);

command!(
    /// Stops the chat's turn, keeping what arrived.
    AiCancel,
    "ai.cancel",
    AiChatInput,
    Empty,
    |ctx, input| {
        turn::cancel(ctx, input.chat_id).await?;
        Ok(Empty {})
    }
);

command!(
    /// Answers a tool call that waits for the user.
    AiConfirm,
    "ai.confirm",
    AiConfirmInput,
    Empty,
    |ctx, input| {
        turn::confirm(ctx, input).await?;
        Ok(Empty {})
    }
);

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<AiConfigGet>()
        .register::<AiSetConfig>()
        .register::<AiSetKey>()
        .register::<AiModels>()
        .register::<AiChats>()
        .register::<AiChatGet>()
        .register::<AiCreateChat>()
        .register::<AiRenameChat>()
        .register::<AiDeleteChat>()
        .register::<AiSetChatModel>()
        .register::<AiSend>()
        .register::<AiCancel>()
        .register::<AiConfirm>();
}
