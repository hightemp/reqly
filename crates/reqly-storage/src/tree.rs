//! Workspaces, collections and the folder/request tree.
//!
//! All multi-row operations run in a transaction. Deletes are soft (trash);
//! a delete stamps the node and its live descendants with the same
//! `deleted_at`, which lets restore bring back exactly that batch.

use std::collections::{HashMap, HashSet};

use reqly_core::model::Node;
use reqly_core::model::{Collection, NodeData, NodeKind, NodeSummary, RequestSettings, Workspace};
use reqly_core::{Clock, CoreError, CoreResult, Id, Meta, SortKey, Timestamp, Version};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::Storage;
use crate::db::db_err;

/// Where to put nodes among their new siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Start,
    End,
    Before(Id),
    After(Id),
}

pub struct TreeRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

/// Columns needed for `NodeSummary` (no `data` JSON).
macro_rules! summary_columns {
    () => {
        "id, collection_id, parent_id, kind, name, method, url, \
         sort_key, created_at, updated_at, version, deleted_at"
    };
}

macro_rules! node_columns {
    () => {
        "id, collection_id, parent_id, kind, name, method, url, data, \
         sort_key, created_at, updated_at, version, deleted_at"
    };
}

fn parse_id(s: &str) -> CoreResult<Id> {
    s.parse()
}

fn meta_from_row(row: &SqliteRow) -> CoreResult<Meta> {
    Ok(Meta {
        id: parse_id(row.get("id"))?,
        created_at: Timestamp(row.get("created_at")),
        updated_at: Timestamp(row.get("updated_at")),
        version: Version(row.get::<i64, _>("version") as u64),
        deleted_at: row.get::<Option<i64>, _>("deleted_at").map(Timestamp),
        sort_key: SortKey::parse(row.get::<String, _>("sort_key"))?,
    })
}

fn node_from_row(row: &SqliteRow) -> CoreResult<Node> {
    Ok(Node {
        meta: meta_from_row(row)?,
        collection_id: parse_id(row.get("collection_id"))?,
        parent_id: row
            .get::<Option<String>, _>("parent_id")
            .map(|s| parse_id(&s))
            .transpose()?,
        name: row.get("name"),
        data: serde_json::from_str(row.get("data")).map_err(db_err)?,
    })
}

pub(crate) fn summary_from_row(row: &SqliteRow) -> CoreResult<NodeSummary> {
    let meta = meta_from_row(row)?;
    Ok(NodeSummary {
        id: meta.id,
        collection_id: parse_id(row.get("collection_id"))?,
        parent_id: row
            .get::<Option<String>, _>("parent_id")
            .map(|s| parse_id(&s))
            .transpose()?,
        kind: match row.get::<&str, _>("kind") {
            "folder" => NodeKind::Folder,
            _ => NodeKind::Request,
        },
        name: row.get("name"),
        sort_key: meta.sort_key,
        method: row.get("method"),
        url: row.get("url"),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        version: meta.version,
    })
}

fn kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Folder => "folder",
        NodeKind::Request => "request",
    }
}

fn request_columns(data: &NodeData) -> (Option<&str>, Option<&str>) {
    match data {
        NodeData::Request(r) => (Some(r.method.as_str()), Some(r.url.as_str())),
        NodeData::Folder(_) => (None, None),
    }
}

