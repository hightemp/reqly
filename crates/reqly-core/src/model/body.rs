use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::kv::KeyValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BodyKind {
    #[default]
    None,
    Json,
    Xml,
    Yaml,
    Text,
    Html,
    FormUrlencoded,
    Multipart,
    Binary,
}

impl BodyKind {
    /// Content-Type set automatically unless the user provides one.
    pub fn default_content_type(self) -> Option<&'static str> {
        match self {
            Self::None | Self::Multipart | Self::Binary => None,
            Self::Json => Some("application/json"),
            Self::Xml => Some("application/xml"),
            Self::Yaml => Some("application/yaml"),
            Self::Text => Some("text/plain"),
            Self::Html => Some("text/html"),
            Self::FormUrlencoded => Some("application/x-www-form-urlencoded"),
        }
    }

    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::Json | Self::Xml | Self::Yaml | Self::Text | Self::Html
        )
    }
}

/// A file attached to a request: a local path (desktop) or an uploaded blob (server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum FileRef {
    Path {
        path: String,
    },
    Blob {
        hash: String,
        name: String,
        size: u64,
    },
}

/// Request body. All variants' data is kept so switching `kind` loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Body {
    pub kind: BodyKind,
    /// Text for json/xml/yaml/text/html.
    pub text: String,
    /// Fields for form-urlencoded and multipart.
    pub fields: Vec<KeyValue>,
    /// File for binary bodies.
    pub file: Option<FileRef>,
}
