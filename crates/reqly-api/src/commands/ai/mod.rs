//! The AI chat (ADR-015): providers, chats and turns. The provider client
//! is `reqly-ai`; tools are [`crate::agent`].

pub mod config;
pub mod transport;

use reqly_core::CoreResult;
use reqly_http::engine::CancelToken;
use serde::Deserialize;
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::registry::{Command, Registry};
use config::{AiConfig, AiStatus};

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

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<AiConfigGet>()
        .register::<AiSetConfig>()
        .register::<AiSetKey>()
        .register::<AiModels>();
}
