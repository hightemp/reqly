//! Messages of `sync.pull` and `sync.push` (ADR-013).

use reqly_core::Id;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::record::Record;

/// How far a client has pulled a workspace: the change sequence of the
/// newest change it got (0: nothing yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cursor {
    pub workspace_id: Id,
    #[ts(type = "number")]
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullInput {
    pub cursors: Vec<Cursor>,
    /// Include the values of secret environment variables.
    #[serde(default)]
    pub secrets: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullOutput {
    /// Changed records, oldest change first.
    pub records: Vec<Record>,
    pub cursors: Vec<Cursor>,
    /// Requested workspaces the caller cannot read (gone or no access).
    pub unavailable: Vec<Id>,
}

/// A local change: the record as the client has it and as it last got it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Change {
    pub record: Record,
    pub base: Option<Record>,
    /// Write `record` as is, even over a conflict (the user chose it).
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushInput {
    pub changes: Vec<Change>,
    #[serde(default)]
    pub secrets: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "status")]
#[ts(export)]
pub enum PushResult {
    /// Stored; `record` is what the server has now (merged).
    Applied { record: Record },
    /// Deleted against changed: nothing was stored; `current` is the
    /// server's record.
    Conflict { record: Record, current: Record },
    /// Not allowed or invalid (`code` is an error code).
    Rejected { record: Record, code: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushOutput {
    pub results: Vec<PushResult>,
}
