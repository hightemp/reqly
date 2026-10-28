use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::assertion::Assertion;
use super::auth::Auth;
use super::body::Body;
use super::kv::KeyValue;
use super::settings::RequestSettings;

/// An HTTP request as stored. `url` holds no query string: query parameters
/// live in `params` and are appended (enabled ones only) when sending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub params: Vec<KeyValue>,
    /// Values for `:name` segments in `url`.
    pub path_params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body: Body,
    pub auth: Auth,
    pub settings: RequestSettings,
    /// Markdown.
    pub description: String,
    /// Request-level variables (highest precedence below runtime values).
    #[ts(type = "Record<string, unknown>")]
    pub variables: serde_json::Map<String, serde_json::Value>,
    /// Imported scripts; not executed until scripting lands (stage 6).
    pub scripts: Scripts,
    /// No-code tests on the response.
    pub assertions: Vec<Assertion>,
    /// Server-sent events: keep the connection open and parse the stream.
    pub event_stream: bool,
}

/// Pre-request and after-response scripts.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Scripts {
    pub pre_request: String,
    pub after_response: String,
}

impl Scripts {
    pub fn is_empty(&self) -> bool {
        self.pre_request.trim().is_empty() && self.after_response.trim().is_empty()
    }
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            params: Vec::new(),
            path_params: Vec::new(),
            headers: Vec::new(),
            body: Body::default(),
            auth: Auth::Inherit,
            settings: RequestSettings::default(),
            description: String::new(),
            variables: serde_json::Map::new(),
            scripts: Scripts::default(),
            assertions: Vec::new(),
            event_stream: false,
        }
    }
}

impl HttpRequest {
    pub fn protocol(&self) -> super::Protocol {
        if self.body.kind == super::BodyKind::Graphql {
            super::Protocol::Graphql
        } else if self.event_stream {
            super::Protocol::Sse
        } else {
            super::Protocol::Http
        }
    }

    pub fn regenerate_row_ids(&mut self) {
        KeyValue::regenerate_ids(&mut self.params);
        KeyValue::regenerate_ids(&mut self.path_params);
        KeyValue::regenerate_ids(&mut self.headers);
        KeyValue::regenerate_ids(&mut self.body.fields);
        self.assertions
            .iter_mut()
            .for_each(|a| a.id = crate::id::Id::new());
    }
}
