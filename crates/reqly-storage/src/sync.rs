//! Sync records over SQLite (ADR-013): the server side ([`SyncServer`])
//! and the client side ([`SyncClient`]) of `reqly-sync`.

use async_trait::async_trait;
use reqly_core::model::{NodeData, NodeKind, Protocol};
use reqly_core::{Clock, CoreError, CoreResult, Id, SortKey};
use reqly_sync::{Conflict, Cursor, Entry, Kind, LocalStore, Record, ServerStore};
use serde_json::{Map, Value, json};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::Storage;
use crate::crypto::SecretBox;
use crate::db::db_err;

fn table(kind: Kind) -> &'static str {
    match kind {
        Kind::Workspace => "workspaces",
        Kind::Collection => "collections",
        Kind::Environment => "environments",
        Kind::Node => "nodes",
    }
}

/// Rows of a kind with their workspace (`ws`) and change sequence; `t` is
/// the row's table.
fn select(kind: Kind) -> &'static str {
    match kind {
        Kind::Workspace => "SELECT t.*, t.id AS ws FROM workspaces t",
        Kind::Collection => "SELECT t.*, t.workspace_id AS ws FROM collections t",
        Kind::Environment => "SELECT t.*, t.workspace_id AS ws FROM environments t",
        Kind::Node => {
            "SELECT t.*, c.workspace_id AS ws FROM nodes t JOIN collections c ON c.id = t.collection_id"
        }
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::new("sync.invalid", message)
}

fn parse_json(raw: &str) -> CoreResult<Value> {
    serde_json::from_str(raw).map_err(db_err)
}

fn str_field<'a>(record: &'a Record, key: &str) -> CoreResult<&'a str> {
    record
        .fields
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{} {}: no {key}", record.kind.as_str(), record.id)))
}

fn id_field(record: &Record, key: &str) -> CoreResult<Option<Id>> {
    match record.fields.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => s.parse().map(Some),
        Some(_) => Err(invalid(format!("{key} is not an id"))),
    }
}

fn sort_key(record: &Record) -> CoreResult<String> {
    Ok(SortKey::parse(str_field(record, "sortKey")?)?
        .as_str()
        .to_string())
}

/// Reads and writes records; `secrets` decides whether the values of
/// secret environment variables are part of them.
struct Records<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
    secrets: bool,
}

