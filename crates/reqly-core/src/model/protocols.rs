//! Requests other than plain HTTP: GraphQL (an HTTP body), server-sent events
//! (an HTTP request whose response is a stream), WebSocket and gRPC.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::auth::Auth;
use super::body::FileRef;
use super::kv::KeyValue;
use super::request::Scripts;
use super::settings::RequestSettings;

/// What a request node speaks; shown as a badge in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Protocol {
    #[default]
    Http,
    Graphql,
    Sse,
    WebSocket,
    Grpc,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
            Self::Sse => "sse",
            Self::WebSocket => "webSocket",
            Self::Grpc => "grpc",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "graphql" => Self::Graphql,
            "sse" => Self::Sse,
            "webSocket" => Self::WebSocket,
            "grpc" => Self::Grpc,
            _ => Self::Http,
        }
    }
}

/// GraphQL operation sent as an HTTP body (`BodyKind::Graphql`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GraphqlBody {
    pub query: String,
    /// JSON object text (templates allowed).
    pub variables: String,
    pub operation_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MessageFormat {
    #[default]
    Text,
    Json,
    /// Base64 in the editor, sent as a binary frame.
    Binary,
}

/// A saved WebSocket message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct MessageTemplate {
    pub id: crate::Id,
    pub name: String,
    pub format: MessageFormat,
    pub body: String,
}

impl Default for MessageTemplate {
    fn default() -> Self {
        Self {
            id: crate::Id::new(),
            name: String::new(),
            format: MessageFormat::Text,
            body: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct WebSocketRequest {
    /// `ws://` or `wss://` (`http(s)://` is accepted and converted).
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub auth: Auth,
    pub settings: RequestSettings,
    /// `Sec-WebSocket-Protocol` values.
    pub subprotocols: Vec<String>,
    /// The message in the composer.
    pub draft: MessageTemplate,
    pub templates: Vec<MessageTemplate>,
    pub auto_reconnect: bool,
    pub reconnect_delay_ms: u64,
    pub description: String,
    #[ts(type = "Record<string, unknown>")]
    pub variables: serde_json::Map<String, serde_json::Value>,
    pub scripts: Scripts,
}

impl Default for WebSocketRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            auth: Auth::Inherit,
            settings: RequestSettings::default(),
            subprotocols: Vec::new(),
            draft: MessageTemplate::default(),
            templates: Vec::new(),
            auto_reconnect: false,
            reconnect_delay_ms: 2000,
            description: String::new(),
            variables: serde_json::Map::new(),
            scripts: Scripts::default(),
        }
    }
}

/// Where gRPC service definitions come from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ProtoSource {
    /// Ask the server (gRPC server reflection) instead of using files.
    pub reflection: bool,
    pub files: Vec<FileRef>,
    /// `.proto` sources kept in the request itself (imports, pasted text).
    pub inline: Vec<InlineProto>,
    /// Directories searched for `import`s (desktop).
    pub import_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct InlineProto {
    /// Path used by `import` statements, e.g. `greet/v1/greet.proto`.
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GrpcRequest {
    /// `host:port`, `grpc://host:port` (plaintext) or `grpcs://host:port` (TLS).
    pub url: String,
    pub proto: ProtoSource,
    /// Fully qualified, e.g. `helloworld.Greeter`.
    pub service: String,
    pub method: String,
    /// Request message(s) as JSON; for client streaming, the composer.
    pub message: String,
    pub metadata: Vec<KeyValue>,
    pub auth: Auth,
    pub settings: RequestSettings,
    pub description: String,
    #[ts(type = "Record<string, unknown>")]
    pub variables: serde_json::Map<String, serde_json::Value>,
    pub scripts: Scripts,
}

impl Default for GrpcRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            proto: ProtoSource::default(),
            service: String::new(),
            method: String::new(),
            message: "{}".into(),
            metadata: Vec::new(),
            auth: Auth::Inherit,
            settings: RequestSettings::default(),
            description: String::new(),
            variables: serde_json::Map::new(),
            scripts: Scripts::default(),
        }
    }
}
