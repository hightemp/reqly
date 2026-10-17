use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::id::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KeyValueKind {
    #[default]
    Text,
    /// Value is a file reference (multipart fields).
    File,
}

/// A row in params / headers / form editors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct KeyValue {
    pub id: Id,
    pub enabled: bool,
    pub name: String,
    pub value: String,
    pub description: String,
    pub kind: KeyValueKind,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self {
            id: Id::new(),
            enabled: true,
            name: String::new(),
            value: String::new(),
            description: String::new(),
            kind: KeyValueKind::Text,
        }
    }
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Same content with fresh ids (used when duplicating).
    pub fn regenerate_ids(rows: &mut [KeyValue]) {
        rows.iter_mut().for_each(|r| r.id = Id::new());
    }
}
