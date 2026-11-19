//! Agent tools (ADR-014, ADR-015): a curated set of operations written for
//! language models, implemented on top of the command registry so access
//! checks, the trash, history and live UI updates work as they do for the
//! user. Served to MCP clients by `reqly-mcp` and to the AI chat.

mod catalog;
mod tools;

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use crate::commands::AppContext;
use crate::registry::Registry;

pub use crate::commands::mcp::McpGroup as Group;
pub use catalog::{HIDDEN, catalog_tools, secret_name};
pub use tools::{Annotations, Tool, ToolContext, Tools};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The command registry tools dispatch to, built on first use. Every app
/// serves the same commands, so one shared instance lets code that only has
/// a borrowed `AppContext` (a command such as `ai.send`) run tools.
pub fn registry() -> &'static Registry<AppContext> {
    static REGISTRY: OnceLock<Registry<AppContext>> = OnceLock::new();
    REGISTRY.get_or_init(crate::commands::registry)
}
