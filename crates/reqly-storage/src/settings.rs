//! JSON key/value settings repository.

use reqly_core::{Clock, CoreResult};
use serde_json::Value;

use crate::Storage;
use crate::db::db_err;

pub struct SettingsRepo<'a> {
    storage: &'a Storage,
}

impl<'a> SettingsRepo<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub async fn get(&self, key: &str) -> CoreResult<Option<Value>> {
        let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.storage.pool())
            .await
            .map_err(db_err)?;
        raw.map(|s| serde_json::from_str(&s).map_err(db_err))
            .transpose()
    }

    pub async fn set(&self, key: &str, value: &Value, clock: &dyn Clock) -> CoreResult<()> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value.to_string())
        .bind(clock.now().millis())
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> CoreResult<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
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
    use serde_json::json;

    #[tokio::test]
    async fn missing_key_is_none() {
        let storage = Storage::in_memory().await.unwrap();
        assert_eq!(
            SettingsRepo::new(&storage).get("theme").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn set_get_overwrite_delete() {
        let storage = Storage::in_memory().await.unwrap();
        let repo = SettingsRepo::new(&storage);
        let clock = FixedClock::new(1);

        repo.set("ui", &json!({ "theme": "dark" }), &clock)
            .await
            .unwrap();
        assert_eq!(
            repo.get("ui").await.unwrap(),
            Some(json!({ "theme": "dark" }))
        );

        repo.set("ui", &json!({ "theme": "nord" }), &clock)
            .await
            .unwrap();
        assert_eq!(
            repo.get("ui").await.unwrap(),
            Some(json!({ "theme": "nord" }))
        );

        repo.delete("ui").await.unwrap();
        assert_eq!(repo.get("ui").await.unwrap(), None);
    }
}
