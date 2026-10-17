//! Environments: global (per workspace), base and sub (per collection).
//!
//! Values of secret keys are encrypted at rest with the storage's
//! [`SecretBox`](crate::crypto::SecretBox); the repository always returns
//! decrypted data.

use reqly_core::model::{Environment, EnvironmentKind};
use reqly_core::{Clock, CoreError, CoreResult, Id, Meta, SortKey, Timestamp, Version};
use serde_json::{Map, Value};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::Storage;
use crate::crypto::SecretBox;
use crate::db::db_err;

pub const BASE_NAME: &str = "Base Environment";

macro_rules! columns {
    () => {
        "id, workspace_id, collection_id, kind, name, color, data, secrets, \
         sort_key, created_at, updated_at, version, deleted_at"
    };
}

pub struct EnvironmentRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

fn kind_str(kind: EnvironmentKind) -> &'static str {
    match kind {
        EnvironmentKind::Global => "global",
        EnvironmentKind::Base => "base",
        EnvironmentKind::Sub => "sub",
    }
}

fn parse_kind(s: &str) -> CoreResult<EnvironmentKind> {
    Ok(match s {
        "global" => EnvironmentKind::Global,
        "base" => EnvironmentKind::Base,
        "sub" => EnvironmentKind::Sub,
        other => return Err(db_err(format!("unknown environment kind {other}"))),
    })
}

