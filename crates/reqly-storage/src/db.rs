use std::path::{Path, PathBuf};
use std::str::FromStr;

use reqly_core::{CoreError, CoreResult};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// Copy the database next to itself before applying pending migrations.
    pub backup_before_migrate: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            backup_before_migrate: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
    secrets: crate::crypto::SecretBox,
}

pub(crate) fn db_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("storage.error", e.to_string())
}

impl Storage {
    /// Opens (creating if needed) the database file and applies migrations.
    pub async fn open(path: &Path, options: OpenOptions) -> CoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(db_err)?;
        }
        let existed = path.metadata().map(|m| m.len() > 0).unwrap_or(false);

        let connect = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .connect_with(connect)
            .await
            .map_err(db_err)?;
        let secrets = crate::crypto::SecretBox::load_or_create(&path.with_file_name("secret.key"))?;
        let storage = Self { pool, secrets };

        if existed && options.backup_before_migrate && storage.has_pending_migrations().await? {
            storage.backup(&backup_path(path)?).await?;
        }
        storage.migrate().await?;
        Ok(storage)
    }

    /// Fresh in-memory database, mainly for tests.
    pub async fn in_memory() -> CoreResult<Self> {
        let connect = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(db_err)?
            .foreign_keys(true);
        // A single connection: every pooled connection would see its own empty database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(connect)
            .await
            .map_err(db_err)?;
        let storage = Self {
            pool,
            secrets: crate::crypto::SecretBox::random(),
        };
        storage.migrate().await?;
        Ok(storage)
    }

    pub fn secrets(&self) -> &crate::crypto::SecretBox {
        &self.secrets
    }

    async fn migrate(&self) -> CoreResult<()> {
        MIGRATOR.run(&self.pool).await.map_err(db_err)
    }

    async fn has_pending_migrations(&self) -> CoreResult<bool> {
        let has_table: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        let applied: Vec<i64> = if has_table == 1 {
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
        } else {
            Vec::new()
        };
        Ok(MIGRATOR.iter().any(|m| !applied.contains(&m.version)))
    }

    /// Writes a consistent copy of the database to `target`.
    pub async fn backup(&self, target: &Path) -> CoreResult<()> {
        let target = target.to_string_lossy();
        sqlx::query("VACUUM INTO ?")
            .bind(target.as_ref())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        tracing::info!(%target, "database backed up before migration");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

fn backup_path(path: &Path) -> CoreResult<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(db_err)?
        .as_millis();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bak-{stamp}"));
    Ok(path.with_file_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn table_exists(storage: &Storage, name: &str) -> bool {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(name)
        .fetch_one(storage.pool())
        .await
        .unwrap()
            == 1
    }

    fn backups(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".bak-"))
            .collect()
    }

    #[tokio::test]
    async fn in_memory_applies_migrations() {
        let storage = Storage::in_memory().await.unwrap();
        assert!(table_exists(&storage, "settings").await);
    }

    #[tokio::test]
    async fn open_creates_file_and_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/reqly.db");
        let storage = Storage::open(&path, OpenOptions::default()).await.unwrap();
        assert!(path.exists());
        assert!(table_exists(&storage, "settings").await);
    }

    #[tokio::test]
    async fn fresh_database_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reqly.db");
        Storage::open(&path, OpenOptions::default())
            .await
            .unwrap()
            .close()
            .await;
        Storage::open(&path, OpenOptions::default())
            .await
            .unwrap()
            .close()
            .await;
        assert!(backups(dir.path()).is_empty(), "nothing pending on reopen");
    }

    #[tokio::test]
    async fn existing_database_with_pending_migrations_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reqly.db");
        // Simulate a database created by an older build with no migrations applied.
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query("CREATE TABLE legacy (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let storage = Storage::open(&path, OpenOptions::default()).await.unwrap();
        assert!(table_exists(&storage, "settings").await);
        let backups = backups(dir.path());
        assert_eq!(backups.len(), 1);

        let backup = SqlitePool::connect(&format!("sqlite://{}", backups[0].display()))
            .await
            .unwrap();
        let legacy: i64 =
            sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name = 'legacy'")
                .fetch_one(&backup)
                .await
                .unwrap();
        assert_eq!(legacy, 1);
    }

    #[tokio::test]
    async fn backup_can_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reqly.db");
        std::fs::write(&path, b"").unwrap();
        Storage::open(
            &path,
            OpenOptions {
                backup_before_migrate: false,
            },
        )
        .await
        .unwrap();
        assert!(backups(dir.path()).is_empty());
    }
}
