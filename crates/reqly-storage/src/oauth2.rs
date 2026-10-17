//! OAuth 2.0 tokens, encrypted at rest.

use reqly_core::{Clock, CoreResult, Id, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::Storage;
use crate::db::db_err;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub token_type: String,
    pub scope: String,
    /// Unix milliseconds; `None` = no expiry given.
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub config_hash: String,
    /// The token endpoint response.
    pub raw: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Secret {
    access_token: String,
    refresh_token: String,
    id_token: String,
    raw: Value,
}

pub struct OAuth2Repo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> OAuth2Repo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    pub async fn get(&self, node_id: Id) -> CoreResult<Option<StoredToken>> {
        let Some(row) = sqlx::query("SELECT * FROM oauth2_tokens WHERE node_id = ?")
            .bind(node_id.to_string())
            .fetch_optional(self.storage.pool())
            .await
            .map_err(db_err)?
        else {
            return Ok(None);
        };
        let plain = self.storage.secrets().decrypt(row.get("secret"))?;
        let secret: Secret = serde_json::from_str(&plain).map_err(db_err)?;
        Ok(Some(StoredToken {
            access_token: secret.access_token,
            refresh_token: secret.refresh_token,
            id_token: secret.id_token,
            raw: secret.raw,
            token_type: row.get("token_type"),
            scope: row.get("scope"),
            expires_at: row.get("expires_at"),
            created_at: row.get("created_at"),
            config_hash: row.get("config_hash"),
        }))
    }

    /// Stores `token` (its `created_at` is set to now).
    pub async fn put(&self, node_id: Id, mut token: StoredToken) -> CoreResult<StoredToken> {
        token.created_at = self.clock.now().0;
        let secret = serde_json::to_string(&Secret {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            id_token: token.id_token.clone(),
            raw: token.raw.clone(),
        })
        .map_err(db_err)?;
        sqlx::query(
            "INSERT INTO oauth2_tokens (node_id, config_hash, secret, token_type, scope, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(node_id) DO UPDATE SET config_hash = excluded.config_hash, secret = excluded.secret, \
             token_type = excluded.token_type, scope = excluded.scope, expires_at = excluded.expires_at, \
             created_at = excluded.created_at",
        )
        .bind(node_id.to_string())
        .bind(&token.config_hash)
        .bind(self.storage.secrets().encrypt(&secret))
        .bind(&token.token_type)
        .bind(&token.scope)
        .bind(token.expires_at)
        .bind(token.created_at)
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(token)
    }

    pub async fn delete(&self, node_id: Id) -> CoreResult<()> {
        sqlx::query("DELETE FROM oauth2_tokens WHERE node_id = ?")
            .bind(node_id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    /// True when the token is past its expiry (with `margin_ms` to spare).
    pub fn is_expired(&self, token: &StoredToken, margin_ms: i64) -> bool {
        token
            .expires_at
            .is_some_and(|at| at - margin_ms <= self.clock.now().0)
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqly_core::FixedClock;

    #[tokio::test]
    async fn round_trip_encrypted() {
        let s = Storage::in_memory().await.unwrap();
        let clock = FixedClock::new(1_000);
        let repo = OAuth2Repo::new(&s, &clock);
        let id = Id::new();
        assert!(repo.get(id).await.unwrap().is_none());
        repo.put(
            id,
            StoredToken {
                access_token: "at-secret".into(),
                refresh_token: "rt".into(),
                token_type: "Bearer".into(),
                expires_at: Some(61_000),
                config_hash: "h".into(),
                ..StoredToken::default()
            },
        )
        .await
        .unwrap();
        let raw: String = sqlx::query_scalar("SELECT secret FROM oauth2_tokens")
            .fetch_one(s.pool())
            .await
            .unwrap();
        assert!(!raw.contains("at-secret"));
        let t = repo.get(id).await.unwrap().unwrap();
        assert_eq!(t.access_token, "at-secret");
        assert_eq!(t.created_at, 1_000);
        assert!(!repo.is_expired(&t, 30_000));
        assert!(repo.is_expired(&t, 60_000));
        repo.delete(id).await.unwrap();
        assert!(repo.get(id).await.unwrap().is_none());
    }
}
