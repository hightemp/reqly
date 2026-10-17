//! Workspace-wide node search (command palette, global search).
//!
//! Query syntax: free words (prefix match on name, URL, method, description),
//! `method:POST` (exact method) and `url:part` (prefix match within the URL).

use reqly_core::model::NodeSummary;
use reqly_core::{CoreResult, Id};
use sqlx::{QueryBuilder, Sqlite};

use crate::Storage;
use crate::db::db_err;
use crate::tree::summary_from_row;

/// A quoted FTS5 prefix term; quoting neutralizes FTS operators in user input.
fn fts_term(word: &str) -> String {
    format!("\"{}\"*", word.replace('"', "\"\""))
}

struct ParsedQuery {
    fts: Vec<String>,
    method: Option<String>,
}

fn parse(query: &str) -> ParsedQuery {
    let mut fts = Vec::new();
    let mut method = None;
    for token in query.split_whitespace() {
        if let Some(m) = token.strip_prefix("method:") {
            if !m.is_empty() {
                method = Some(m.to_uppercase());
            }
        } else if let Some(u) = token.strip_prefix("url:") {
            if !u.is_empty() {
                fts.push(format!("url : {}", fts_term(u)));
            }
        } else {
            fts.push(fts_term(token));
        }
    }
    ParsedQuery { fts, method }
}

pub async fn search_nodes(
    storage: &Storage,
    workspace_id: Id,
    query: &str,
    limit: u32,
) -> CoreResult<Vec<NodeSummary>> {
    let parsed = parse(query);
    if parsed.fts.is_empty() && parsed.method.is_none() {
        return Ok(Vec::new());
    }

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT n.id, n.collection_id, n.parent_id, n.kind, n.protocol, n.name, n.method, n.url, n.data, \
         n.sort_key, n.created_at, n.updated_at, n.version, n.deleted_at \
         FROM nodes n JOIN collections c ON c.id = n.collection_id ",
    );
    if !parsed.fts.is_empty() {
        qb.push("JOIN nodes_fts f ON f.rowid = n.rowid AND nodes_fts MATCH ")
            .push_bind(parsed.fts.join(" AND "))
            .push(" ");
    }
    qb.push("WHERE n.deleted_at IS NULL AND c.deleted_at IS NULL AND c.workspace_id = ")
        .push_bind(workspace_id.to_string());
    if let Some(method) = parsed.method {
        qb.push(" AND n.method = ").push_bind(method);
    }
    qb.push(if parsed.fts.is_empty() {
        " ORDER BY n.name"
    } else {
        " ORDER BY f.rank"
    });
    qb.push(" LIMIT ").push_bind(limit as i64);

    let rows = qb.build().fetch_all(storage.pool()).await.map_err(db_err)?;
    rows.iter().map(summary_from_row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_prefixes_terms() {
        let q = parse("say \"hi\" method:post url:/v1");
        assert_eq!(q.fts, ["\"say\"*", "\"\"\"hi\"\"\"*", "url : \"/v1\"*"]);
        assert_eq!(q.method.as_deref(), Some("POST"));
    }
}