impl Records<'_> {
    fn secret_box(&self) -> &SecretBox {
        self.storage.secrets()
    }

    fn read_row(&self, kind: Kind, row: &SqliteRow) -> CoreResult<Record> {
        let mut fields = Map::new();
        let sort = row.get::<String, _>("sort_key");
        fields.insert("name".into(), json!(row.get::<String, _>("name")));
        fields.insert("sortKey".into(), json!(sort));
        match kind {
            Kind::Workspace => {}
            Kind::Collection => {
                fields.insert("settings".into(), parse_json(row.get("settings"))?);
                fields.insert("scripts".into(), parse_json(row.get("scripts"))?);
            }
            Kind::Environment => {
                let secrets: Vec<String> =
                    serde_json::from_str(row.get("secrets")).map_err(db_err)?;
                let mut data: Map<String, Value> =
                    serde_json::from_str(row.get("data")).map_err(db_err)?;
                for key in &secrets {
                    if !self.secrets {
                        data.remove(key);
                    } else if let Some(Value::String(enc)) = data.get(key)
                        && SecretBox::is_encrypted(enc)
                    {
                        let plain = self.secret_box().decrypt(enc)?;
                        data.insert(key.clone(), parse_json(&plain)?);
                    }
                }
                fields.insert(
                    "collectionId".into(),
                    json!(row.get::<Option<String>, _>("collection_id")),
                );
                fields.insert("kind".into(), json!(row.get::<String, _>("kind")));
                fields.insert("color".into(), json!(row.get::<Option<String>, _>("color")));
                fields.insert("data".into(), Value::Object(data));
                fields.insert("secrets".into(), json!(secrets));
            }
            Kind::Node => {
                fields.insert(
                    "collectionId".into(),
                    json!(row.get::<String, _>("collection_id")),
                );
                fields.insert(
                    "parentId".into(),
                    json!(row.get::<Option<String>, _>("parent_id")),
                );
                fields.insert("data".into(), parse_json(row.get("data"))?);
            }
        }
        Ok(Record {
            kind,
            id: row.get::<String, _>("id").parse()?,
            workspace_id: row.get::<String, _>("ws").parse()?,
            deleted: row.get::<Option<i64>, _>("deleted_at").is_some(),
            fields,
        })
    }

    async fn get(
        &self,
        conn: &mut SqliteConnection,
        kind: Kind,
        id: Id,
    ) -> CoreResult<Option<(Record, i64)>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{} WHERE t.id = ?",
            select(kind)
        )))
        .bind(id.to_string())
        .fetch_optional(&mut *conn)
        .await
        .map_err(db_err)?;
        row.map(|r| Ok((self.read_row(kind, &r)?, r.get("sync_seq"))))
            .transpose()
    }

    async fn exists(conn: &mut SqliteConnection, table: &str, id: Id) -> CoreResult<bool> {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_one(&mut *conn)
        .await
        .map_err(db_err)?;
        Ok(n > 0)
    }

    /// Creates or replaces the row of a record.
    async fn put(&self, conn: &mut SqliteConnection, record: &Record) -> CoreResult<()> {
        // A record cannot move to another workspace: access is checked
        // against the workspace it names.
        if let Some((current, _)) = self.get(conn, record.kind, record.id).await?
            && current.workspace_id != record.workspace_id
        {
            return Err(invalid("the record belongs to another workspace"));
        }
        let now = self.clock.now().0;
        let deleted_at = record.deleted.then_some(now);
        let id = record.id.to_string();
        let name = str_field(record, "name")?;
        let sort = sort_key(record)?;
        // Keeps the time of an earlier deletion.
        let keep_deleted = |t: &str| {
            format!(
                "deleted_at = CASE WHEN excluded.deleted_at IS NULL THEN NULL \
                 ELSE COALESCE({t}.deleted_at, excluded.deleted_at) END, \
                 updated_at = excluded.updated_at, version = {t}.version + 1"
            )
        };
        match record.kind {
            Kind::Workspace => {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO workspaces (id, name, sort_key, created_at, updated_at, version, deleted_at) \
                     VALUES (?, ?, ?, ?, ?, 1, ?) ON CONFLICT (id) DO UPDATE SET \
                     name = excluded.name, sort_key = excluded.sort_key, {}",
                    keep_deleted("workspaces")
                )))
                .bind(&id)
                .bind(name)
                .bind(&sort)
                .bind(now)
                .bind(now)
                .bind(deleted_at)
                .execute(&mut *conn)
                .await
                .map_err(db_err)?;
            }
            Kind::Collection => {
                if !Self::exists(conn, "workspaces", record.workspace_id).await? {
                    return Err(CoreError::new("sync.missing_parent", "workspace not found"));
                }
                let settings = record.fields.get("settings").cloned().unwrap_or(json!({}));
                let scripts = record.fields.get("scripts").cloned().unwrap_or(json!({}));
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO collections (id, workspace_id, name, settings, scripts, sort_key, \
                     created_at, updated_at, version, deleted_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?) ON CONFLICT (id) DO UPDATE SET \
                     name = excluded.name, settings = excluded.settings, scripts = excluded.scripts, \
                     sort_key = excluded.sort_key, {}",
                    keep_deleted("collections")
                )))
                .bind(&id)
                .bind(record.workspace_id.to_string())
                .bind(name)
                .bind(settings.to_string())
                .bind(scripts.to_string())
                .bind(&sort)
                .bind(now)
                .bind(now)
                .bind(deleted_at)
                .execute(&mut *conn)
                .await
                .map_err(db_err)?;
            }
            Kind::Environment => {
                let collection = id_field(record, "collectionId")?;
                if let Some(c) = collection {
                    let ws: Option<String> =
                        sqlx::query_scalar("SELECT workspace_id FROM collections WHERE id = ?")
                            .bind(c.to_string())
                            .fetch_optional(&mut *conn)
                            .await
                            .map_err(db_err)?;
                    if ws != Some(record.workspace_id.to_string()) {
                        return Err(CoreError::new(
                            "sync.missing_parent",
                            "collection not found",
                        ));
                    }
                } else if !Self::exists(conn, "workspaces", record.workspace_id).await? {
                    return Err(CoreError::new("sync.missing_parent", "workspace not found"));
                }
                let env_kind = str_field(record, "kind")?;
                if !["global", "base", "sub"].contains(&env_kind) {
                    return Err(invalid("unknown environment kind"));
                }
                let secrets: Vec<String> = serde_json::from_value(
                    record.fields.get("secrets").cloned().unwrap_or(json!([])),
                )
                .map_err(|e| invalid(e.to_string()))?;
                let mut data: Map<String, Value> = match record.fields.get("data") {
                    Some(Value::Object(d)) => d.clone(),
                    _ => Map::new(),
                };
                if !self.secrets {
                    // Secret values never come in: keep the stored ones.
                    let stored: Option<String> =
                        sqlx::query_scalar("SELECT data FROM environments WHERE id = ?")
                            .bind(&id)
                            .fetch_optional(&mut *conn)
                            .await
                            .map_err(db_err)?;
                    let stored: Map<String, Value> = stored
                        .map(|s| serde_json::from_str(&s).map_err(db_err))
                        .transpose()?
                        .unwrap_or_default();
                    for key in &secrets {
                        data.remove(key);
                        if let Some(v) = stored.get(key) {
                            data.insert(key.clone(), v.clone());
                        }
                    }
                } else {
                    for key in &secrets {
                        if let Some(v) = data.get_mut(key) {
                            *v = Value::String(self.secret_box().encrypt(&v.to_string()));
                        }
                    }
                }
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO environments (id, workspace_id, collection_id, kind, name, color, data, \
                     secrets, sort_key, created_at, updated_at, version, deleted_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?) ON CONFLICT (id) DO UPDATE SET \
                     collection_id = excluded.collection_id, kind = excluded.kind, name = excluded.name, \
                     color = excluded.color, data = excluded.data, secrets = excluded.secrets, \
                     sort_key = excluded.sort_key, {}",
                    keep_deleted("environments")
                )))
                .bind(&id)
                .bind(record.workspace_id.to_string())
                .bind(collection.map(|c| c.to_string()))
                .bind(env_kind)
                .bind(name)
                .bind(record.fields.get("color").and_then(Value::as_str))
                .bind(Value::Object(data).to_string())
                .bind(json!(secrets).to_string())
                .bind(&sort)
                .bind(now)
                .bind(now)
                .bind(deleted_at)
                .execute(&mut *conn)
                .await
                .map_err(db_err)?;
            }
            Kind::Node => {
                let collection = id_field(record, "collectionId")?
                    .ok_or_else(|| invalid("a node needs a collection"))?;
                let in_workspace: Option<String> =
                    sqlx::query_scalar("SELECT workspace_id FROM collections WHERE id = ?")
                        .bind(collection.to_string())
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(db_err)?;
                if in_workspace != Some(record.workspace_id.to_string()) {
                    return Err(CoreError::new(
                        "sync.missing_parent",
                        "collection not found",
                    ));
                }
                // A parent that is gone (purged) puts the node at the root;
                // the difference syncs back as a move.
                let mut parent = id_field(record, "parentId")?;
                if let Some(p) = parent {
                    let n: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM nodes WHERE id = ? AND collection_id = ? AND kind = 'folder'",
                    )
                    .bind(p.to_string())
                    .bind(collection.to_string())
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(db_err)?;
                    if n == 0 || p == record.id {
                        parent = None;
                    }
                }
                let data: NodeData = serde_json::from_value(
                    record.fields.get("data").cloned().unwrap_or(Value::Null),
                )
                .map_err(|e| invalid(e.to_string()))?;
                let (method, url) = data.method_and_url();
                let kind = match data.kind() {
                    NodeKind::Folder => "folder",
                    NodeKind::Request => "request",
                };
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO nodes (id, collection_id, parent_id, kind, name, protocol, method, url, \
                     data, sort_key, created_at, updated_at, version, deleted_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?) ON CONFLICT (id) DO UPDATE SET \
                     collection_id = excluded.collection_id, parent_id = excluded.parent_id, \
                     name = excluded.name, protocol = excluded.protocol, method = excluded.method, \
                     url = excluded.url, data = excluded.data, sort_key = excluded.sort_key, {}",
                    keep_deleted("nodes")
                )))
                .bind(&id)
                .bind(collection.to_string())
                .bind(parent.map(|p| p.to_string()))
                .bind(kind)
                .bind(name)
                .bind(data.protocol().map(Protocol::as_str))
                .bind(method)
                .bind(url)
                .bind(serde_json::to_string(&data).map_err(db_err)?)
                .bind(&sort)
                .bind(now)
                .bind(now)
                .bind(deleted_at)
                .execute(&mut *conn)
                .await
                .map_err(db_err)?;
            }
        }
        Ok(())
    }
}

