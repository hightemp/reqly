//! Cached GraphQL schemas, one per request.

use reqly_core::{Clock, CoreResult, Id};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use ts_rs::TS;

use crate::Storage;
use crate::db::db_err;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphqlSchema {
    /// `introspection` (JSON of `{ __schema }`) or `sdl`.
    pub format: String,
    /// Where it came from: the endpoint URL or a file name.
    pub source: String,
    pub schema: String,
    /// Unix milliseconds.
    pub fetched_at: i64,
}

pub struct GraphqlRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> GraphqlRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    pub async fn get(&self, node_id: Id) -> CoreResult<Option<GraphqlSchema>> {
        let row = sqlx::query(
            "SELECT format, source, schema, fetched_at FROM graphql_schemas WHERE node_id = ?",
        )
        .bind(node_id.to_string())
        .fetch_optional(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(row.map(|r| GraphqlSchema {
            format: r.get("format"),
            source: r.get("source"),
            schema: r.get("schema"),
            fetched_at: r.get("fetched_at"),
        }))
    }

    pub async fn put(
        &self,
        node_id: Id,
        format: &str,
        source: &str,
        schema: &str,
    ) -> CoreResult<GraphqlSchema> {
        let now = self.clock.now().0;
        sqlx::query(
            "INSERT INTO graphql_schemas (node_id, format, source, schema, fetched_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(node_id) DO UPDATE SET format = excluded.format, source = excluded.source, \
             schema = excluded.schema, fetched_at = excluded.fetched_at",
        )
        .bind(node_id.to_string())
        .bind(format)
        .bind(source)
        .bind(schema)
        .bind(now)
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(GraphqlSchema {
            format: format.into(),
            source: source.into(),
            schema: schema.into(),
            fetched_at: now,
        })
    }

    pub async fn delete(&self, node_id: Id) -> CoreResult<()> {
        sqlx::query("DELETE FROM graphql_schemas WHERE node_id = ?")
            .bind(node_id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqly_core::FixedClock;

    #[tokio::test]
    async fn round_trip() {
        let s = Storage::in_memory().await.unwrap();
        let clock = FixedClock::new(5);
        let repo = GraphqlRepo::new(&s, &clock);
        let id = Id::new();
        assert!(repo.get(id).await.unwrap().is_none());
        repo.put(id, "sdl", "schema.graphql", "type Query { a: Int }")
            .await
            .unwrap();
        repo.put(id, "introspection", "http://x/graphql", "{}")
            .await
            .unwrap();
        let got = repo.get(id).await.unwrap().unwrap();
        assert_eq!((got.format.as_str(), got.fetched_at), ("introspection", 5));
        repo.delete(id).await.unwrap();
        assert!(repo.get(id).await.unwrap().is_none());
    }
}
