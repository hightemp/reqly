//! MCP settings, token and call log (ADR-014). The protocol and the tools
//! live in `reqly-mcp`; the core keeps what the settings UI needs and what
//! the endpoint checks on every request.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

use reqly_core::{CoreError, CoreResult, Id};
use reqly_storage::settings::SettingsRepo;
use reqly_storage::users::{random_token, token_hash};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AppContext, Empty, Platform};
use crate::events::EventBus;
use crate::registry::{Command, Registry};

/// Settings group of a tool (PRD §7.22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum McpGroup {
    Read,
    Edit,
    Run,
    Environments,
    Interop,
}

impl McpGroup {
    /// Groups that are on when MCP is first enabled.
    pub fn on_by_default(self) -> bool {
        matches!(self, McpGroup::Read | McpGroup::Run)
    }
}

/// Every tool `reqly-mcp` offers, in display order. A test there keeps
/// this list and the implemented tools in step.
pub const CATALOG: &[(&str, McpGroup)] = {
    use McpGroup::*;
    &[
        ("list_workspaces", Read),
        ("list_collections", Read),
        ("get_tree", Read),
        ("get_request", Read),
        ("search_requests", Read),
        ("list_environments", Read),
        ("get_history", Read),
        ("get_response", Read),
        ("create_request", Edit),
        ("update_request", Edit),
        ("create_folder", Edit),
        ("rename_item", Edit),
        ("move_item", Edit),
        ("duplicate_item", Edit),
        ("delete_item", Edit),
        ("send_request", Run),
        ("render_request", Run),
        ("run_collection", Run),
        ("get_variables", Environments),
        ("set_variables", Environments),
        ("create_environment", Environments),
        ("set_active_environment", Environments),
        ("import_text", Interop),
        ("export_collection", Interop),
    ]
};

pub const DEFAULT_PORT: u16 = 7655;
/// Settings keys; `settings.get`/`settings.set` refuse them.
pub const CONFIG_KEY: &str = "mcp";
pub const TOKEN_KEY: &str = "mcp.token";
const LOG_SIZE: usize = 200;

/// Whether the generic settings commands must leave `key` alone.
pub fn reserved_key(key: &str) -> bool {
    key == CONFIG_KEY || key.starts_with("mcp.")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct McpConfig {
    pub enabled: bool,
    /// Desktop listener port (`127.0.0.1:<port>`).
    pub port: u16,
    /// Explicit choices; a tool missing here follows its group's default.
    pub tools: BTreeMap<String, bool>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            tools: BTreeMap::new(),
        }
    }
}

impl McpConfig {
    pub fn tool_enabled(&self, name: &str, group: McpGroup) -> bool {
        self.tools
            .get(name)
            .copied()
            .unwrap_or_else(|| group.on_by_default())
    }

