//! The gate the apps use: MCP settings decide on every request.

use std::sync::Arc;

use reqly_api::access::Caller;
use reqly_api::commands::mcp::{enabled_tools, load_config, token_matches};
use reqly_api::commands::{AppContext, Platform};
use reqly_storage::users::UserRepo;

use crate::{Allowed, BoxFuture, Denied, Gate, Session};

/// Which bearer tokens the endpoint accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// The MCP token from the settings; calls run as [`Caller::Local`]
    /// (desktop, server modes `none` and `token`).
    Mcp,
    /// Personal API tokens; calls run as their user (server mode `multi`).
    UserApi,
}

pub struct SettingsGate {
    ctx: Arc<AppContext>,
    kind: TokenKind,
}

impl SettingsGate {
    pub fn new(ctx: Arc<AppContext>, kind: TokenKind) -> Self {
        Self { ctx, kind }
    }

    async fn caller(&self, token: &str) -> Option<Caller> {
        match self.kind {
            TokenKind::Mcp => token_matches(&self.ctx.storage, token)
                .await
                .ok()?
                .then_some(Caller::Local),
            TokenKind::UserApi => {
                let users = UserRepo::new(&self.ctx.storage, self.ctx.clock.as_ref());
                let user = users.user_for_token(token).await.ok()??;
                Some(Caller::User {
                    id: user.id,
                    admin: user.is_admin,
                })
            }
        }
    }
}

impl Gate for SettingsGate {
    fn open<'a>(&'a self, token: Option<&'a str>) -> BoxFuture<'a, Result<Session, Denied>> {
        Box::pin(async move {
            if self.ctx.platform == Platform::Mobile {
                return Err(Denied::Disabled);
            }
            let config = load_config(&self.ctx.storage)
                .await
                .map_err(|_| Denied::Disabled)?;
            if !config.enabled {
                return Err(Denied::Disabled);
            }
            let token = token
                .filter(|t| !t.is_empty())
                .ok_or(Denied::Unauthorized)?;
            let caller = self.caller(token).await.ok_or(Denied::Unauthorized)?;
            Ok(Session {
                caller,
                allowed: Allowed::Only(enabled_tools(&config)),
            })
        })
    }
}
