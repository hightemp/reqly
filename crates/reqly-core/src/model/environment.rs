use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::id::Id;
use crate::meta::Meta;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EnvironmentKind {
    /// Shared by every collection of a workspace (lowest precedence).
    Global,
    /// The collection's base environment (always present).
    Base,
    /// A switchable collection environment (dev, stage, prod…).
    Sub,
}

/// A set of variables. Values are JSON, so environments can hold objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Environment {
    #[serde(flatten)]
    pub meta: Meta,
    pub workspace_id: Id,
    /// `None` for global environments.
    pub collection_id: Option<Id>,
    pub kind: EnvironmentKind,
    pub name: String,
    /// CSS color shown next to the environment name.
    pub color: Option<String>,
    #[ts(type = "Record<string, unknown>")]
    pub data: Map<String, Value>,
    /// Top-level keys whose values are secret (masked, encrypted at rest, not exported).
    pub secrets: Vec<String>,
}