    fn validate(&self) -> CoreResult<()> {
        if self.port < 1024 {
            return Err(invalid("port must be 1024–65535").with_param("port", self.port));
        }
        if let Some(name) = self
            .tools
            .keys()
            .find(|n| !CATALOG.iter().any(|(c, _)| c == n))
        {
            return Err(invalid("unknown tool").with_param("tool", name.as_str()));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::new("mcp.invalid_config", message)
}

/// The saved configuration (defaults when never saved).
pub async fn load_config(storage: &reqly_storage::Storage) -> CoreResult<McpConfig> {
    let value = SettingsRepo::new(storage).get(CONFIG_KEY).await?;
    Ok(value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

/// Names of the tools the configuration allows.
pub fn enabled_tools(config: &McpConfig) -> BTreeSet<String> {
    CATALOG
        .iter()
        .filter(|(name, group)| config.tool_enabled(name, *group))
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Whether `token` is the MCP token (desktop, server modes `none`/`token`).
pub async fn token_matches(storage: &reqly_storage::Storage, token: &str) -> CoreResult<bool> {
    let stored = SettingsRepo::new(storage).get(TOKEN_KEY).await?;
    Ok(stored
        .as_ref()
        .and_then(|v| v.as_str())
        .is_some_and(|digest| digest == token_hash(token)))
}

async fn new_token(ctx: &AppContext) -> CoreResult<String> {
    let token = random_token();
    SettingsRepo::new(&ctx.storage)
        .set(
            TOKEN_KEY,
            &serde_json::Value::String(token_hash(&token)),
            ctx.clock.as_ref(),
        )
        .await?;
    Ok(token)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpToolInfo {
    pub name: String,
    pub group: McpGroup,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpStatus {
    /// `false` on Android: there is no MCP there.
    pub available: bool,
    pub config: McpConfig,
    pub tools: Vec<McpToolInfo>,
    pub has_token: bool,
    /// The plain token, only right after it was created.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

async fn status(ctx: &AppContext, token: Option<String>) -> CoreResult<McpStatus> {
    let config = load_config(&ctx.storage).await?;
    let has_token = SettingsRepo::new(&ctx.storage)
        .get(TOKEN_KEY)
        .await?
        .is_some();
    Ok(McpStatus {
        available: ctx.platform != Platform::Mobile,
        tools: CATALOG
            .iter()
            .map(|(name, group)| McpToolInfo {
                name: name.to_string(),
                group: *group,
                enabled: config.tool_enabled(name, *group),
            })
            .collect(),
        config,
        has_token,
        token,
    })
}

fn available(ctx: &AppContext) -> CoreResult<()> {
    if ctx.platform == Platform::Mobile {
        return Err(CoreError::new(
            "mcp.unavailable",
            "MCP is not available on this platform",
        ));
    }
    Ok(())
}

/// One `tools/call`, as shown in the settings log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpCall {
    /// Unix milliseconds.
    pub at: i64,
    pub tool: String,
    /// Shortened JSON of the arguments, secrets removed.
    pub arguments: String,
    pub ok: bool,
    #[ts(optional)]
    pub error: Option<String>,
    pub duration_ms: u64,
    /// The user whose token made the call (multi-user server).
    #[ts(optional)]
    pub user: Option<Id>,
}

/// The last calls, newest last (in memory: cleared on restart).
#[derive(Default)]
pub struct McpLog {
    calls: Mutex<VecDeque<McpCall>>,
}

impl McpLog {
    /// Keeps the call and announces it as `mcp.call`.
    pub fn record(&self, events: &EventBus, call: McpCall) {
        events.publish(crate::Event::new("mcp.call", &call).by(call.user));
        if let Ok(mut calls) = self.calls.lock() {
            if calls.len() == LOG_SIZE {
                calls.pop_front();
            }
            calls.push_back(call);
        }
    }

    /// Newest first.
    pub fn recent(&self) -> Vec<McpCall> {
        self.calls
            .lock()
            .map(|c| c.iter().rev().cloned().collect())
            .unwrap_or_default()
    }
}

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
    /// Configuration, tools and whether a token exists.
    McpConfigGet,
    "mcp.config",
    Empty,
    McpStatus,
    |ctx, _input| { status(ctx, None).await }
);

command!(
    /// Saves the configuration; the first enable also creates the token.
    McpSetConfig,
    "mcp.setConfig",
    McpConfig,
    McpStatus,
    |ctx, input| {
        available(ctx)?;
        input.validate()?;
        SettingsRepo::new(&ctx.storage)
            .set(
                CONFIG_KEY,
                &serde_json::to_value(&input).map_err(|e| CoreError::internal(e.to_string()))?,
                ctx.clock.as_ref(),
            )
            .await?;
        let has_token = SettingsRepo::new(&ctx.storage)
            .get(TOKEN_KEY)
            .await?
            .is_some();
        let token = if input.enabled && !has_token {
            Some(new_token(ctx).await?)
        } else {
            None
        };
        ctx.events
            .publish(crate::Event::new("mcp.configChanged", &input));
        status(ctx, token).await
    }
);

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct McpToken {
    pub token: String,
}

command!(
    /// Replaces the MCP token; the old one stops working at once.
    McpRegenerateToken,
    "mcp.regenerateToken",
    Empty,
    McpToken,
    |ctx, _input| {
        available(ctx)?;
        Ok(McpToken {
            token: new_token(ctx).await?,
        })
    }
);

command!(
    /// The last agent calls, newest first.
    McpLogGet,
    "mcp.log",
    Empty,
    Vec<McpCall>,
    |ctx, _input| { Ok(ctx.mcp_log.recent()) }
);

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<McpConfigGet>()
        .register::<McpSetConfig>()
        .register::<McpRegenerateToken>()
        .register::<McpLogGet>();
}
