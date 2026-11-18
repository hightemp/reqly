//! Tool definitions and the context they run in.

use std::sync::Arc;

use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, dispatch_as};
use reqly_api::registry::Registry;
use reqly_core::CoreResult;
use serde::Serialize;
use serde_json::{Value, json};

use crate::BoxFuture;

/// Settings groups of tools (PRD §7.22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    Read,
    Edit,
    Run,
    Environments,
    Interop,
}

/// MCP tool annotations: hints for clients, not guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

impl Default for Annotations {
    /// A tool that changes data without destroying it.
    fn default() -> Self {
        Self {
            read_only_hint: false,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }
    }
}

impl Annotations {
    pub fn read_only() -> Self {
        Self {
            read_only_hint: true,
            idempotent_hint: true,
            ..Self::default()
        }
    }

    pub fn destructive() -> Self {
        Self {
            destructive_hint: true,
            ..Self::default()
        }
    }

    /// Talks to the outside world (sends requests).
    pub fn open_world(self) -> Self {
        Self {
            open_world_hint: true,
            ..self
        }
    }

    pub fn idempotent(self) -> Self {
        Self {
            idempotent_hint: true,
            ..self
        }
    }
}

/// What a tool runs with: the core and the caller of the session.
#[derive(Clone)]
pub struct ToolContext {
    pub ctx: Arc<AppContext>,
    pub registry: Arc<Registry<AppContext>>,
    pub caller: Caller,
}

impl ToolContext {
    /// Runs a registry command as the session's caller, with its access checks.
    pub async fn call(&self, name: &str, input: Value) -> CoreResult<Value> {
        dispatch_as(&self.registry, &self.ctx, self.caller, name, input).await
    }
}

type Handler =
    Arc<dyn Fn(ToolContext, Value) -> BoxFuture<'static, CoreResult<Value>> + Send + Sync>;

pub struct Tool {
    pub name: &'static str,
    pub group: Group,
    pub description: &'static str,
    pub input_schema: Value,
    pub annotations: Annotations,
    handler: Handler,
}

impl Tool {
    pub fn new<F>(
        name: &'static str,
        group: Group,
        description: &'static str,
        input_schema: Value,
        annotations: Annotations,
        handler: F,
    ) -> Self
    where
        F: Fn(ToolContext, Value) -> BoxFuture<'static, CoreResult<Value>> + Send + Sync + 'static,
    {
        Self {
            name,
            group,
            description,
            input_schema,
            annotations,
            handler: Arc::new(handler),
        }
    }

    pub fn run(&self, cx: ToolContext, args: Value) -> BoxFuture<'static, CoreResult<Value>> {
        (self.handler)(cx, args)
    }

    /// The entry of `tools/list`.
    pub fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": self.annotations,
        })
    }
}

/// The tools an MCP server offers, in registration order.
#[derive(Default)]
pub struct Tools {
    tools: Vec<Tool>,
}

impl Tools {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a tool. Panics on duplicate names: that is a programming error.
    pub fn add(&mut self, tool: Tool) -> &mut Self {
        assert!(
            self.get(tool.name).is_none(),
            "tool {} added twice",
            tool.name
        );
        self.tools.push(tool);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tool> {
        self.tools.iter()
    }
}
