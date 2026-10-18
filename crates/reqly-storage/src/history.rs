//! Response history: one row per executed request, bodies live in the blob store.

use std::collections::HashSet;

use reqly_core::model::{ResponseRecord, ResponseSummary};
use reqly_core::{Clock, CoreError, CoreResult, Id, Timestamp};
use serde_json::Value;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::Storage;
use crate::db::db_err;

pub struct NewResponse {
    pub request_id: Id,
    pub workspace_id: Id,
    pub status: Option<u16>,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    pub body_hash: Option<String>,
    pub body_size: u64,
    pub headers_size: u64,
    pub content_type: Option<String>,
    pub timings: Value,
    pub timeline: Value,
    pub request_snapshot: Value,
    pub redirects: u32,
    pub final_url: String,
    pub remote_addr: Option<String>,
    pub error: Option<CoreError>,
    pub script_logs: Value,
    pub tests: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLimits {
    /// Newest records kept per request.
    pub per_request: u32,
    /// Total body bytes kept across all history (newest first).
    pub total_bytes: u64,
    /// Records older than this are removed.
    pub max_age_ms: Option<i64>,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            per_request: 20,
            total_bytes: 1024 * 1024 * 1024,
            max_age_ms: None,
        }
    }
}

pub struct HistoryRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
    /// Recorded as the sender of new records.
    author: Option<Id>,
    /// Private history: only this user's records (and ones without a user).
    viewer: Option<Id>,
}

/// SQL condition for the viewer (two binds: the viewer id twice).
const VISIBLE: &str = "(? IS NULL OR user_id IS NULL OR user_id = ?)";

fn json<T: serde::de::DeserializeOwned>(row: &SqliteRow, col: &str) -> CoreResult<T> {
    serde_json::from_str(row.get(col)).map_err(db_err)
}

fn record_from_row(row: &SqliteRow) -> CoreResult<ResponseRecord> {
    Ok(ResponseRecord {
        id: row.get::<&str, _>("id").parse()?,
        request_id: row.get::<&str, _>("request_id").parse()?,
        workspace_id: row.get::<&str, _>("workspace_id").parse()?,
        created_at: Timestamp(row.get("created_at")),
        status: row.get::<Option<i64>, _>("status").map(|s| s as u16),
        status_text: row.get("status_text"),
        http_version: row.get("http_version"),
        headers: json(row, "headers")?,
        body_hash: row.get("body_hash"),
        body_size: row.get::<i64, _>("body_size") as u64,
        headers_size: row.get::<i64, _>("headers_size") as u64,
        content_type: row.get("content_type"),
        timings: json(row, "timings")?,
        timeline: json(row, "timeline")?,
        request_snapshot: json(row, "request_snapshot")?,
        redirects: row.get::<i64, _>("redirects") as u32,
        final_url: row.get("final_url"),
        remote_addr: row.get("remote_addr"),
        error: row
            .get::<Option<String>, _>("error")
            .map(|e| serde_json::from_str(&e))
            .transpose()
            .map_err(db_err)?,
        script_logs: json(row, "script_logs")?,
        tests: json(row, "tests")?,
    })
}

fn summary_from_row(row: &SqliteRow) -> CoreResult<ResponseSummary> {
    let timings: Value = json(row, "timings")?;
    let error: Option<CoreError> = row
        .get::<Option<String>, _>("error")
        .map(|e| serde_json::from_str(&e))
        .transpose()
        .map_err(db_err)?;
    Ok(ResponseSummary {
        id: row.get::<&str, _>("id").parse()?,
        request_id: row.get::<&str, _>("request_id").parse()?,
        created_at: Timestamp(row.get("created_at")),
        status: row.get::<Option<i64>, _>("status").map(|s| s as u16),
        status_text: row.get("status_text"),
        total_ms: timings.get("totalMs").and_then(Value::as_f64),
        body_size: row.get::<i64, _>("body_size") as u64,
        method: row.get("method"),
        url: row.get("url"),
        error_code: error.map(|e| e.code),
    })
}

const SUMMARY_COLUMNS: &str =
    "id, request_id, created_at, status, status_text, timings, body_size, method, url, error";

