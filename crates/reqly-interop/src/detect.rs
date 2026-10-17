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
    Insomnia,
    Postman,
    Curl,
    Har,
    OpenApi,
    Bruno,
}

impl Format {
    pub fn id(self) -> &'static str {
        match self {
            Self::Reqly => "reqly",
            Self::Insomnia => "insomnia",
            Self::Postman => "postman",
            Self::Curl => "curl",
            Self::Har => "har",
            Self::OpenApi => "openapi",
            Self::Bruno => "bruno",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reqly => "Reqly",
            Self::Insomnia => "Insomnia",
            Self::Postman => "Postman",
            Self::Curl => "cURL",
            Self::Har => "HAR",
            Self::OpenApi => "OpenAPI / Swagger",
            Self::Bruno => "Bruno",
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
    if crate::bruno::is_bruno(sources) {
        return Some(Format::Bruno);
    }
    if crate::curl::looks_like_curl(&first.content) {
        return Some(Format::Curl);
    }
    let value = parse_structured(&first.content)?;
    let obj = value.as_object()?;
    if obj.get("format").and_then(Value::as_str) == Some("reqly") {
        return Some(Format::Reqly);
    }
    if crate::insomnia::is_v4(&value) || crate::insomnia::is_v5(&value) {
        return Some(Format::Insomnia);
    }
    if crate::postman::is_collection(&value) || crate::postman::is_environment(&value) {
        return Some(Format::Postman);
    }
    if crate::openapi::is_openapi(&value) {
        return Some(Format::OpenApi);
    }
    if crate::har::is_har(&value) {
        return Some(Format::Har);
    }
    None
}
