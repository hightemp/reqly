//! Users of the multi-user server, their API tokens and workspace
//! memberships (ADR-010). Passwords are argon2id hashes; API tokens are
//! random and only their SHA-256 is stored.

use std::collections::HashMap;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use reqly_core::{Clock, CoreError, CoreResult, Id, Timestamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use ts_rs::TS;

use crate::Storage;
use crate::db::db_err;

/// Failed logins in a row that lock an account.
pub const MAX_FAILED_LOGINS: i64 = 5;
/// How long a locked account stays locked.
pub const LOCK_MS: i64 = 15 * 60 * 1000;
pub const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct User {
    pub id: Id,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub disabled: bool,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Owner => "owner",
        }
    }

    fn parse(s: &str) -> CoreResult<Self> {
        match s {
            "viewer" => Ok(Role::Viewer),
            "editor" => Ok(Role::Editor),
            "owner" => Ok(Role::Owner),
            other => Err(CoreError::internal(format!("unknown role {other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Member {
    pub user_id: Id,
    pub username: String,
    pub display_name: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApiToken {
    pub id: Id,
    pub name: String,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
}

pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 32 random bytes, hex encoded.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS randomness");
    hex::encode(bytes)
}

async fn hash_password(password: String) -> CoreResult<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| CoreError::internal(e.to_string()))
    })
    .await
    .map_err(|e| CoreError::internal(e.to_string()))?
}

async fn password_matches(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

fn check_password(password: &str) -> CoreResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(
            CoreError::new("user.weak_password", "password is too short")
                .with_param("min", MIN_PASSWORD_LEN),
        );
    }
    Ok(())
}

fn check_username(username: &str) -> CoreResult<()> {
    let ok = (2..=64).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    if !ok {
        return Err(CoreError::new(
            "user.invalid_username",
            "use 2-64 letters, digits, '.', '_', '-' or '@'",
        ));
    }
    Ok(())
}

fn user_from_row(r: &SqliteRow) -> CoreResult<User> {
    Ok(User {
        id: r.get::<&str, _>("id").parse()?,
        username: r.get("username"),
        display_name: r.get("display_name"),
        is_admin: r.get::<i64, _>("is_admin") != 0,
        disabled: r.get::<i64, _>("disabled") != 0,
        created_at: Timestamp(r.get("created_at")),
    })
}

fn user_not_found() -> CoreError {
    CoreError::new("not_found", "user not found")
}