impl<'a> HistoryRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self {
            storage,
            clock,
            author: None,
            viewer: None,
        }
    }

    /// Records new responses as sent by `user`; with `private`, reads only
    /// show that user's records.
    pub fn for_user(mut self, user: Option<Id>, private: bool) -> Self {
        self.author = user;
        self.viewer = user.filter(|_| private);
        self
    }

    fn viewer(&self) -> Option<String> {
        self.viewer.map(|id| id.to_string())
    }

    pub async fn insert(&self, r: NewResponse) -> CoreResult<ResponseRecord> {
        let id = Id::new();
        let created_at = self.clock.now();
        let method = r
            .request_snapshot
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let url = r
            .request_snapshot
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        sqlx::query(
            "INSERT INTO response_records (id, request_id, workspace_id, created_at, status, \
             status_text, http_version, headers, body_hash, body_size, headers_size, content_type, \
             timings, timeline, request_snapshot, redirects, final_url, remote_addr, error, method, url, \
             script_logs, tests, user_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(r.request_id.to_string())
        .bind(r.workspace_id.to_string())
        .bind(created_at.0)
        .bind(r.status.map(i64::from))
        .bind(&r.status_text)
        .bind(&r.http_version)
        .bind(serde_json::to_string(&r.headers).map_err(db_err)?)
        .bind(&r.body_hash)
        .bind(r.body_size as i64)
        .bind(r.headers_size as i64)
        .bind(&r.content_type)
        .bind(r.timings.to_string())
        .bind(r.timeline.to_string())
        .bind(r.request_snapshot.to_string())
        .bind(i64::from(r.redirects))
        .bind(&r.final_url)
        .bind(&r.remote_addr)
        .bind(r.error.as_ref().map(serde_json::to_string).transpose().map_err(db_err)?)
        .bind(method)
        .bind(url)
        .bind(if r.script_logs.is_null() { "[]".to_string() } else { r.script_logs.to_string() })
        .bind(if r.tests.is_null() { "[]".to_string() } else { r.tests.to_string() })
        .bind(self.author.map(|id| id.to_string()))
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        self.get(id).await
    }

    pub async fn get(&self, id: Id) -> CoreResult<ResponseRecord> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT * FROM response_records WHERE id = ? AND {VISIBLE}"
        )))
        .bind(id.to_string())
        .bind(self.viewer())
        .bind(self.viewer())
        .fetch_optional(self.storage.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(|| CoreError::not_found("response", id))?;
        record_from_row(&row)
    }

    /// Newest first.
    pub async fn list_for_request(
        &self,
        request_id: Id,
        limit: u32,
    ) -> CoreResult<Vec<ResponseSummary>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMMARY_COLUMNS} FROM response_records WHERE request_id = ? AND {VISIBLE} \
             ORDER BY created_at DESC, id DESC LIMIT ?"
        )))
        .bind(request_id.to_string())
        .bind(self.viewer())
        .bind(self.viewer())
        .bind(i64::from(limit))
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(summary_from_row).collect()
    }

    /// Workspace-wide log, optionally filtered by a substring of method or URL.
    pub async fn list_for_workspace(
        &self,
        workspace_id: Id,
        filter: Option<&str>,
        limit: u32,
    ) -> CoreResult<Vec<ResponseSummary>> {
        let pattern = filter.map(|f| format!("%{}%", f.replace('%', "\\%").replace('_', "\\_")));
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMMARY_COLUMNS} FROM response_records WHERE workspace_id = ? AND {VISIBLE} \
             AND (? IS NULL OR url LIKE ? ESCAPE '\\' OR method LIKE ? ESCAPE '\\') \
             ORDER BY created_at DESC, id DESC LIMIT ?"
        )))
        .bind(workspace_id.to_string())
        .bind(self.viewer())
        .bind(self.viewer())
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(i64::from(limit))
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(summary_from_row).collect()
    }

    pub async fn delete(&self, ids: &[Id]) -> CoreResult<()> {
        for id in ids {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM response_records WHERE id = ? AND {VISIBLE}"
            )))
            .bind(id.to_string())
            .bind(self.viewer())
            .bind(self.viewer())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn clear_for_requests(&self, request_ids: &[Id]) -> CoreResult<()> {
        for id in request_ids {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM response_records WHERE request_id = ? AND {VISIBLE}"
            )))
            .bind(id.to_string())
            .bind(self.viewer())
            .bind(self.viewer())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        }
        Ok(())
    }

    pub async fn total_body_bytes(&self) -> CoreResult<u64> {
        let total: Option<i64> = sqlx::query_scalar("SELECT sum(body_size) FROM response_records")
            .fetch_one(self.storage.pool())
            .await
            .map_err(db_err)?;
        Ok(total.unwrap_or(0) as u64)
    }

    /// Applies retention limits; returns the number of removed records.
    pub async fn enforce_limits(&self, limits: &HistoryLimits) -> CoreResult<u64> {
        let pool = self.storage.pool();
        let mut removed = 0;

        removed += sqlx::query(
            "DELETE FROM response_records WHERE id IN (SELECT id FROM (SELECT id, row_number() \
             OVER (PARTITION BY request_id ORDER BY created_at DESC, id DESC) AS rn \
             FROM response_records) WHERE rn > ?)",
        )
        .bind(i64::from(limits.per_request))
        .execute(pool)
        .await
        .map_err(db_err)?
        .rows_affected();

        if let Some(max_age) = limits.max_age_ms {
            removed += sqlx::query("DELETE FROM response_records WHERE created_at < ?")
                .bind(self.clock.now().0 - max_age)
                .execute(pool)
                .await
                .map_err(db_err)?
                .rows_affected();
        }

        let budget = i64::try_from(limits.total_bytes).unwrap_or(i64::MAX);
        removed += sqlx::query(
            "DELETE FROM response_records WHERE id IN (SELECT id FROM (SELECT id, sum(body_size) \
             OVER (ORDER BY created_at DESC, id DESC) AS running FROM response_records) \
             WHERE running > ?)",
        )
        .bind(budget)
        .execute(pool)
        .await
        .map_err(db_err)?
        .rows_affected();

        Ok(removed)
    }

    pub async fn live_body_hashes(&self) -> CoreResult<HashSet<String>> {
        let hashes: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT body_hash FROM response_records WHERE body_hash IS NOT NULL",
        )
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(hashes.into_iter().collect())
    }
}
