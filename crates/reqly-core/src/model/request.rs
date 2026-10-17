use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
        }
    }
}

impl HttpRequest {
    pub fn regenerate_row_ids(&mut self) {
        KeyValue::regenerate_ids(&mut self.params);
        KeyValue::regenerate_ids(&mut self.path_params);
        KeyValue::regenerate_ids(&mut self.headers);
        KeyValue::regenerate_ids(&mut self.body.fields);
    }
}