async fn insert_node(conn: &mut SqliteConnection, node: &Node) -> CoreResult<()> {
    let (method, url) = request_columns(&node.data);
    sqlx::query(
        "INSERT INTO nodes (id, collection_id, parent_id, kind, name, method, url, data, \
         sort_key, created_at, updated_at, version, deleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(node.meta.id.to_string())
    .bind(node.collection_id.to_string())
    .bind(node.parent_id.map(|p| p.to_string()))
    .bind(kind_str(node.data.kind()))
    .bind(&node.name)
    .bind(method)
    .bind(url)
    .bind(serde_json::to_string(&node.data).map_err(db_err)?)
    .bind(node.meta.sort_key.as_str())
    .bind(node.meta.created_at.0)
    .bind(node.meta.updated_at.0)
    .bind(node.meta.version.0 as i64)
    .bind(node.meta.deleted_at.map(|t| t.0))
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(())
}

async fn fetch_node(conn: &mut SqliteConnection, id: Id) -> CoreResult<Node> {
    let row = sqlx::query(concat!(
        "SELECT ",
        node_columns!(),
        " FROM nodes WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(&mut *conn)
    .await
    .map_err(db_err)?
    .ok_or_else(|| CoreError::not_found("node", id))?;
    node_from_row(&row)
}

/// Live children of `parent` (or the root) in display order: `(id, sort_key)`.
async fn siblings(
    conn: &mut SqliteConnection,
    collection_id: Id,
    parent: Option<Id>,
) -> CoreResult<Vec<(Id, SortKey)>> {
    let rows = sqlx::query(
        "SELECT id, sort_key FROM nodes WHERE collection_id = ? AND parent_id IS ? \
         AND deleted_at IS NULL ORDER BY sort_key, id",
    )
    .bind(collection_id.to_string())
    .bind(parent.map(|p| p.to_string()))
    .fetch_all(&mut *conn)
    .await
    .map_err(db_err)?;
    rows.iter()
        .map(|r| {
            Ok((
                parse_id(r.get("id"))?,
                SortKey::parse(r.get::<String, _>("sort_key"))?,
            ))
        })
        .collect()
}

/// Ids of `root` and all its descendants (any deletion state).
async fn subtree(conn: &mut SqliteConnection, root: Id) -> CoreResult<Vec<Id>> {
    let rows = sqlx::query(
        "WITH RECURSIVE d(id) AS (SELECT id FROM nodes WHERE id = ? \
         UNION ALL SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id) SELECT id FROM d",
    )
    .bind(root.to_string())
    .fetch_all(&mut *conn)
    .await
    .map_err(db_err)?;
    rows.iter().map(|r| parse_id(r.get("id"))).collect()
}

/// Generates `n` keys for `placement` among `siblings` (which must not
/// contain the nodes being placed).
fn keys_for(
    siblings: &[(Id, SortKey)],
    placement: Placement,
    n: usize,
) -> CoreResult<Vec<SortKey>> {
    let index_of = |anchor: Id| {
        siblings
            .iter()
            .position(|(id, _)| *id == anchor)
            .ok_or_else(|| {
                CoreError::new("tree.invalid_anchor", "anchor is not a sibling")
                    .with_param("id", anchor.to_string())
            })
    };
    let (prev, next) = match placement {
        Placement::Start => (None, siblings.first().map(|s| &s.1)),
        Placement::End => (siblings.last().map(|s| &s.1), None),
        Placement::Before(anchor) => {
            let i = index_of(anchor)?;
            (
                i.checked_sub(1).map(|p| &siblings[p].1),
                Some(&siblings[i].1),
            )
        }
        Placement::After(anchor) => {
            let i = index_of(anchor)?;
            (Some(&siblings[i].1), siblings.get(i + 1).map(|s| &s.1))
        }
    };
    SortKey::n_between(prev, next, n)
}

impl<'a> TreeRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    async fn conn(&self) -> CoreResult<sqlx::pool::PoolConnection<sqlx::Sqlite>> {
        self.storage.pool().acquire().await.map_err(db_err)
    }

    async fn begin(&self) -> CoreResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
        self.storage.pool().begin().await.map_err(db_err)
    }

    // ----- workspaces ------------------------------------------------------

    pub async fn list_workspaces(&self) -> CoreResult<Vec<Workspace>> {
        let rows =
            sqlx::query("SELECT * FROM workspaces WHERE deleted_at IS NULL ORDER BY sort_key, id")
                .fetch_all(self.storage.pool())
                .await
                .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok(Workspace {
                    meta: meta_from_row(r)?,
                    name: r.get("name"),
                })
            })
            .collect()
    }

    pub async fn create_workspace(&self, name: &str) -> CoreResult<Workspace> {
        let last: Option<String> =
            sqlx::query_scalar("SELECT max(sort_key) FROM workspaces WHERE deleted_at IS NULL")
                .fetch_one(self.storage.pool())
                .await
                .map_err(db_err)?;
        let last = last.map(SortKey::parse).transpose()?;
        let ws = Workspace {
            meta: Meta::new(self.clock, SortKey::between(last.as_ref(), None)?),
            name: name.to_string(),
        };
        sqlx::query(
            "INSERT INTO workspaces (id, name, sort_key, created_at, updated_at, version) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(ws.meta.id.to_string())
        .bind(&ws.name)
        .bind(ws.meta.sort_key.as_str())
        .bind(ws.meta.created_at.0)
        .bind(ws.meta.updated_at.0)
        .bind(ws.meta.version.0 as i64)
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(ws)
    }

    pub async fn rename_workspace(&self, id: Id, name: &str) -> CoreResult<()> {
        self.touch("workspaces", id, Some(("name", name))).await
    }

    pub async fn delete_workspace(&self, id: Id) -> CoreResult<()> {
        self.soft_delete_row("workspaces", id).await
    }

    // ----- collections -----------------------------------------------------

    fn collection_from_row(r: &SqliteRow) -> CoreResult<Collection> {
        Ok(Collection {
            meta: meta_from_row(r)?,
            workspace_id: parse_id(r.get("workspace_id"))?,
            name: r.get("name"),
            settings: serde_json::from_str(r.get("settings")).map_err(db_err)?,
        })
    }

    pub async fn list_collections(&self, workspace_id: Id) -> CoreResult<Vec<Collection>> {
        let rows = sqlx::query(
            "SELECT * FROM collections WHERE workspace_id = ? AND deleted_at IS NULL \
             ORDER BY sort_key, id",
        )
        .bind(workspace_id.to_string())
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(Self::collection_from_row).collect()
    }

    pub async fn get_collection(&self, id: Id) -> CoreResult<Collection> {
        let row = sqlx::query("SELECT * FROM collections WHERE id = ? AND deleted_at IS NULL")
            .bind(id.to_string())
            .fetch_optional(self.storage.pool())
            .await
            .map_err(db_err)?
            .ok_or_else(|| CoreError::not_found("collection", id))?;
        Self::collection_from_row(&row)
    }

    pub async fn create_collection(&self, workspace_id: Id, name: &str) -> CoreResult<Collection> {
        let last: Option<String> = sqlx::query_scalar(
            "SELECT max(sort_key) FROM collections WHERE workspace_id = ? AND deleted_at IS NULL",
        )
        .bind(workspace_id.to_string())
        .fetch_one(self.storage.pool())
        .await
        .map_err(db_err)?;
        let last = last.map(SortKey::parse).transpose()?;
        let col = Collection {
            meta: Meta::new(self.clock, SortKey::between(last.as_ref(), None)?),
            workspace_id,
            name: name.to_string(),
            settings: RequestSettings::default(),
        };
        sqlx::query(
            "INSERT INTO collections (id, workspace_id, name, settings, sort_key, created_at, \
             updated_at, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(col.meta.id.to_string())
        .bind(workspace_id.to_string())
        .bind(&col.name)
        .bind(serde_json::to_string(&col.settings).map_err(db_err)?)
        .bind(col.meta.sort_key.as_str())
        .bind(col.meta.created_at.0)
        .bind(col.meta.updated_at.0)
        .bind(col.meta.version.0 as i64)
        .execute(self.storage.pool())
        .await
        .map_err(|e| {
            if e.to_string().contains("FOREIGN KEY") {
                CoreError::not_found("workspace", workspace_id)
            } else {
                db_err(e)
            }
        })?;
        Ok(col)
    }

    pub async fn rename_collection(&self, id: Id, name: &str) -> CoreResult<()> {
        self.touch("collections", id, Some(("name", name))).await
    }

    pub async fn update_collection_settings(
        &self,
        id: Id,
        settings: &RequestSettings,
    ) -> CoreResult<()> {
        let json = serde_json::to_string(settings).map_err(db_err)?;
        self.touch("collections", id, Some(("settings", &json)))
            .await
    }

    pub async fn delete_collection(&self, id: Id) -> CoreResult<()> {
        self.soft_delete_row("collections", id).await
    }

    /// Bumps version/updated_at and optionally sets one text column.
    async fn touch(&self, table: &str, id: Id, set: Option<(&str, &str)>) -> CoreResult<()> {
        let now = self.clock.now().0;
        let sql = match set {
            Some((col, _)) => format!(
                "UPDATE {table} SET {col} = ?, updated_at = ?, version = version + 1 \
                 WHERE id = ? AND deleted_at IS NULL"
            ),
            None => format!(
                "UPDATE {table} SET updated_at = ?, version = version + 1 \
                 WHERE id = ? AND deleted_at IS NULL"
            ),
        };
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some((_, value)) = set {
            q = q.bind(value);
        }
        let done = q
            .bind(now)
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(CoreError::not_found(table.trim_end_matches('s'), id));
        }
        Ok(())
    }

    async fn soft_delete_row(&self, table: &str, id: Id) -> CoreResult<()> {
        let now = self.clock.now().0;
        let sql = format!(
            "UPDATE {table} SET deleted_at = ?, updated_at = ?, version = version + 1 \
             WHERE id = ? AND deleted_at IS NULL"
        );
        let done = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now)
            .bind(now)
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(CoreError::not_found(table.trim_end_matches('s'), id));
        }
        Ok(())
    }

    // ----- nodes -----------------------------------------------------------

    pub async fn get_node(&self, id: Id) -> CoreResult<Node> {
        fetch_node(&mut *self.conn().await?, id).await
    }

    /// Blob hashes referenced by request bodies (binary files, multipart
    /// `blob:` fields), including trashed nodes so restore keeps working.
    pub async fn blob_refs(&self) -> CoreResult<HashSet<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT data FROM nodes WHERE kind = 'request' \
             AND (data LIKE '%\"type\":\"blob\"%' OR data LIKE '%blob:%')",
        )
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        let mut refs = HashSet::new();
        for raw in rows {
            if let Ok(NodeData::Request(r)) = serde_json::from_str::<NodeData>(&raw) {
                if let Some(reqly_core::model::FileRef::Blob { hash, .. }) = &r.body.file {
                    refs.insert(hash.clone());
                }
                for f in &r.body.fields {
                    if let Some(hash) = f.value.strip_prefix("blob:") {
                        refs.insert(hash.to_string());
                    }
                }
            }
        }
        Ok(refs)
    }

    /// Ancestor folders of a node, outermost first.
    pub async fn ancestors(&self, id: Id) -> CoreResult<Vec<Node>> {
        let mut conn = self.conn().await?;
        let mut chain = Vec::new();
        let mut parent = fetch_node(&mut conn, id).await?.parent_id;
        while let Some(p) = parent {
            let node = fetch_node(&mut conn, p).await?;
            parent = node.parent_id;
            chain.push(node);
        }
        chain.reverse();
        Ok(chain)
    }

    /// Live nodes of a collection, ordered by `(sort_key, id)` within each parent.
    pub async fn list_tree(&self, collection_id: Id) -> CoreResult<Vec<NodeSummary>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            summary_columns!(),
            " FROM nodes WHERE collection_id = ? AND deleted_at IS NULL ORDER BY sort_key, id"
        ))
        .bind(collection_id.to_string())
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(summary_from_row).collect()
    }

    /// Checks that `parent` can contain nodes of `collection_id`.
    async fn check_parent(
        conn: &mut SqliteConnection,
        collection_id: Id,
        parent: Option<Id>,
    ) -> CoreResult<()> {
        let Some(parent) = parent else { return Ok(()) };
        let node = fetch_node(conn, parent).await?;
        if node.meta.is_deleted() {
            return Err(CoreError::not_found("node", parent));
        }
        if node.data.kind() != NodeKind::Folder {
            return Err(
                CoreError::new("tree.parent_not_folder", "parent is not a folder")
                    .with_param("id", parent.to_string()),
            );
        }
        if node.collection_id != collection_id {
            return Err(CoreError::new(
                "tree.parent_other_collection",
                "parent belongs to another collection",
            )
            .with_param("id", parent.to_string()));
        }
        Ok(())
    }

    pub async fn create_node(
        &self,
        collection_id: Id,
        parent_id: Option<Id>,
        name: &str,
        data: NodeData,
        placement: Placement,
    ) -> CoreResult<Node> {
        let mut tx = self.begin().await?;
        Self::check_parent(&mut tx, collection_id, parent_id).await?;
        let siblings = siblings(&mut tx, collection_id, parent_id).await?;
        let key = keys_for(&siblings, placement, 1)?.remove(0);
        let node = Node {
            meta: Meta::new(self.clock, key),
            collection_id,
            parent_id,
            name: name.to_string(),
            data,
        };
        insert_node(&mut tx, &node).await.map_err(|e| {
            if e.message.contains("FOREIGN KEY") {
                CoreError::not_found("collection", collection_id)
            } else {
                e
            }
        })?;
        tx.commit().await.map_err(db_err)?;
        Ok(node)
    }

    /// Updates name and/or data. With `expected_version`, fails with
    /// `tree.version_conflict` if someone else changed the node meanwhile.
    pub async fn update_node(
        &self,
        id: Id,
        name: Option<&str>,
        data: Option<NodeData>,
        expected_version: Option<Version>,
    ) -> CoreResult<Node> {
        let mut tx = self.begin().await?;
        let mut node = fetch_node(&mut tx, id).await?;
        if let Some(expected) = expected_version
            && expected != node.meta.version
        {
            return Err(CoreError::new("tree.version_conflict", "node was changed")
                .with_param("id", id.to_string())
                .with_param("version", node.meta.version.0));
        }
        if let Some(name) = name {
            node.name = name.to_string();
        }
        if let Some(data) = data {
            if data.kind() != node.data.kind() {
                return Err(CoreError::new(
                    "tree.kind_change",
                    "cannot change node kind",
                ));
            }
            node.data = data;
        }
        node.meta.touch(self.clock);
        let (method, url) = request_columns(&node.data);
        sqlx::query(
            "UPDATE nodes SET name = ?, method = ?, url = ?, data = ?, updated_at = ?, version = ? \
             WHERE id = ?",
        )
        .bind(&node.name)
        .bind(method)
        .bind(url)
        .bind(serde_json::to_string(&node.data).map_err(db_err)?)
        .bind(node.meta.updated_at.0)
        .bind(node.meta.version.0 as i64)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(node)
    }

    /// Drops ids whose ancestor is also selected and orders the rest in
    /// tree (depth-first) order, so group operations keep relative order.
    async fn normalize_selection(conn: &mut SqliteConnection, ids: &[Id]) -> CoreResult<Vec<Node>> {
        let mut nodes = Vec::new();
        for &id in ids {
            nodes.push(fetch_node(conn, id).await?);
        }
        let selected: HashSet<Id> = ids.iter().copied().collect();
        let mut keep = Vec::new();
        for node in nodes {
            let mut ancestor = node.parent_id;
            let mut covered = false;
            while let Some(a) = ancestor {
                if selected.contains(&a) {
                    covered = true;
                    break;
                }
                ancestor = fetch_node(conn, a).await?.parent_id;
            }
            if !covered && !keep.iter().any(|n: &Node| n.meta.id == node.meta.id) {
                keep.push(node);
            }
        }
        let mut order = HashMap::new();
        for node in &keep {
            if !order.contains_key(&node.meta.id) {
                order.extend(Self::dfs_order(conn, node.collection_id).await?);
            }
        }
        keep.sort_by_key(|n| order.get(&n.meta.id).copied().unwrap_or(usize::MAX));
        Ok(keep)
    }

    /// Depth-first position of every node (including deleted ones) of a collection.
    async fn dfs_order(
        conn: &mut SqliteConnection,
        collection_id: Id,
    ) -> CoreResult<HashMap<Id, usize>> {
        let rows = sqlx::query(
            "SELECT id, parent_id FROM nodes WHERE collection_id = ? ORDER BY sort_key, id",
        )
        .bind(collection_id.to_string())
        .fetch_all(&mut *conn)
        .await
        .map_err(db_err)?;
        let mut children: HashMap<Option<Id>, Vec<Id>> = HashMap::new();
        for r in &rows {
            let id = parse_id(r.get("id"))?;
            let parent = r
                .get::<Option<String>, _>("parent_id")
                .map(|s| parse_id(&s))
                .transpose()?;
            children.entry(parent).or_default().push(id);
        }
        let mut order = HashMap::new();
        let mut stack: Vec<Id> = children.get(&None).cloned().unwrap_or_default();
        stack.reverse();
        while let Some(id) = stack.pop() {
            order.insert(id, order.len());
            if let Some(kids) = children.get(&Some(id)) {
                stack.extend(kids.iter().rev());
            }
        }
        Ok(order)
    }

    pub async fn move_nodes(
        &self,
        ids: &[Id],
        target_collection: Id,
        target_parent: Option<Id>,
        placement: Placement,
    ) -> CoreResult<Vec<Node>> {
        let mut tx = self.begin().await?;
        Self::check_parent(&mut tx, target_collection, target_parent).await?;
        let moving = Self::normalize_selection(&mut tx, ids).await?;

        if let Some(parent) = target_parent {
            for node in &moving {
                if subtree(&mut tx, node.meta.id).await?.contains(&parent) {
                    return Err(
                        CoreError::new("tree.cycle", "cannot move a folder into itself")
                            .with_param("id", node.meta.id.to_string()),
                    );
                }
            }
        }

        let moving_ids: HashSet<Id> = moving.iter().map(|n| n.meta.id).collect();
        let siblings: Vec<_> = siblings(&mut tx, target_collection, target_parent)
            .await?
            .into_iter()
            .filter(|(id, _)| !moving_ids.contains(id))
            .collect();
        let keys = keys_for(&siblings, placement, moving.len())?;
        let now = self.clock.now().0;

        let mut moved = Vec::new();
        for (mut node, key) in moving.into_iter().zip(keys) {
            sqlx::query(
                "UPDATE nodes SET parent_id = ?, collection_id = ?, sort_key = ?, updated_at = ?, \
                 version = version + 1 WHERE id = ?",
            )
            .bind(target_parent.map(|p| p.to_string()))
            .bind(target_collection.to_string())
            .bind(key.as_str())
            .bind(now)
            .bind(node.meta.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
            if node.collection_id != target_collection {
                for id in subtree(&mut tx, node.meta.id).await? {
                    sqlx::query(
                        "UPDATE nodes SET collection_id = ?, updated_at = ?, version = version + 1 \
                         WHERE id = ? AND collection_id != ?",
                    )
                    .bind(target_collection.to_string())
                    .bind(now)
                    .bind(id.to_string())
                    .bind(target_collection.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                }
            }
            node = fetch_node(&mut tx, node.meta.id).await?;
            moved.push(node);
        }
        tx.commit().await.map_err(db_err)?;
        Ok(moved)
    }

    /// Deep-copies `root` (live nodes only) under `parent` with `key`.
    async fn copy_subtree(
        &self,
        conn: &mut SqliteConnection,
        root: &Node,
        collection_id: Id,
        parent: Option<Id>,
        key: SortKey,
        name: String,
    ) -> CoreResult<Node> {
        let mut copy = root.clone();
        copy.meta = Meta::new(self.clock, key);
        copy.collection_id = collection_id;
        copy.parent_id = parent;
        copy.name = name;
        copy.data.regenerate_row_ids();
        insert_node(conn, &copy).await?;

        // Breadth-first over (original id, copy id).
        let mut queue = vec![(root.meta.id, copy.meta.id)];
        while let Some((orig, new_parent)) = queue.pop() {
            let rows = sqlx::query(concat!(
                "SELECT ",
                node_columns!(),
                " FROM nodes WHERE parent_id = ? AND deleted_at IS NULL"
            ))
            .bind(orig.to_string())
            .fetch_all(&mut *conn)
            .await
            .map_err(db_err)?;
            for row in rows {
                let child = node_from_row(&row)?;
                let mut c = child.clone();
                c.meta = Meta::new(self.clock, child.meta.sort_key.clone());
                c.collection_id = collection_id;
                c.parent_id = Some(new_parent);
                c.data.regenerate_row_ids();
                insert_node(conn, &c).await?;
                queue.push((child.meta.id, c.meta.id));
            }
        }
        Ok(copy)
    }

    /// Copies each node next to itself, named "<name> (copy)".
    pub async fn duplicate_nodes(&self, ids: &[Id]) -> CoreResult<Vec<Node>> {
        let mut tx = self.begin().await?;
        let originals = Self::normalize_selection(&mut tx, ids).await?;
        let mut copies = Vec::new();
        for orig in originals {
            let siblings = siblings(&mut tx, orig.collection_id, orig.parent_id).await?;
            let key = keys_for(&siblings, Placement::After(orig.meta.id), 1)?.remove(0);
            let name = format!("{} (copy)", orig.name);
            let copy = self
                .copy_subtree(
                    &mut tx,
                    &orig,
                    orig.collection_id,
                    orig.parent_id,
                    key,
                    name,
                )
                .await?;
            copies.push(copy);
        }
        tx.commit().await.map_err(db_err)?;
        Ok(copies)
    }

    /// Pastes deep copies at the target (clipboard "copy").
    pub async fn copy_nodes(
        &self,
        ids: &[Id],
        target_collection: Id,
        target_parent: Option<Id>,
        placement: Placement,
    ) -> CoreResult<Vec<Node>> {
        let mut tx = self.begin().await?;
        Self::check_parent(&mut tx, target_collection, target_parent).await?;
        let originals = Self::normalize_selection(&mut tx, ids).await?;
        let siblings = siblings(&mut tx, target_collection, target_parent).await?;
        let keys = keys_for(&siblings, placement, originals.len())?;
        let mut copies = Vec::new();
        for (orig, key) in originals.iter().zip(keys) {
            let copy = self
                .copy_subtree(
                    &mut tx,
                    orig,
                    target_collection,
                    target_parent,
                    key,
                    orig.name.clone(),
                )
                .await?;
            copies.push(copy);
        }
        tx.commit().await.map_err(db_err)?;
        Ok(copies)
    }

    /// Moves nodes and their live descendants to the trash.
    pub async fn delete_nodes(&self, ids: &[Id]) -> CoreResult<()> {
        let mut tx = self.begin().await?;
        let now = self.clock.now().0;
        for node in Self::normalize_selection(&mut tx, ids).await? {
            for id in subtree(&mut tx, node.meta.id).await? {
                sqlx::query(
                    "UPDATE nodes SET deleted_at = ?, updated_at = ?, version = version + 1 \
                     WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(now)
                .bind(now)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            }
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    /// Restores nodes deleted in the same batch. If the parent is gone, the
    /// node lands at the end of the collection root.
    pub async fn restore_nodes(&self, ids: &[Id]) -> CoreResult<()> {
        let mut tx = self.begin().await?;
        let now = self.clock.now().0;
        for &id in ids {
            let node = fetch_node(&mut tx, id).await?;
            let Some(batch) = node.meta.deleted_at else {
                continue;
            };

            let parent_alive = match node.parent_id {
                Some(p) => !fetch_node(&mut tx, p).await?.meta.is_deleted(),
                None => true,
            };
            let parent = if parent_alive { node.parent_id } else { None };
            let siblings = siblings(&mut tx, node.collection_id, parent).await?;
            let key = keys_for(&siblings, Placement::End, 1)?.remove(0);
            sqlx::query("UPDATE nodes SET parent_id = ?, sort_key = ? WHERE id = ?")
                .bind(parent.map(|p| p.to_string()))
                .bind(key.as_str())
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;

            for sub in subtree(&mut tx, id).await? {
                sqlx::query(
                    "UPDATE nodes SET deleted_at = NULL, updated_at = ?, version = version + 1 \
                     WHERE id = ? AND deleted_at = ?",
                )
                .bind(now)
                .bind(sub.to_string())
                .bind(batch.0)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            }
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    /// Top-level trash entries: deleted nodes whose parent was not deleted in the same batch.
    pub async fn list_trash(&self, collection_id: Id) -> CoreResult<Vec<Node>> {
        let rows = sqlx::query(
            "SELECT n.* FROM nodes n LEFT JOIN nodes p ON p.id = n.parent_id \
             WHERE n.collection_id = ? AND n.deleted_at IS NOT NULL \
             AND (p.id IS NULL OR p.deleted_at IS NULL OR p.deleted_at != n.deleted_at) \
             ORDER BY n.deleted_at DESC, n.id",
        )
        .bind(collection_id.to_string())
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(node_from_row).collect()
    }

    /// Permanently deletes nodes and everything below them.
    pub async fn purge_nodes(&self, ids: &[Id]) -> CoreResult<()> {
        let mut tx = self.begin().await?;
        for id in ids {
            sqlx::query("DELETE FROM nodes WHERE id = ?")
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    /// Empties trash entries deleted before `cutoff`; returns how many entries were purged.
    pub async fn purge_deleted_before(&self, cutoff: Timestamp) -> CoreResult<u64> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT n.id FROM nodes n LEFT JOIN nodes p ON p.id = n.parent_id \
             WHERE n.deleted_at IS NOT NULL AND n.deleted_at < ? \
             AND (p.id IS NULL OR p.deleted_at IS NULL OR p.deleted_at != n.deleted_at)",
        )
        .bind(cutoff.0)
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        let ids: Vec<Id> = ids.iter().map(|s| parse_id(s)).collect::<CoreResult<_>>()?;
        self.purge_nodes(&ids).await?;
        Ok(ids.len() as u64)
    }
}
