use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::auth::Auth;
use super::kv::KeyValue;
use super::request::HttpRequest;
use super::settings::RequestSettings;
use crate::id::Id;
use crate::meta::Meta;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Workspace {
    #[serde(flatten)]
    pub meta: Meta,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Collection {
    #[serde(flatten)]
    pub meta: Meta,
    pub workspace_id: Id,
    pub name: String,
    /// Collection-level defaults for request settings.
    pub settings: RequestSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NodeKind {
    Folder,
    Request,
}

/// Settings shared by everything inside a folder.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct FolderData {
    pub headers: Vec<KeyValue>,
    pub auth: Auth,
    pub settings: RequestSettings,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
// Both variants are large; boxing would only complicate (de)serialization.
#[allow(clippy::large_enum_variant)]
pub enum NodeData {
    Folder(FolderData),
    Request(HttpRequest),
}

impl NodeData {
    pub fn kind(&self) -> NodeKind {
        match self {
            Self::Folder(_) => NodeKind::Folder,
            Self::Request(_) => NodeKind::Request,
        }
    }

    pub fn regenerate_row_ids(&mut self) {
        match self {
            Self::Folder(f) => KeyValue::regenerate_ids(&mut f.headers),
            Self::Request(r) => r.regenerate_row_ids(),
        }
    }
}

/// A folder or request in a collection tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Node {
    #[serde(flatten)]
    pub meta: Meta,
    pub collection_id: Id,
    /// `None` = collection root.
    pub parent_id: Option<Id>,
    pub name: String,
    pub data: NodeData,
}

/// Lightweight view of a node for rendering the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeSummary {
    pub id: Id,
    pub collection_id: Id,
    pub parent_id: Option<Id>,
    pub kind: NodeKind,
    pub name: String,
    pub sort_key: crate::SortKey,
    /// Requests only.
    pub method: Option<String>,
    pub url: Option<String>,
    pub created_at: crate::Timestamp,
    pub updated_at: crate::Timestamp,
    pub version: crate::Version,
}

impl From<&Node> for NodeSummary {
    fn from(n: &Node) -> Self {
        let (method, url) = match &n.data {
            NodeData::Request(r) => (Some(r.method.clone()), Some(r.url.clone())),
            NodeData::Folder(_) => (None, None),
        };
        Self {
            id: n.meta.id,
            collection_id: n.collection_id,
            parent_id: n.parent_id,
            kind: n.data.kind(),
            name: n.name.clone(),
            sort_key: n.meta.sort_key.clone(),
            method,
            url,
            created_at: n.meta.created_at,
            updated_at: n.meta.updated_at,
            version: n.meta.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FixedClock, SortKey};

    #[test]
    fn node_json_shape_is_flat_and_tagged() {
        let clock = FixedClock::new(1);
        let node = Node {
            meta: Meta::new(&clock, SortKey::between(None, None).unwrap()),
            collection_id: Id::new(),
            parent_id: None,
            name: "Get users".into(),
            data: NodeData::Request(HttpRequest {
                url: "https://x".into(),
                ..Default::default()
            }),
        };
        let json = serde_json::to_value(&node).unwrap();
        assert!(json.get("id").is_some(), "meta is flattened");
        assert_eq!(json["data"]["kind"], "request");
        assert_eq!(json["data"]["method"], "GET");
        assert_eq!(json["data"]["auth"]["type"], "inherit");
        let back: Node = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let data: NodeData = serde_json::from_value(serde_json::json!({
            "kind": "request", "url": "http://a"
        }))
        .unwrap();
        let NodeData::Request(r) = data else { panic!() };
        assert_eq!(r.method, "GET");
        assert_eq!(r.body.kind, crate::model::BodyKind::None);
    }

    #[test]
    fn auth_variants_serialize_camel_case() {
        let auth = Auth::ApiKey {
            key: "X-Key".into(),
            value: "v".into(),
            location: crate::model::ApiKeyLocation::Query,
        };
        assert_eq!(
            serde_json::to_value(auth).unwrap(),
            serde_json::json!({ "type": "apiKey", "key": "X-Key", "value": "v", "location": "query" })
        );
    }
}