pub struct UserRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> UserRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    fn pool(&self) -> &sqlx::SqlitePool {
        self.storage.pool()
    }

    pub async fn count(&self) -> CoreResult<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }

    pub async fn create(
        &self,
        username: &str,
        display_name: &str,
        password: &str,
        is_admin: bool,
    ) -> CoreResult<User> {
        let username = username.trim();
        check_username(username)?;
        check_password(password)?;
        if self.find_by_username(username).await?.is_some() {
            return Err(CoreError::new("user.exists", "username is taken"));
        }
        let hash = hash_password(password.to_string()).await?;
        let id = Id::new();
        let now = self.clock.now().0;
        let display_name = match display_name.trim() {
            "" => username,
            name => name,
        };
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_hash, is_admin, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(username)
        .bind(display_name)
        .bind(hash)
        .bind(i64::from(is_admin))
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        self.get(id).await
    }

    pub async fn get(&self, id: Id) -> CoreResult<User> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?
            .ok_or_else(user_not_found)?;
        user_from_row(&row)
    }

    pub async fn find_by_username(&self, username: &str) -> CoreResult<Option<User>> {
        sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username.trim())
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?
            .as_ref()
            .map(user_from_row)
            .transpose()
    }

    pub async fn list(&self) -> CoreResult<Vec<User>> {
        sqlx::query("SELECT * FROM users ORDER BY username")
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?
            .iter()
            .map(user_from_row)
            .collect()
    }

    /// Checks a password login. Errors: `auth.invalid` (unknown user or
    /// wrong password, indistinguishable), `auth.locked`, `auth.disabled`.
    pub async fn verify_login(&self, username: &str, password: &str) -> CoreResult<User> {
        let invalid = || CoreError::new("auth.invalid", "invalid username or password");
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username.trim())
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        let Some(row) = row else {
            // Spend the same time as for a real user.
            let _ = hash_password(password.to_string()).await;
            return Err(invalid());
        };
        let user = user_from_row(&row)?;
        let now = self.clock.now().0;
        if let Some(until) = row.get::<Option<i64>, _>("locked_until")
            && until > now
        {
            return Err(
                CoreError::new("auth.locked", "too many failed logins, try again later")
                    .with_param("until", until),
            );
        }
        let hash: String = row.get("password_hash");
        if !password_matches(password.to_string(), hash).await {
            sqlx::query(
                "UPDATE users SET failed_logins = failed_logins + 1, \
                 locked_until = CASE WHEN failed_logins + 1 >= ? THEN ? ELSE locked_until END \
                 WHERE id = ?",
            )
            .bind(MAX_FAILED_LOGINS)
            .bind(now + LOCK_MS)
            .bind(user.id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
            return Err(invalid());
        }
        if user.disabled {
            return Err(CoreError::new("auth.disabled", "the account is disabled"));
        }
        sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = ?")
            .bind(user.id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(user)
    }

    pub async fn check_password(&self, id: Id, password: &str) -> CoreResult<bool> {
        let hash: Option<String> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await
                .map_err(db_err)?;
        let hash = hash.ok_or_else(user_not_found)?;
        Ok(password_matches(password.to_string(), hash).await)
    }

    /// Sets a new password and ends every session of the user.
    pub async fn set_password(&self, id: Id, password: &str) -> CoreResult<()> {
        check_password(password)?;
        let hash = hash_password(password.to_string()).await?;
        self.update(
            id,
            "password_hash = ?, failed_logins = 0, locked_until = NULL",
            hash,
        )
        .await?;
        self.end_sessions(id).await
    }

    pub async fn set_display_name(&self, id: Id, name: &str) -> CoreResult<User> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::new("user.invalid_name", "name is empty"));
        }
        self.update(id, "display_name = ?", name.to_string())
            .await?;
        self.get(id).await
    }

    pub async fn set_admin(&self, id: Id, admin: bool) -> CoreResult<User> {
        if !admin && self.get(id).await?.is_admin && self.admin_count().await? <= 1 {
            return Err(CoreError::new(
                "user.last_admin",
                "the last administrator stays",
            ));
        }
        self.update(id, "is_admin = ?", i64::from(admin)).await?;
        self.get(id).await
    }

    /// Disabling also ends the user's sessions.
    pub async fn set_disabled(&self, id: Id, disabled: bool) -> CoreResult<User> {
        if disabled && self.get(id).await?.is_admin && self.admin_count().await? <= 1 {
            return Err(CoreError::new(
                "user.last_admin",
                "the last administrator stays",
            ));
        }
        self.update(id, "disabled = ?", i64::from(disabled)).await?;
        if disabled {
            self.end_sessions(id).await?;
        }
        self.get(id).await
    }

    /// Deletes the user with tokens, sessions and memberships. Refuses when
    /// that would leave a workspace without an owner.
    pub async fn delete(&self, id: Id) -> CoreResult<()> {
        let user = self.get(id).await?;
        if user.is_admin && self.admin_count().await? <= 1 {
            return Err(CoreError::new(
                "user.last_admin",
                "the last administrator stays",
            ));
        }
        let sole_owner: Option<String> = sqlx::query_scalar(
            "SELECT m.workspace_id FROM memberships m WHERE m.user_id = ? AND m.role = 'owner' \
             AND (SELECT COUNT(*) FROM memberships o WHERE o.workspace_id = m.workspace_id \
                  AND o.role = 'owner') = 1 LIMIT 1",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        if let Some(ws) = sole_owner {
            return Err(CoreError::new(
                "member.last_owner",
                "the user is the only owner of a workspace",
            )
            .with_param("workspaceId", ws));
        }
        self.end_sessions(id).await?;
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn admin_count(&self) -> CoreResult<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND disabled = 0")
            .fetch_one(self.pool())
            .await
            .map_err(db_err)
    }

    async fn update<T>(&self, id: Id, set: &str, value: T) -> CoreResult<()>
    where
        T: for<'q> sqlx::Encode<'q, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send,
    {
        let done = // `set` is a fixed fragment from this module, never user input.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE users SET {set}, updated_at = ? WHERE id = ?"
        )))
            .bind(value)
            .bind(self.clock.now().0)
            .bind(id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(user_not_found());
        }
        Ok(())
    }

    async fn end_sessions(&self, id: Id) -> CoreResult<()> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // API tokens

    /// Creates a token; the plain value is returned once and never stored.
    pub async fn create_token(
        &self,
        user_id: Id,
        name: &str,
        expires_at: Option<Timestamp>,
    ) -> CoreResult<(ApiToken, String)> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::new("token.invalid_name", "name is empty"));
        }
        let plain = format!("rq_{}", random_token());
        let id = Id::new();
        let now = self.clock.now();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(name)
        .bind(token_hash(&plain))
        .bind(now.0)
        .bind(expires_at.map(|t| t.0))
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        let token = ApiToken {
            id,
            name: name.to_string(),
            created_at: now,
            last_used_at: None,
            expires_at,
        };
        Ok((token, plain))
    }

    pub async fn list_tokens(&self, user_id: Id) -> CoreResult<Vec<ApiToken>> {
        let rows = sqlx::query("SELECT * FROM api_tokens WHERE user_id = ? ORDER BY created_at")
            .bind(user_id.to_string())
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok(ApiToken {
                    id: r.get::<&str, _>("id").parse()?,
                    name: r.get("name"),
                    created_at: Timestamp(r.get("created_at")),
                    last_used_at: r.get::<Option<i64>, _>("last_used_at").map(Timestamp),
                    expires_at: r.get::<Option<i64>, _>("expires_at").map(Timestamp),
                })
            })
            .collect()
    }

    pub async fn delete_token(&self, user_id: Id, token_id: Id) -> CoreResult<()> {
        let done = sqlx::query("DELETE FROM api_tokens WHERE user_id = ? AND id = ?")
            .bind(user_id.to_string())
            .bind(token_id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(CoreError::new("not_found", "token not found"));
        }
        Ok(())
    }

    /// The active user owning a live token; records its use.
    pub async fn user_for_token(&self, plain: &str) -> CoreResult<Option<User>> {
        let now = self.clock.now().0;
        let row = sqlx::query(
            "SELECT u.*, t.id AS token_id FROM api_tokens t JOIN users u ON u.id = t.user_id \
             WHERE t.token_hash = ? AND (t.expires_at IS NULL OR t.expires_at > ?) AND u.disabled = 0",
        )
        .bind(token_hash(plain))
        .bind(now)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        let Some(row) = row else { return Ok(None) };
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(row.get::<&str, _>("token_id"))
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        user_from_row(&row).map(Some)
    }
}

