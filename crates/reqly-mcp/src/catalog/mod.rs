//! The tools of the catalog (`reqly_api::commands::mcp::CATALOG`).

mod mask;
mod read;

use reqly_api::commands::mcp::CATALOG;
use reqly_core::{CoreError, CoreResult};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Annotations, BoxFuture, Tool, ToolContext, Tools};

pub use mask::HIDDEN;
pub(crate) use mask::secret_name;

/// Every tool an MCP server of Reqly offers.
pub fn catalog_tools() -> Tools {
    let mut tools = Tools::new();
    for tool in read::tools() {
        tools.add(tool);
    }
    tools
}

/// A tool of the catalog; its group comes from the catalog.
fn tool<F>(
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    annotations: Annotations,
    handler: F,
) -> Tool
where
    F: Fn(ToolContext, Value) -> BoxFuture<'static, CoreResult<Value>> + Send + Sync + 'static,
{
    let group = CATALOG
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, g)| *g)
        .unwrap_or_else(|| panic!("tool {name} is not in the catalog"));
    Tool::new(name, group, description, input_schema, annotations, handler)
}

/// Typed arguments of a tool.
fn args<T: DeserializeOwned>(value: Value) -> CoreResult<T> {
    serde_json::from_value(value)
        .map_err(|e| CoreError::new("mcp.invalid_arguments", e.to_string()))
}

fn invalid(message: &str) -> CoreError {
    CoreError::new("mcp.invalid_arguments", message)
}
