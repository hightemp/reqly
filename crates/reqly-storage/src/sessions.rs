//! Server login sessions. Tokens are random; only their SHA-256 is stored.

use reqly_core::{Clock, CoreResult};
use sha2::{Digest, Sha256};

use crate::Storage;
use crate::db::db_err;

pub struct SessionRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

impl<'a> SessionRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    /// Stores a session for `token` valid for `ttl_ms`.
    pub async fn create(&self, token: &str, user_id: Option<&str>, ttl_ms: i64) -> CoreResult<()> {
        let now = self.clock.now().0;
        sqlx::query("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
            .bind(hash(token))
            .bind(user_id)
            .bind(now)
            .bind(now + ttl_ms)
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn is_valid(&self, token: &str) -> CoreResult<bool> {
        Ok(self.lookup(token).await?.is_some())
    }

    /// `Some(user_id)` for a live session; the user id is `None` for
    /// sessions of the single-user `token` mode.
    pub async fn lookup(&self, token: &str) -> CoreResult<Option<Option<String>>> {
        let found: Option<(i64, Option<String>)> =
            sqlx::query_as("SELECT expires_at, user_id FROM sessions WHERE token_hash = ?")
                .bind(hash(token))
                .fetch_optional(self.storage.pool())
                .await
                .map_err(db_err)?;
        Ok(found
            .filter(|(exp, _)| *exp > self.clock.now().0)
            .map(|(_, user)| user))
    }

    pub async fn delete(&self, token: &str) -> CoreResult<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash(token))
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn purge_expired(&self) -> CoreResult<u64> {
        Ok(sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(self.clock.now().0)
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?
            .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqly_core::FixedClock;

    #[tokio::test]
    async fn create_validate_expire_delete() {
        let storage = Storage::in_memory().await.unwrap();
        let clock = FixedClock::new(1_000);
        let repo = SessionRepo::new(&storage, &clock);
        repo.create("tok", None, 100).await.unwrap();
        assert!(repo.is_valid("tok").await.unwrap());
        assert!(!repo.is_valid("other").await.unwrap());
        clock.advance(100);
        assert!(!repo.is_valid("tok").await.unwrap());
        assert_eq!(repo.purge_expired().await.unwrap(), 1);
        repo.create("tok2", Some("u1"), 100).await.unwrap();
        assert_eq!(repo.lookup("tok2").await.unwrap(), Some(Some("u1".into())));
        repo.delete("tok2").await.unwrap();
        assert!(!repo.is_valid("tok2").await.unwrap());
    }

    #[tokio::test]
    async fn stores_only_hashes() {
        let storage = Storage::in_memory().await.unwrap();
        let clock = FixedClock::new(0);
        SessionRepo::new(&storage, &clock)
            .create("plain-token", None, 10)
            .await
            .unwrap();
        let stored: String = sqlx::query_scalar("SELECT token_hash FROM sessions")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_ne!(stored, "plain-token");
        assert_eq!(stored.len(), 64);
    }
}