/// The server's records for `sync.pull` and `sync.push` (ADR-013).
pub struct SyncServer<'a> {
    records: Records<'a>,
}

impl<'a> SyncServer<'a> {
    /// `secrets`: the client syncs the values of secret environment
    /// variables; otherwise they are left out and the stored ones kept.
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock, secrets: bool) -> Self {
        Self {
            records: Records {
                storage,
                clock,
                secrets,
            },
        }
    }
}

#[async_trait]
impl ServerStore for SyncServer<'_> {
    async fn changed(&self, workspace_id: Id, since: u64) -> CoreResult<Vec<(u64, Record)>> {
        let mut conn = self
            .records
            .storage
            .pool()
            .acquire()
            .await
            .map_err(db_err)?;
        let mut out = Vec::new();
        for kind in Kind::ALL {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{} WHERE ws = ? AND t.sync_seq > ? ORDER BY t.sync_seq",
                select(kind)
            )))
            .bind(workspace_id.to_string())
            .bind(since as i64)
            .fetch_all(&mut *conn)
            .await
            .map_err(db_err)?;
            for row in rows {
                out.push((
                    row.get::<i64, _>("sync_seq") as u64,
                    self.records.read_row(kind, &row)?,
                ));
            }
        }
        out.sort_by_key(|(seq, _)| *seq);
        Ok(out)
    }

    async fn get(&self, kind: Kind, id: Id) -> CoreResult<Option<Record>> {
        let mut conn = self
            .records
            .storage
            .pool()
            .acquire()
            .await
            .map_err(db_err)?;
        Ok(self.records.get(&mut conn, kind, id).await?.map(|(r, _)| r))
    }

    async fn put(&self, record: &Record) -> CoreResult<()> {
        let mut tx = self.records.storage.begin_write().await?;
        self.records.put(&mut tx, record).await?;
        tx.commit().await.map_err(db_err)
    }
}

