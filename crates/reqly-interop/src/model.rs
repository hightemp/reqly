//! Format-independent result of an import.

use reqly_core::model::{FolderData, HttpRequest, NodeData, RequestSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::detect::Format;
use crate::report::ImportReport;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export)]
pub enum ImportedItem {
    Folder {
        /// Id in the source, used to rewrite references (response chaining).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        id: Option<String>,
        name: String,
        #[serde(default)]
        data: FolderData,
        #[serde(default)]
        children: Vec<ImportedItem>,
    },
    Request {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        id: Option<String>,
        name: String,
        #[serde(default)]
        data: HttpRequest,
    },
}

impl ImportedItem {
    pub fn folder(name: impl Into<String>, data: FolderData, children: Vec<ImportedItem>) -> Self {
        Self::Folder {
            id: None,
            name: name.into(),
            data,
            children,
        }
    }

    pub fn request(name: impl Into<String>, data: HttpRequest) -> Self {
        Self::Request {
            id: None,
            name: name.into(),
            data,
        }
    }

    pub fn with_id(mut self, source_id: impl Into<String>) -> Self {
        match &mut self {
            Self::Folder { id, .. } | Self::Request { id, .. } => *id = Some(source_id.into()),
        }
        self
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Folder { id, .. } | Self::Request { id, .. } => id.as_deref(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } | Self::Request { name, .. } => name,
        }
    }

    pub fn node_data(&self) -> NodeData {
        match self {
            Self::Folder { data, .. } => NodeData::Folder(data.clone()),
            Self::Request { data, .. } => NodeData::Request(data.clone()),
        }
    }

    pub fn children(&self) -> &[ImportedItem] {
        match self {
            Self::Folder { children, .. } => children,
            Self::Request { .. } => &[],
        }
    }

    /// Number of requests in this subtree.
    pub fn request_count(&self) -> usize {
        match self {
            Self::Folder { children, .. } => children.iter().map(Self::request_count).sum(),
            Self::Request { .. } => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ImportedEnvironment {
    pub name: String,
    pub color: Option<String>,
    #[ts(type = "Record<string, unknown>")]
    pub data: Map<String, Value>,
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ImportedCollection {
    pub name: String,
    pub description: String,
    pub settings: RequestSettings,
    /// Base environment variables (merged into the collection's base environment).
    #[ts(type = "Record<string, unknown>")]
    pub variables: Map<String, Value>,
    pub variable_secrets: Vec<String>,
    /// Switchable environments of the collection.
    pub environments: Vec<ImportedEnvironment>,
    pub items: Vec<ImportedItem>,
}

impl ImportedCollection {
    pub fn request_count(&self) -> usize {
        self.items.iter().map(ImportedItem::request_count).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ImportedCookie {
    pub domain: String,
    pub include_subdomains: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Unix seconds; `None` = session cookie.
    pub expires: Option<i64>,
    pub name: String,
    pub value: String,
}

/// A stored response carried by the Reqly format (history export).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportedResponse {
    /// Source id of the request (an item `id`).
    pub request_id: String,
    /// `ResponseRecord` as JSON.
    #[ts(type = "unknown")]
    pub record: Value,
    /// Body bytes, base64.
    pub body_base64: Option<String>,
}

/// Everything an exporter may write.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportData {
    pub workspace: String,
    pub collections: Vec<ImportedCollection>,
    pub environments: Vec<ImportedEnvironment>,
    pub cookies: Vec<ImportedCookie>,
    pub history: Vec<ExportedResponse>,
}

impl ExportData {
    /// Blanks secret values (keys stay so users know what to fill in).
    pub fn strip_secrets(&mut self) {
        fn strip(data: &mut Map<String, Value>, secrets: &[String]) {
            for key in secrets {
                if let Some(v) = data.get_mut(key) {
                    *v = Value::String(String::new());
                }
            }
        }
        for c in &mut self.collections {
            strip(&mut c.variables, &c.variable_secrets);
            for e in &mut c.environments {
                strip(&mut e.data, &e.secrets);
            }
        }
        for e in &mut self.environments {
            strip(&mut e.data, &e.secrets);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ImportBundle {
    pub format: Format,
    pub collections: Vec<ImportedCollection>,
    /// Environments not tied to a collection (Postman environments,
    /// Insomnia / Reqly global environments).
    pub environments: Vec<ImportedEnvironment>,
    pub cookies: Vec<ImportedCookie>,
    pub history: Vec<ExportedResponse>,
    pub report: ImportReport,
}