pub struct MemberRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> MemberRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    fn pool(&self) -> &sqlx::SqlitePool {
        self.storage.pool()
    }

    /// Adds the user or changes their role.
    pub async fn set(&self, workspace_id: Id, user_id: Id, role: Role) -> CoreResult<()> {
        if role != Role::Owner && self.is_last_owner(workspace_id, user_id).await? {
            return Err(last_owner());
        }
        sqlx::query(
            "INSERT INTO memberships (workspace_id, user_id, role, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = excluded.role",
        )
        .bind(workspace_id.to_string())
        .bind(user_id.to_string())
        .bind(role.as_str())
        .bind(self.clock.now().0)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn remove(&self, workspace_id: Id, user_id: Id) -> CoreResult<()> {
        if self.is_last_owner(workspace_id, user_id).await? {
            return Err(last_owner());
        }
        sqlx::query("DELETE FROM memberships WHERE workspace_id = ? AND user_id = ?")
            .bind(workspace_id.to_string())
            .bind(user_id.to_string())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn role(&self, workspace_id: Id, user_id: Id) -> CoreResult<Option<Role>> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM memberships WHERE workspace_id = ? AND user_id = ?",
        )
        .bind(workspace_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?;
        role.as_deref().map(Role::parse).transpose()
    }

    pub async fn list(&self, workspace_id: Id) -> CoreResult<Vec<Member>> {
        let rows = sqlx::query(
            "SELECT m.role, u.id, u.username, u.display_name FROM memberships m \
             JOIN users u ON u.id = m.user_id WHERE m.workspace_id = ? ORDER BY u.username",
        )
        .bind(workspace_id.to_string())
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok(Member {
                    user_id: r.get::<&str, _>("id").parse()?,
                    username: r.get("username"),
                    display_name: r.get("display_name"),
                    role: Role::parse(r.get("role"))?,
                })
            })
            .collect()
    }

    /// Workspaces of the user with their role.
    pub async fn workspaces_of(&self, user_id: Id) -> CoreResult<HashMap<Id, Role>> {
        let rows = sqlx::query("SELECT workspace_id, role FROM memberships WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.get::<&str, _>("workspace_id").parse()?,
                    Role::parse(r.get("role"))?,
                ))
            })
            .collect()
    }

    async fn is_last_owner(&self, workspace_id: Id, user_id: Id) -> CoreResult<bool> {
        let (is_owner, owners): (i64, i64) = sqlx::query_as(
            "SELECT \
               COALESCE(SUM(user_id = ?), 0), COUNT(*) \
             FROM memberships WHERE workspace_id = ? AND role = 'owner'",
        )
        .bind(user_id.to_string())
        .bind(workspace_id.to_string())
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(is_owner > 0 && owners <= 1)
    }
}