/// The client's synced records, bases, cursors and conflicts.
pub struct SyncClient<'a> {
    records: Records<'a>,
}

impl<'a> SyncClient<'a> {
    /// `secrets`: sync the values of secret environment variables.
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock, secrets: bool) -> Self {
        Self {
            records: Records {
                storage,
                clock,
                secrets,
            },
        }
    }

    fn storage(&self) -> &Storage {
        self.records.storage
    }

    /// Starts syncing a workspace (from the beginning).
    pub async fn follow(&self, workspace_id: Id) -> CoreResult<()> {
        sqlx::query(
            "INSERT INTO sync_cursors (workspace_id, seq) VALUES (?, 0) ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id.to_string())
        .execute(self.storage().pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// Stops syncing a workspace; its local data stays.
    pub async fn unfollow(&self, workspace_id: Id) -> CoreResult<()> {
        let mut tx = self.storage().begin_write().await?;
        for table in ["sync_cursors", "sync_base", "sync_conflicts"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM {table} WHERE workspace_id = ?"
            )))
            .bind(workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)
    }

    /// Forgets every synced workspace (disconnecting from the server).
    pub async fn unfollow_all(&self) -> CoreResult<()> {
        let mut tx = self.storage().begin_write().await?;
        for table in ["sync_cursors", "sync_base", "sync_conflicts"] {
            sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table}")))
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)
    }

    async fn base(
        &self,
        conn: &mut SqliteConnection,
        kind: Kind,
        id: Id,
    ) -> CoreResult<Option<Record>> {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT record FROM sync_base WHERE kind = ? AND id = ?")
                .bind(kind.as_str())
                .bind(id.to_string())
                .fetch_optional(&mut *conn)
                .await
                .map_err(db_err)?;
        raw.map(|r| serde_json::from_str(&r).map_err(db_err))
            .transpose()
    }
}

