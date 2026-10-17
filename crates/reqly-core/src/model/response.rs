use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::clock::Timestamp;
use crate::error::CoreError;
use crate::id::Id;

/// One execution of a request, kept in history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResponseRecord {
    pub id: Id,
    pub request_id: Id,
    pub workspace_id: Id,
    pub created_at: Timestamp,
    /// `None` when the request failed before a response arrived.
    pub status: Option<u16>,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    /// Blob hash of the decoded body.
    pub body_hash: Option<String>,
    pub body_size: u64,
    pub headers_size: u64,
    pub content_type: Option<String>,
    /// `Timings` from reqly-http.
    #[ts(type = "import('./Timings').Timings")]
    pub timings: Value,
    /// `TimelineEntry[]` from reqly-http.
    #[ts(type = "import('./TimelineEntry').TimelineEntry[]")]
    pub timeline: Value,
    /// What was actually sent: method, url, headers, body preview.
    #[ts(type = "import('./RequestSnapshot').RequestSnapshot")]
    pub request_snapshot: Value,
    pub redirects: u32,
    pub final_url: String,
    pub remote_addr: Option<String>,
    pub error: Option<CoreError>,
}

/// Row in the response history dropdown / workspace log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResponseSummary {
    pub id: Id,
    pub request_id: Id,
    pub created_at: Timestamp,
    pub status: Option<u16>,
    pub status_text: String,
    pub total_ms: Option<f64>,
    pub body_size: u64,
    pub method: String,
    pub url: String,
    pub error_code: Option<String>,
}