fn last_owner() -> CoreError {
    CoreError::new("member.last_owner", "a workspace needs at least one owner")
}

/// Maps entity ids (workspaces, collections, nodes, environments, response
/// records, cookies) to the workspace they belong to. Unknown ids are left
/// out.
pub async fn workspaces_of_ids(storage: &Storage, ids: &[Id]) -> CoreResult<HashMap<Id, Id>> {
    const SQL: &str = "\
        SELECT u.ws FROM ( \
            SELECT id AS ws FROM workspaces WHERE id = ?1 \
            UNION ALL SELECT workspace_id FROM collections WHERE id = ?1 \
            UNION ALL SELECT c.workspace_id FROM nodes n JOIN collections c ON c.id = n.collection_id WHERE n.id = ?1 \
            UNION ALL SELECT workspace_id FROM environments WHERE id = ?1 \
            UNION ALL SELECT workspace_id FROM response_records WHERE id = ?1 \
            UNION ALL SELECT workspace_id FROM cookies WHERE id = ?1 \
        ) u JOIN workspaces w ON w.id = u.ws WHERE w.deleted_at IS NULL LIMIT 1";
    let mut out = HashMap::new();
    for id in ids {
        if out.contains_key(id) {
            continue;
        }
        let ws: Option<String> = sqlx::query_scalar(SQL)
            .bind(id.to_string())
            .fetch_optional(storage.pool())
            .await
            .map_err(db_err)?;
        if let Some(ws) = ws {
            out.insert(*id, ws.parse()?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use reqly_core::FixedClock;

    use super::*;
    use crate::sessions::SessionRepo;
    use crate::tree::TreeRepo;

    async fn setup() -> (Storage, FixedClock) {
        (Storage::in_memory().await.unwrap(), FixedClock::new(1_000))
    }

    #[tokio::test]
    async fn creates_users_and_checks_passwords() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        let ann = users
            .create("ann", "", "correct horse", true)
            .await
            .unwrap();
        assert_eq!(ann.display_name, "ann");
        assert!(ann.is_admin);
        assert_eq!(users.count().await.unwrap(), 1);

        let err = users
            .create("ANN", "Other", "password1", false)
            .await
            .unwrap_err();
        assert_eq!(err.code, "user.exists");
        let err = users.create("bob", "", "short", false).await.unwrap_err();
        assert_eq!(err.code, "user.weak_password");
        let err = users
            .create("b o b", "", "password1", false)
            .await
            .unwrap_err();
        assert_eq!(err.code, "user.invalid_username");

        assert_eq!(
            users.verify_login("Ann", "correct horse").await.unwrap(),
            ann
        );
        let err = users.verify_login("ann", "wrong").await.unwrap_err();
        assert_eq!(err.code, "auth.invalid");
        let err = users.verify_login("nobody", "wrong").await.unwrap_err();
        assert_eq!(err.code, "auth.invalid");

        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert!(hash.starts_with("$argon2id$"), "{hash}");
    }

    #[tokio::test]
    async fn locks_after_failed_logins() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        users.create("ann", "", "password1", false).await.unwrap();
        for _ in 0..MAX_FAILED_LOGINS {
            let _ = users.verify_login("ann", "nope").await;
        }
        let err = users.verify_login("ann", "password1").await.unwrap_err();
        assert_eq!(err.code, "auth.locked");
        clock.advance(LOCK_MS + 1);
        users.verify_login("ann", "password1").await.unwrap();
        // A success resets the counter.
        let _ = users.verify_login("ann", "nope").await;
        users.verify_login("ann", "password1").await.unwrap();
    }

    #[tokio::test]
    async fn password_change_and_disabling_end_sessions() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        let sessions = SessionRepo::new(&storage, &clock);
        let ann = users.create("ann", "", "password1", false).await.unwrap();
        users.create("root", "", "password1", true).await.unwrap();
        let id = ann.id.to_string();
        sessions.create("s1", Some(&id), 1_000).await.unwrap();
        users.set_password(ann.id, "password2").await.unwrap();
        assert!(!sessions.is_valid("s1").await.unwrap());
        assert!(users.check_password(ann.id, "password2").await.unwrap());
        assert!(!users.check_password(ann.id, "password1").await.unwrap());

        sessions.create("s2", Some(&id), 1_000).await.unwrap();
        users.set_disabled(ann.id, true).await.unwrap();
        assert!(!sessions.is_valid("s2").await.unwrap());
        let err = users.verify_login("ann", "password2").await.unwrap_err();
        assert_eq!(err.code, "auth.disabled");
    }

    #[tokio::test]
    async fn keeps_the_last_admin() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        let root = users.create("root", "", "password1", true).await.unwrap();
        for err in [
            users.set_admin(root.id, false).await.unwrap_err(),
            users.set_disabled(root.id, true).await.unwrap_err(),
            users.delete(root.id).await.unwrap_err(),
        ] {
            assert_eq!(err.code, "user.last_admin");
        }
        let second = users.create("second", "", "password1", true).await.unwrap();
        users.set_admin(root.id, false).await.unwrap();
        users.delete(root.id).await.unwrap();
        assert_eq!(users.list().await.unwrap(), [second]);
    }

    #[tokio::test]
    async fn api_tokens() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        let ann = users.create("ann", "", "password1", false).await.unwrap();
        let (token, plain) = users.create_token(ann.id, "cli", None).await.unwrap();
        let (_, short) = users
            .create_token(ann.id, "ci", Some(Timestamp(1_500)))
            .await
            .unwrap();
        assert!(plain.starts_with("rq_"));

        assert_eq!(
            users.user_for_token(&plain).await.unwrap(),
            Some(ann.clone())
        );
        assert_eq!(users.user_for_token("rq_nope").await.unwrap(), None);
        let listed = users.list_tokens(ann.id).await.unwrap();
        assert_eq!(listed[0].last_used_at, Some(Timestamp(1_000)));

        clock.advance(600);
        assert_eq!(users.user_for_token(&short).await.unwrap(), None);

        users.delete_token(ann.id, token.id).await.unwrap();
        assert_eq!(users.user_for_token(&plain).await.unwrap(), None);
        let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM api_tokens")
            .fetch_all(storage.pool())
            .await
            .unwrap();
        assert!(stored.iter().all(|h| h.len() == 64 && *h != short));
    }

    #[tokio::test]
    async fn memberships_keep_an_owner() {
        let (storage, clock) = setup().await;
        let users = UserRepo::new(&storage, &clock);
        let members = MemberRepo::new(&storage, &clock);
        let ann = users.create("ann", "", "password1", false).await.unwrap();
        let bob = users.create("bob", "", "password1", false).await.unwrap();
        let ws = TreeRepo::new(&storage, &clock)
            .create_workspace("W")
            .await
            .unwrap()
            .meta
            .id;

        members.set(ws, ann.id, Role::Owner).await.unwrap();
        members.set(ws, bob.id, Role::Viewer).await.unwrap();
        assert_eq!(members.role(ws, bob.id).await.unwrap(), Some(Role::Viewer));
        assert_eq!(members.list(ws).await.unwrap().len(), 2);
        assert_eq!(
            members.workspaces_of(bob.id).await.unwrap(),
            HashMap::from([(ws, Role::Viewer)])
        );

        assert_eq!(
            members
                .set(ws, ann.id, Role::Editor)
                .await
                .unwrap_err()
                .code,
            "member.last_owner"
        );
        assert_eq!(
            members.remove(ws, ann.id).await.unwrap_err().code,
            "member.last_owner"
        );
        assert_eq!(
            users.delete(ann.id).await.unwrap_err().code,
            "member.last_owner"
        );

        members.set(ws, bob.id, Role::Owner).await.unwrap();
        members.remove(ws, ann.id).await.unwrap();
        assert_eq!(members.role(ws, ann.id).await.unwrap(), None);
        users.delete(bob.id).await.unwrap_err();
    }

    #[tokio::test]
    async fn maps_ids_to_workspaces() {
        let (storage, clock) = setup().await;
        let tree = TreeRepo::new(&storage, &clock);
        let ws = tree.create_workspace("W").await.unwrap().meta.id;
        let col = tree.create_collection(ws, "C").await.unwrap().meta.id;
        let unknown = Id::new();
        let map = workspaces_of_ids(&storage, &[ws, col, unknown])
            .await
            .unwrap();
        assert_eq!(map, HashMap::from([(ws, ws), (col, ws)]));
        // Deleted workspaces are gone for access checks.
        tree.delete_workspace(ws).await.unwrap();
        assert!(
            workspaces_of_ids(&storage, &[ws, col])
                .await
                .unwrap()
                .is_empty()
        );
    }
}
