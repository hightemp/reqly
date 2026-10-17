//! What was actually sent, stored with each history record.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::prepare::{PreparedBody, PreparedRequest};

/// Request bodies longer than this are cut in the snapshot.
const BODY_PREVIEW_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RequestSnapshot {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Text preview of the body (lossy UTF-8, truncated).
    pub body_preview: Option<String>,
    pub body_truncated: bool,
    /// Short description for non-text bodies, e.g. `multipart: 3 parts`.
    pub body_note: Option<String>,
}

impl From<&PreparedRequest> for RequestSnapshot {
    fn from(r: &PreparedRequest) -> Self {
        let (body_preview, body_truncated, body_note) = match &r.body {
            PreparedBody::None => (None, false, None),
            PreparedBody::Bytes(b) => {
                let cut = &b[..b.len().min(BODY_PREVIEW_LIMIT)];
                (
                    Some(String::from_utf8_lossy(cut).into_owned()),
                    b.len() > BODY_PREVIEW_LIMIT,
                    None,
                )
            }
            PreparedBody::File(p) => (None, false, Some(format!("file: {}", p.display()))),
            PreparedBody::Blob(h) => (None, false, Some(format!("file: blob {h}"))),
            PreparedBody::Multipart(parts) => (
                None,
                false,
                Some(format!("multipart: {} parts", parts.len())),
            ),
        };
        Self {
            method: r.method.clone(),
            url: r.url.clone(),
            headers: r.headers.clone(),
            body_preview,
            body_truncated,
            body_note,
        }
    }
}