#[async_trait]
impl LocalStore for SyncClient<'_> {
    async fn cursors(&self) -> CoreResult<Vec<Cursor>> {
        let rows = sqlx::query("SELECT workspace_id, seq FROM sync_cursors ORDER BY workspace_id")
            .fetch_all(self.storage().pool())
            .await
            .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok(Cursor {
                    workspace_id: r.get::<String, _>("workspace_id").parse()?,
                    seq: r.get::<i64, _>("seq") as u64,
                })
            })
            .collect()
    }

    async fn set_cursor(&self, cursor: Cursor) -> CoreResult<()> {
        sqlx::query("UPDATE sync_cursors SET seq = ? WHERE workspace_id = ?")
            .bind(cursor.seq as i64)
            .bind(cursor.workspace_id.to_string())
            .execute(self.storage().pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn entry(&self, kind: Kind, id: Id) -> CoreResult<Entry> {
        let mut conn = self.storage().pool().acquire().await.map_err(db_err)?;
        let local = self.records.get(&mut conn, kind, id).await?.map(|(r, _)| r);
        let base = self.base(&mut conn, kind, id).await?;
        Ok(Entry { local, base })
    }

    async fn changed(&self) -> CoreResult<Vec<Entry>> {
        let mut conn = self.storage().pool().acquire().await.map_err(db_err)?;
        let mut out = Vec::new();
        for kind in Kind::ALL {
            // Rows newer than their base (or without one).
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{} LEFT JOIN sync_base b ON b.kind = ? AND b.id = t.id \
                 WHERE ws IN (SELECT workspace_id FROM sync_cursors) \
                 AND (b.id IS NULL OR t.sync_seq > b.local_seq)",
                select(kind).replacen("SELECT t.*", "SELECT t.*, b.record AS base_record", 1)
            )))
            .bind(kind.as_str())
            .fetch_all(&mut *conn)
            .await
            .map_err(db_err)?;
            for row in rows {
                let local = self.records.read_row(kind, &row)?;
                let base = row
                    .get::<Option<String>, _>("base_record")
                    .map(|r| serde_json::from_str(&r).map_err(db_err))
                    .transpose()?;
                out.push(Entry {
                    local: Some(local),
                    base,
                });
            }
            // Bases whose row is gone (purged).
            let gone: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT record FROM sync_base b WHERE b.kind = ? \
                 AND NOT EXISTS (SELECT 1 FROM {} t WHERE t.id = b.id)",
                table(kind)
            )))
            .bind(kind.as_str())
            .fetch_all(&mut *conn)
            .await
            .map_err(db_err)?;
            for raw in gone {
                out.push(Entry {
                    local: None,
                    base: Some(serde_json::from_str(&raw).map_err(db_err)?),
                });
            }
        }
        Ok(out.into_iter().filter(Entry::is_changed).collect())
    }

    async fn write(&self, local: Option<&Record>, base: &Record) -> CoreResult<()> {
        let mut tx = self.storage().begin_write().await?;
        let current = self.records.get(&mut tx, base.kind, base.id).await?;
        let mut seq = current.as_ref().map(|(_, s)| *s);
        let mut written = current.map(|(r, _)| r);
        if let Some(local) = local
            && written.as_ref().is_none_or(|w| !w.same_content(local))
        {
            self.records.put(&mut tx, local).await?;
            let now = self.records.get(&mut tx, base.kind, base.id).await?;
            seq = now.as_ref().map(|(_, s)| *s);
            written = now.map(|(r, _)| r);
        }
        // Clean when the row is the base; otherwise older, so it is checked.
        let local_seq = match (&written, seq) {
            (Some(w), Some(s)) if w.same_content(base) => s,
            _ => 0,
        };
        sqlx::query(
            "INSERT INTO sync_base (kind, id, workspace_id, record, local_seq) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (kind, id) DO UPDATE SET workspace_id = excluded.workspace_id, \
             record = excluded.record, local_seq = excluded.local_seq",
        )
        .bind(base.kind.as_str())
        .bind(base.id.to_string())
        .bind(base.workspace_id.to_string())
        .bind(serde_json::to_string(base).map_err(db_err)?)
        .bind(local_seq)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn conflicts(&self) -> CoreResult<Vec<Conflict>> {
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT conflict FROM sync_conflicts ORDER BY created_at, id")
                .fetch_all(self.storage().pool())
                .await
                .map_err(db_err)?;
        rows.iter()
            .map(|r| serde_json::from_str(r).map_err(db_err))
            .collect()
    }

    async fn set_conflict(&self, conflict: &Conflict) -> CoreResult<()> {
        sqlx::query(
            "INSERT INTO sync_conflicts (kind, id, workspace_id, conflict, created_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (kind, id) DO UPDATE SET conflict = excluded.conflict",
        )
        .bind(conflict.local.kind.as_str())
        .bind(conflict.local.id.to_string())
        .bind(conflict.local.workspace_id.to_string())
        .bind(serde_json::to_string(conflict).map_err(db_err)?)
        .bind(self.records.clock.now().0)
        .execute(self.storage().pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn remove_conflict(&self, kind: Kind, id: Id) -> CoreResult<()> {
        sqlx::query("DELETE FROM sync_conflicts WHERE kind = ? AND id = ?")
            .bind(kind.as_str())
            .bind(id.to_string())
            .execute(self.storage().pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

/// A server database answered in process (tests and tools).
pub struct InProcess<'a> {
    pub storage: &'a Storage,
    pub clock: &'a dyn Clock,
}

#[async_trait]
impl reqly_sync::Remote for InProcess<'_> {
    async fn pull(&self, input: reqly_sync::PullInput) -> CoreResult<reqly_sync::PullOutput> {
        let store = SyncServer::new(self.storage, self.clock, input.secrets);
        reqly_sync::server::pull(&store, &input, &|_| true).await
    }

    async fn push(&self, input: reqly_sync::PushInput) -> CoreResult<reqly_sync::PushOutput> {
        let store = SyncServer::new(self.storage, self.clock, input.secrets);
        reqly_sync::server::push(&store, input, &|_| true).await
    }
}
