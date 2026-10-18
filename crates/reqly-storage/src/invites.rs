//! Invitation links of the multi-user server (ADR-010). A link carries a
//! random token, works once and expires; a workspace invitation also makes
//! the user a member with the given role.

use reqly_core::{Clock, CoreError, CoreResult, Id, Timestamp};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use ts_rs::TS;

use crate::Storage;
use crate::db::db_err;
use crate::users::{MemberRepo, Role, random_token, token_hash};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Invite {
    pub id: Id,
    pub workspace_id: Option<Id>,
    pub role: Option<Role>,
    pub created_by: Id,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub used_by: Option<Id>,
    pub used_at: Option<Timestamp>,
}

/// What an invitation offers, shown before it is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InvitePreview {
    pub workspace_name: Option<String>,
    pub role: Option<Role>,
    pub invited_by: String,
}

fn invalid() -> CoreError {
    CoreError::new(
        "invite.invalid",
        "the invitation is invalid, used or expired",
    )
}

fn invite_from_row(r: &SqliteRow) -> CoreResult<Invite> {
    let id = |col: &str| -> CoreResult<Option<Id>> {
        r.get::<Option<&str>, _>(col).map(str::parse).transpose()
    };
    Ok(Invite {
        id: r.get::<&str, _>("id").parse()?,
        workspace_id: id("workspace_id")?,
        role: r
            .get::<Option<String>, _>("role")
            .map(|s| serde_json::from_value(serde_json::Value::String(s)))
            .transpose()
            .map_err(db_err)?,
        created_by: r.get::<&str, _>("created_by").parse()?,
        created_at: Timestamp(r.get("created_at")),
        expires_at: Timestamp(r.get("expires_at")),
        used_by: id("used_by")?,
        used_at: r.get::<Option<i64>, _>("used_at").map(Timestamp),
    })
}

pub struct InviteRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> InviteRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    fn pool(&self) -> &sqlx::SqlitePool {
        self.storage.pool()
    }

    /// Creates an invitation valid for `ttl_ms`; the token is returned once.
    pub async fn create(
        &self,
        created_by: Id,
        workspace: Option<(Id, Role)>,
        ttl_ms: i64,
    ) -> CoreResult<(Invite, String)> {
        let token = format!("inv_{}", random_token());
        let id = Id::new();
        let now = self.clock.now().0;
        sqlx::query(
            "INSERT INTO invitations (id, token_hash, workspace_id, role, created_by, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(token_hash(&token))
        .bind(workspace.map(|(ws, _)| ws.to_string()))
        .bind(workspace.map(|(_, role)| role.as_str()))
        .bind(created_by.to_string())
        .bind(now)
        .bind(now + ttl_ms)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok((self.get(id).await?, token))
    }

    async fn get(&self, id: Id) -> CoreResult<Invite> {
        let row = sqlx::query("SELECT * FROM invitations WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?
            .ok_or_else(|| CoreError::new("not_found", "invitation not found"))?;
        invite_from_row(&row)
    }

    /// Open (unused, unexpired) invitations of a workspace, or server-wide
    /// ones for `None`.
    pub async fn list(&self, workspace_id: Option<Id>) -> CoreResult<Vec<Invite>> {
        let rows = sqlx::query(
            "SELECT * FROM invitations WHERE workspace_id IS ? AND used_at IS NULL AND expires_at > ? \
             ORDER BY created_at",
        )
        .bind(workspace_id.map(|id| id.to_string()))
        .bind(self.clock.now().0)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(invite_from_row).collect()
    }

    pub async fn revoke(&self, workspace_id: Option<Id>, id: Id) -> CoreResult<()> {
        let done = sqlx::query("DELETE FROM invitations WHERE id = ? AND workspace_id IS ?")
            .bind(id.to_string())
            .bind(workspace_id.map(|id| id.to_string()))
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(CoreError::new("not_found", "invitation not found"));
        }
        Ok(())
    }

    async fn open_row(&self, token: &str) -> CoreResult<SqliteRow> {
        sqlx::query(
            "SELECT i.*, u.display_name AS inviter, w.name AS workspace_name FROM invitations i \
             JOIN users u ON u.id = i.created_by \
             LEFT JOIN workspaces w ON w.id = i.workspace_id \
             WHERE i.token_hash = ? AND i.used_at IS NULL AND i.expires_at > ? \
               AND (i.workspace_id IS NULL OR w.deleted_at IS NULL)",
        )
        .bind(token_hash(token))
        .bind(self.clock.now().0)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(invalid)
    }

    pub async fn preview(&self, token: &str) -> CoreResult<InvitePreview> {
        let row = self.open_row(token).await?;
        let invite = invite_from_row(&row)?;
        Ok(InvitePreview {
            workspace_name: row.get("workspace_name"),
            role: invite.role,
            invited_by: row.get("inviter"),
        })
    }

    /// Uses the invitation for `user_id`: marks it used and adds the
    /// membership it grants. Fails if it was used meanwhile.
    pub async fn accept(&self, token: &str, user_id: Id) -> CoreResult<Invite> {
        let invite = invite_from_row(&self.open_row(token).await?)?;
        let now = self.clock.now().0;
        let done = sqlx::query(
            "UPDATE invitations SET used_by = ?, used_at = ? WHERE id = ? AND used_at IS NULL",
        )
        .bind(user_id.to_string())
        .bind(now)
        .bind(invite.id.to_string())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(invalid());
        }
        if let (Some(ws), Some(role)) = (invite.workspace_id, invite.role) {
            let members = MemberRepo::new(self.storage, self.clock);
            // Never lower an existing role.
            if members.role(ws, user_id).await?.is_none_or(|r| r < role) {
                members.set(ws, user_id, role).await?;
            }
        }
        self.get(invite.id).await
    }
}

