//! `reqly-mcp`: the MCP server for agents (ADR-014).
//!
//! Streamable HTTP without sessions or streams: every `POST /mcp` carries one
//! JSON-RPC message and gets a JSON response (or `202` for notifications).
//! Tools are the agent catalog of `reqly-api`, shared with the AI chat.
//! The apps mount [`router`] and decide who may call it through a [`Gate`].

mod gate;
mod http;
mod local;
mod protocol;

pub use gate::{SettingsGate, TokenKind};
pub use http::{McpState, OriginPolicy, router};
pub use local::serve_local;
pub use protocol::{PROTOCOL_VERSION, SUPPORTED_VERSIONS};
pub use reqly_api::agent::{
    Annotations, BoxFuture, Group, HIDDEN, Tool, ToolContext, Tools, catalog_tools,
};

use reqly_api::access::Caller;
use std::collections::BTreeSet;

/// Which tools a session may see and call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowed {
    All,
    Only(BTreeSet<String>),
}

impl Allowed {
    pub fn permits(&self, tool: &str) -> bool {
        match self {
            Allowed::All => true,
            Allowed::Only(names) => names.contains(tool),
        }
    }
}

/// An authenticated MCP caller.
#[derive(Debug, Clone)]
pub struct Session {
    /// Who the tools run as (ADR-010).
    pub caller: Caller,
    pub allowed: Allowed,
}

/// Why a request was turned away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// MCP is turned off: the endpoint does not exist (`404`).
    Disabled,
    /// No token or a wrong one (`401`).
    Unauthorized,
}

/// Decides, per request, whether MCP is on, who the bearer token belongs to
/// and which tools they get. The settings may change between requests.
pub trait Gate: Send + Sync + 'static {
    fn open<'a>(&'a self, token: Option<&'a str>) -> BoxFuture<'a, Result<Session, Denied>>;
}
