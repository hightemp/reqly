//! What syncs: one record per workspace, collection, environment and node.

use reqly_core::Id;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncKind")]
pub enum Kind {
    Workspace,
    Collection,
    Environment,
    Node,
}

impl Kind {
    pub const ALL: [Kind; 4] = [
        Kind::Workspace,
        Kind::Collection,
        Kind::Environment,
        Kind::Node,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Workspace => "workspace",
            Kind::Collection => "collection",
            Kind::Environment => "environment",
            Kind::Node => "node",
        }
    }

    /// Dotted paths (into [`Record::fields`]) of text merged edit by edit;
    /// the text fields of collaboration (ADR-012).
    pub fn text_fields(self) -> &'static [&'static str] {
        match self {
            Kind::Node => &[
                "data.url",
                "data.description",
                "data.body.text",
                "data.body.graphql.query",
                "data.body.graphql.variables",
                "data.scripts.preRequest",
                "data.scripts.afterResponse",
            ],
            Kind::Collection => &["scripts.preRequest", "scripts.afterResponse"],
            Kind::Workspace | Kind::Environment => &[],
        }
    }
}

/// A synced entity without its local-only columns (ADR-013).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncRecord")]
pub struct Record {
    pub kind: Kind,
    pub id: Id,
    /// The workspace it belongs to (its own id for a workspace).
    pub workspace_id: Id,
    pub deleted: bool,
    #[ts(type = "Record<string, unknown>")]
    pub fields: Map<String, Value>,
}

impl Record {
    /// Same entity, same content.
    pub fn same_content(&self, other: &Record) -> bool {
        self.deleted == other.deleted && self.fields == other.fields
    }

    /// The parent node id of a node record.
    pub fn parent_id(&self) -> Option<Id> {
        self.fields
            .get("parentId")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
    }
}

/// Orders records so that everything a record refers to comes first:
/// workspaces, collections, environments, then nodes parents first.
pub fn dependency_order(records: &mut Vec<Record>) {
    records.sort_by_key(|r| r.kind);
    let start = records.partition_point(|r| r.kind != Kind::Node);
    let nodes: Vec<Record> = records.drain(start..).collect();
    let ids: std::collections::HashSet<Id> = nodes.iter().map(|r| r.id).collect();
    let mut placed: std::collections::HashSet<Id> = std::collections::HashSet::new();
    let mut pending = nodes;
    while !pending.is_empty() {
        let before = pending.len();
        let (ready, rest): (Vec<Record>, Vec<Record>) = pending.into_iter().partition(|r| {
            r.parent_id()
                .is_none_or(|p| !ids.contains(&p) || placed.contains(&p))
        });
        placed.extend(ready.iter().map(|r| r.id));
        records.extend(ready);
        pending = rest;
        if pending.len() == before {
            // A cycle cannot be stored anyway; keep the rest as is.
            records.append(&mut pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: Id, parent: Option<Id>) -> Record {
        let mut fields = Map::new();
        fields.insert("parentId".into(), json!(parent.map(|p| p.to_string())));
        Record {
            kind: Kind::Node,
            id,
            workspace_id: Id::new(),
            deleted: false,
            fields,
        }
    }

    #[test]
    fn parents_come_first() {
        let (a, b, c) = (Id::new(), Id::new(), Id::new());
        let ws = Record {
            kind: Kind::Workspace,
            id: Id::new(),
            workspace_id: Id::new(),
            deleted: false,
            fields: Map::new(),
        };
        let mut records = vec![
            node(c, Some(b)),
            node(b, Some(a)),
            ws.clone(),
            node(a, None),
        ];
        dependency_order(&mut records);
        let order: Vec<Id> = records.iter().map(|r| r.id).collect();
        assert_eq!(order, vec![ws.id, a, b, c]);
    }
}