#[cfg(test)]
mod tests {
    use reqly_core::FixedClock;

    use super::*;
    use crate::tree::TreeRepo;
    use crate::users::UserRepo;

    #[tokio::test]
    async fn invitations_work_once_and_expire() {
        let storage = Storage::in_memory().await.unwrap();
        let clock = FixedClock::new(1_000);
        let users = UserRepo::new(&storage, &clock);
        let invites = InviteRepo::new(&storage, &clock);
        let members = MemberRepo::new(&storage, &clock);
        let ann = users
            .create("ann", "Ann", "password1", false)
            .await
            .unwrap();
        let bob = users.create("bob", "", "password1", false).await.unwrap();
        let ws = TreeRepo::new(&storage, &clock)
            .create_workspace("Team")
            .await
            .unwrap()
            .meta
            .id;
        members.set(ws, ann.id, Role::Owner).await.unwrap();

        let (invite, token) = invites
            .create(ann.id, Some((ws, Role::Editor)), 100)
            .await
            .unwrap();
        assert!(token.starts_with("inv_"));
        assert_eq!(
            invites.list(Some(ws)).await.unwrap(),
            std::slice::from_ref(&invite)
        );
        assert!(invites.list(None).await.unwrap().is_empty());
        let preview = invites.preview(&token).await.unwrap();
        assert_eq!(preview.workspace_name.as_deref(), Some("Team"));
        assert_eq!(preview.invited_by, "Ann");

        let used = invites.accept(&token, bob.id).await.unwrap();
        assert_eq!(used.used_by, Some(bob.id));
        assert_eq!(members.role(ws, bob.id).await.unwrap(), Some(Role::Editor));
        assert_eq!(
            invites.accept(&token, bob.id).await.unwrap_err().code,
            "invite.invalid"
        );
        assert!(invites.list(Some(ws)).await.unwrap().is_empty());

        // An invitation never lowers a role.
        let (_, viewer) = invites
            .create(ann.id, Some((ws, Role::Viewer)), 100)
            .await
            .unwrap();
        invites.accept(&viewer, ann.id).await.unwrap();
        assert_eq!(members.role(ws, ann.id).await.unwrap(), Some(Role::Owner));

        let (_, late) = invites.create(ann.id, None, 100).await.unwrap();
        clock.advance(101);
        assert_eq!(
            invites.preview(&late).await.unwrap_err().code,
            "invite.invalid"
        );
        assert_eq!(
            invites.preview("inv_nope").await.unwrap_err().code,
            "invite.invalid"
        );

        let (open, _) = invites.create(ann.id, None, 100).await.unwrap();
        assert_eq!(
            invites.revoke(Some(ws), open.id).await.unwrap_err().code,
            "not_found"
        );
        invites.revoke(None, open.id).await.unwrap();
    }
}