impl<'a> EnvironmentRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    fn secrets_box(&self) -> &SecretBox {
        self.storage.secrets()
    }

    fn encrypt(&self, data: &Map<String, Value>, secrets: &[String]) -> CoreResult<String> {
        let mut out = data.clone();
        for key in secrets {
            if let Some(v) = out.get_mut(key) {
                let plain = serde_json::to_string(v).map_err(db_err)?;
                *v = Value::String(self.secrets_box().encrypt(&plain));
            }
        }
        serde_json::to_string(&out).map_err(db_err)
    }

    fn read_row(&self, row: &SqliteRow) -> CoreResult<Environment> {
        let secrets: Vec<String> = serde_json::from_str(row.get("secrets")).map_err(db_err)?;
        let mut data: Map<String, Value> = serde_json::from_str(row.get("data")).map_err(db_err)?;
        for key in &secrets {
            if let Some(Value::String(enc)) = data.get(key)
                && SecretBox::is_encrypted(enc)
            {
                let plain = self.secrets_box().decrypt(enc)?;
                let value = serde_json::from_str(&plain).map_err(db_err)?;
                data.insert(key.clone(), value);
            }
        }
        Ok(Environment {
            meta: Meta {
                id: row.get::<String, _>("id").parse()?,
                created_at: Timestamp(row.get("created_at")),
                updated_at: Timestamp(row.get("updated_at")),
                version: Version(row.get::<i64, _>("version") as u64),
                deleted_at: row.get::<Option<i64>, _>("deleted_at").map(Timestamp),
                sort_key: SortKey::parse(row.get::<String, _>("sort_key"))?,
            },
            workspace_id: row.get::<String, _>("workspace_id").parse()?,
            collection_id: row
                .get::<Option<String>, _>("collection_id")
                .map(|s| s.parse())
                .transpose()?,
            kind: parse_kind(row.get("kind"))?,
            name: row.get("name"),
            color: row.get("color"),
            data,
            secrets,
        })
    }

    async fn insert(&self, env: &Environment) -> CoreResult<()> {
        sqlx::query(concat!(
            "INSERT INTO environments (",
            columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(env.meta.id.to_string())
        .bind(env.workspace_id.to_string())
        .bind(env.collection_id.map(|id| id.to_string()))
        .bind(kind_str(env.kind))
        .bind(&env.name)
        .bind(&env.color)
        .bind(self.encrypt(&env.data, &env.secrets)?)
        .bind(serde_json::to_string(&env.secrets).map_err(db_err)?)
        .bind(env.meta.sort_key.as_str())
        .bind(env.meta.created_at.0)
        .bind(env.meta.updated_at.0)
        .bind(env.meta.version.0 as i64)
        .bind(env.meta.deleted_at.map(|t| t.0))
        .execute(self.storage.pool())
        .await
        .map_err(|e| {
            if e.to_string().contains("FOREIGN KEY") {
                CoreError::not_found(
                    "collection",
                    env.collection_id.map(|i| i.to_string()).unwrap_or_default(),
                )
            } else {
                db_err(e)
            }
        })?;
        Ok(())
    }

    async fn last_sort_key(
        &self,
        workspace_id: Id,
        collection_id: Option<Id>,
    ) -> CoreResult<Option<SortKey>> {
        let last: Option<String> = match collection_id {
            Some(col) => sqlx::query_scalar("SELECT max(sort_key) FROM environments WHERE collection_id = ?")
                .bind(col.to_string())
                .fetch_one(self.storage.pool())
                .await,
            None => sqlx::query_scalar(
                "SELECT max(sort_key) FROM environments WHERE workspace_id = ? AND collection_id IS NULL",
            )
            .bind(workspace_id.to_string())
            .fetch_one(self.storage.pool())
            .await,
        }
        .map_err(db_err)?;
        last.map(SortKey::parse).transpose()
    }

    pub async fn create(
        &self,
        workspace_id: Id,
        collection_id: Option<Id>,
        kind: EnvironmentKind,
        name: &str,
    ) -> CoreResult<Environment> {
        match (kind, collection_id) {
            (EnvironmentKind::Global, Some(_))
            | (EnvironmentKind::Base | EnvironmentKind::Sub, None) => {
                return Err(CoreError::new(
                    "environment.invalid",
                    "kind does not match the owner",
                ));
            }
            _ => {}
        }
        let last = self.last_sort_key(workspace_id, collection_id).await?;
        let env = Environment {
            meta: Meta::new(self.clock, SortKey::between(last.as_ref(), None)?),
            workspace_id,
            collection_id,
            kind,
            name: name.to_string(),
            color: None,
            data: Map::new(),
            secrets: Vec::new(),
        };
        self.insert(&env).await?;
        Ok(env)
    }

    pub async fn get(&self, id: Id) -> CoreResult<Environment> {
        let row = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM environments WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id.to_string())
        .fetch_optional(self.storage.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(|| CoreError::not_found("environment", id))?;
        self.read_row(&row)
    }

    /// Environments of a collection, base first; creates the base one when missing.
    pub async fn list_for_collection(&self, collection_id: Id) -> CoreResult<Vec<Environment>> {
        let mut list = self.query_collection(collection_id).await?;
        if !list.iter().any(|e| e.kind == EnvironmentKind::Base) {
            let workspace_id: Option<String> =
                sqlx::query_scalar("SELECT workspace_id FROM collections WHERE id = ?")
                    .bind(collection_id.to_string())
                    .fetch_optional(self.storage.pool())
                    .await
                    .map_err(db_err)?;
            let workspace_id: Id = workspace_id
                .ok_or_else(|| CoreError::not_found("collection", collection_id))?
                .parse()?;
            let base = Environment {
                meta: Meta::new(
                    self.clock,
                    SortKey::between(None, list.first().map(|e| &e.meta.sort_key))?,
                ),
                workspace_id,
                collection_id: Some(collection_id),
                kind: EnvironmentKind::Base,
                name: BASE_NAME.to_string(),
                color: None,
                data: Map::new(),
                secrets: Vec::new(),
            };
            self.insert(&base).await?;
            list = self.query_collection(collection_id).await?;
        }
        Ok(list)
    }

    async fn query_collection(&self, collection_id: Id) -> CoreResult<Vec<Environment>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM environments WHERE collection_id = ? AND deleted_at IS NULL \
             ORDER BY kind = 'sub', sort_key"
        ))
        .bind(collection_id.to_string())
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(|r| self.read_row(r)).collect()
    }

    pub async fn list_global(&self, workspace_id: Id) -> CoreResult<Vec<Environment>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM environments WHERE workspace_id = ? AND collection_id IS NULL \
             AND deleted_at IS NULL ORDER BY sort_key"
        ))
        .bind(workspace_id.to_string())
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(|r| self.read_row(r)).collect()
    }

    /// Updates the given fields (`None` = unchanged; `color: Some(None)` clears it).
    pub async fn update(
        &self,
        id: Id,
        name: Option<&str>,
        color: Option<Option<&str>>,
        data: Option<Map<String, Value>>,
        secrets: Option<Vec<String>>,
    ) -> CoreResult<Environment> {
        let mut env = self.get(id).await?;
        if let Some(name) = name {
            env.name = name.to_string();
        }
        if let Some(color) = color {
            env.color = color.map(str::to_string);
        }
        if let Some(data) = data {
            env.data = data;
        }
        if let Some(mut secrets) = secrets {
            secrets.sort();
            secrets.dedup();
            env.secrets = secrets;
        }
        env.meta.touch(self.clock);
        sqlx::query(
            "UPDATE environments SET name = ?, color = ?, data = ?, secrets = ?, \
             updated_at = ?, version = ? WHERE id = ?",
        )
        .bind(&env.name)
        .bind(&env.color)
        .bind(self.encrypt(&env.data, &env.secrets)?)
        .bind(serde_json::to_string(&env.secrets).map_err(db_err)?)
        .bind(env.meta.updated_at.0)
        .bind(env.meta.version.0 as i64)
        .bind(id.to_string())
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(env)
    }

    pub async fn duplicate(&self, id: Id) -> CoreResult<Environment> {
        let src = self.get(id).await?;
        if src.kind == EnvironmentKind::Base {
            return Err(CoreError::new(
                "environment.base_duplicate",
                "the base environment cannot be duplicated",
            ));
        }
        let siblings = match src.collection_id {
            Some(col) => self.query_collection(col).await?,
            None => self.list_global(src.workspace_id).await?,
        };
        let next = siblings
            .iter()
            .map(|e| &e.meta.sort_key)
            .find(|k| *k > &src.meta.sort_key);
        let copy = Environment {
            meta: Meta::new(
                self.clock,
                SortKey::between(Some(&src.meta.sort_key), next)?,
            ),
            name: format!("{} (copy)", src.name),
            ..src
        };
        self.insert(&copy).await?;
        Ok(copy)
    }

    pub async fn delete(&self, id: Id) -> CoreResult<()> {
        let env = self.get(id).await?;
        if env.kind == EnvironmentKind::Base {
            return Err(CoreError::new(
                "environment.base_delete",
                "the base environment cannot be deleted",
            ));
        }
        sqlx::query("DELETE FROM environments WHERE id = ?")
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
