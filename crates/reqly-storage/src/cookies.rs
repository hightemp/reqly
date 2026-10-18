//! Cookie jar per workspace. libcurl's cookie engine does the RFC 6265 work
//! during a request; this module persists its state between requests
//! (Netscape cookie-file lines) and backs the cookie manager UI.

use reqly_core::{Clock, CoreResult, Id};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use ts_rs::TS;

use crate::Storage;
use crate::db::db_err;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cookie {
    pub id: Id,
    /// Without a leading dot.
    pub domain: String,
    pub include_subdomains: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Unix seconds; `None` = session cookie.
    pub expires: Option<i64>,
    pub name: String,
    pub value: String,
}

impl Cookie {
    /// Parses one line of curl's Netscape cookie format.
    pub fn from_netscape(line: &str) -> Option<Self> {
        let (http_only, rest) = match line.strip_prefix("#HttpOnly_") {
            Some(rest) => (true, rest),
            None if line.starts_with('#') => return None,
            None => (false, line),
        };
        let f: Vec<&str> = rest.split('\t').collect();
        if f.len() != 7 {
            return None;
        }
        let expires: i64 = f[4].parse().ok()?;
        Some(Self {
            id: Id::new(),
            domain: f[0].trim_start_matches('.').to_string(),
            include_subdomains: f[1] == "TRUE",
            path: f[2].to_string(),
            secure: f[3] == "TRUE",
            http_only,
            expires: (expires > 0).then_some(expires),
            name: f[5].to_string(),
            value: f[6].to_string(),
        })
    }

    pub fn to_netscape(&self) -> String {
        let bool_str = |b: bool| if b { "TRUE" } else { "FALSE" };
        let domain = if self.include_subdomains {
            format!(".{}", self.domain)
        } else {
            self.domain.clone()
        };
        format!(
            "{}{}\t{}\t{}\t{}\t{}\t{}\t{}",
            if self.http_only { "#HttpOnly_" } else { "" },
            domain,
            bool_str(self.include_subdomains),
            self.path,
            bool_str(self.secure),
            self.expires.unwrap_or(0),
            self.name,
            self.value
        )
    }
}

pub struct CookieRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

impl<'a> CookieRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    /// Live (unexpired) cookies, sorted by domain, path, name.
    pub async fn list(&self, workspace_id: Id) -> CoreResult<Vec<Cookie>> {
        let now_secs = self.clock.now().0 / 1000;
        let rows = sqlx::query(
            "SELECT * FROM cookies WHERE workspace_id = ? AND (expires IS NULL OR expires > ?) \
             ORDER BY domain, path, name",
        )
        .bind(workspace_id.to_string())
        .bind(now_secs)
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter()
            .map(|r| {
                Ok(Cookie {
                    id: r.get::<&str, _>("id").parse()?,
                    domain: r.get("domain"),
                    include_subdomains: r.get::<i64, _>("include_subdomains") != 0,
                    path: r.get("path"),
                    secure: r.get::<i64, _>("secure") != 0,
                    http_only: r.get::<i64, _>("http_only") != 0,
                    expires: r.get("expires"),
                    name: r.get("name"),
                    value: r.get("value"),
                })
            })
            .collect()
    }

    pub async fn netscape_lines(&self, workspace_id: Id) -> CoreResult<Vec<String>> {
        Ok(self
            .list(workspace_id)
            .await?
            .iter()
            .map(Cookie::to_netscape)
            .collect())
    }

    pub async fn upsert(&self, workspace_id: Id, c: &Cookie) -> CoreResult<()> {
        sqlx::query(
            "INSERT INTO cookies (id, workspace_id, domain, include_subdomains, path, secure, \
             http_only, expires, name, value, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (workspace_id, domain, path, name) DO UPDATE SET \
             include_subdomains = excluded.include_subdomains, secure = excluded.secure, \
             http_only = excluded.http_only, expires = excluded.expires, value = excluded.value, \
             updated_at = excluded.updated_at",
        )
        .bind(c.id.to_string())
        .bind(workspace_id.to_string())
        .bind(&c.domain)
        .bind(i64::from(c.include_subdomains))
        .bind(&c.path)
        .bind(i64::from(c.secure))
        .bind(i64::from(c.http_only))
        .bind(c.expires)
        .bind(&c.name)
        .bind(&c.value)
        .bind(self.clock.now().0)
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    /// Replaces the whole jar with curl's state after a request.
    pub async fn replace_all(&self, workspace_id: Id, lines: &[String]) -> CoreResult<()> {
        let mut tx = self.storage.begin_write().await?;
        sqlx::query("DELETE FROM cookies WHERE workspace_id = ?")
            .bind(workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        for c in lines.iter().filter_map(|l| Cookie::from_netscape(l)) {
            self.upsert(workspace_id, &c).await?;
        }
        Ok(())
    }

    pub async fn delete(&self, workspace_id: Id, ids: &[Id]) -> CoreResult<()> {
        for id in ids {
            sqlx::query("DELETE FROM cookies WHERE workspace_id = ? AND id = ?")
                .bind(workspace_id.to_string())
                .bind(id.to_string())
                .execute(self.storage.pool())
                .await
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn clear(&self, workspace_id: Id) -> CoreResult<()> {
        sqlx::query("DELETE FROM cookies WHERE workspace_id = ?")
            .bind(workspace_id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
