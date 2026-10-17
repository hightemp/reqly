//! What an import could not convert exactly.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::detect::Format;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReportKind {
    /// Not supported by Reqly yet; kept in the report (and `raw`).
    Unsupported,
    /// Imported with a different representation.
    Converted,
    /// Informational.
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReportEntry {
    pub kind: ReportKind,
    /// Stable code for translation, e.g. `auth_unsupported`.
    pub code: String,
    /// Location in the source: `Collection / Folder / Request`.
    pub path: String,
    /// English description with details.
    pub message: String,
    /// The original data that could not be imported.
    #[ts(type = "unknown")]
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ImportReport {
    pub format: Format,
    pub entries: Vec<ReportEntry>,
}

impl ImportReport {
    pub fn push(&mut self, kind: ReportKind, code: &str, path: &str, message: impl Into<String>) {
        self.entries.push(ReportEntry {
            kind,
            code: code.into(),
            path: path.into(),
            message: message.into(),
            raw: None,
        });
    }

    pub fn push_raw(
        &mut self,
        kind: ReportKind,
        code: &str,
        path: &str,
        message: impl Into<String>,
        raw: Value,
    ) {
        self.push(kind, code, path, message);
        if let Some(last) = self.entries.last_mut() {
            last.raw = Some(raw);
        }
    }

    pub fn unsupported(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == ReportKind::Unsupported)
            .count()
    }

    /// Plain-text version for copying / saving.
    pub fn to_text(&self) -> String {
        let mut out = format!("Import report ({})\n", self.format.label());
        if self.entries.is_empty() {
            out.push_str("Everything was imported.\n");
        }
        for e in &self.entries {
            let kind = match e.kind {
                ReportKind::Unsupported => "UNSUPPORTED",
                ReportKind::Converted => "CONVERTED",
                ReportKind::Info => "INFO",
            };
            out.push_str(&format!("[{kind}] {}: {}\n", e.path, e.message));
            if let Some(raw) = &e.raw {
                out.push_str(&format!("    {raw}\n"));
            }
        }
        out
    }
}

/// Joins path segments for report entries.
pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent} / {name}")
    }
}
