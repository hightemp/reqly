//! Error model shared across the core.
//!
//! Errors carry a stable machine-readable `code` plus parameters and are never
//! localized here: the UI turns `code` + `params` into a translated message.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, thiserror::Error, Serialize, Deserialize, TS)]
#[error("{code}: {message}")]
#[ts(export)]
pub struct CoreError {
    /// Stable dotted identifier, e.g. `sort_key.invalid`.
    pub code: String,
    /// Developer-facing English description; not shown to end users as-is.
    pub message: String,
    #[ts(type = "Record<string, unknown>")]
    pub params: Map<String, Value>,
}

impl CoreError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            params: Map::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn not_found(entity: &str, id: impl ToString) -> Self {
        Self::new("not_found", format!("{entity} not found"))
            .with_param("entity", entity)
            .with_param("id", id.to_string())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_params() {
        let err = CoreError::new("sort_key.invalid", "bad key").with_param("value", "a0");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "code": "sort_key.invalid",
                "message": "bad key",
                "params": { "value": "a0" }
            })
        );
    }

    #[test]
    fn not_found_has_entity_and_id() {
        let err = CoreError::not_found("request", 42);
        assert_eq!(err.code, "not_found");
        assert_eq!(err.params["entity"], "request");
        assert_eq!(err.params["id"], "42");
    }
}
