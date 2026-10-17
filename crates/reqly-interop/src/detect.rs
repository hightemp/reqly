//! Format detection from content.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Format {
    #[default]
    Reqly,
}

impl Format {
    pub fn id(self) -> &'static str {
        match self {
            Self::Reqly => "reqly",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reqly => "Reqly",
        }
    }
}

/// Parses JSON or YAML into a JSON value.
pub fn parse_structured(text: &str) -> Option<Value> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).ok();
    }
    serde_yaml_ng::from_str::<Value>(trimmed)
        .ok()
        .filter(Value::is_object)
}

pub fn detect(sources: &[Source]) -> Option<Format> {
    let first = sources.first()?;
    let value = parse_structured(&first.content)?;
    let obj = value.as_object()?;
    if obj.get("format").and_then(Value::as_str) == Some("reqly") {
        return Some(Format::Reqly);
    }
    None
}
