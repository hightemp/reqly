//! AI chat conversations and their messages (ADR-015).

use reqly_core::{Clock, CoreError, CoreResult, Id, Timestamp};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use ts_rs::TS;

use crate::Storage;
use crate::db::db_err;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiChat {
    pub id: Id,
    pub workspace_id: Id,
    #[ts(optional)]
    pub owner: Option<Id>,
    pub title: String,
    #[ts(optional)]
    pub provider_id: Option<String>,
    #[ts(optional)]
    pub model: Option<String>,
    /// Tool calls in this chat run without asking.
    pub auto_approve: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AiRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AiMessageStatus {
    Done,
    /// Being written, or a tool call waiting for the user.
    Pending,
    Error,
    Cancelled,
    /// A tool call the user declined.
    Denied,
}

impl AiMessageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Pending => "pending",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
            Self::Denied => "denied",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "error" => Self::Error,
            "cancelled" => Self::Cancelled,
            "denied" => Self::Denied,
            _ => Self::Done,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    /// JSON text as the model wrote it.
    pub arguments: String,
}

/// What a user attached to a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiAttachment {
    /// `request` or `response`.
    pub kind: String,
    pub id: Id,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiChatMessage {
    pub id: Id,
    pub chat_id: Id,
    pub seq: i64,
    pub role: AiRole,
    pub content: String,
    /// Sent to the model with a user message (attachments); not shown.
    #[serde(skip)]
    #[ts(skip)]
    pub context: String,
    pub reasoning: String,
    pub tool_calls: Vec<AiToolCall>,
    #[ts(optional)]
    pub tool_call_id: Option<String>,
    #[ts(optional)]
    pub tool_name: Option<String>,
    pub attachments: Vec<AiAttachment>,
    pub status: AiMessageStatus,
    #[ts(optional)]
    pub error: Option<CoreError>,
    #[ts(optional)]
    pub usage: Option<AiUsage>,
    pub created_at: Timestamp,
}

/// A message to add; `id`, `seq` and `created_at` are set by the repository.
#[derive(Debug, Clone)]
pub struct NewAiMessage {
    pub role: AiRole,
    pub content: String,
    pub context: String,
    pub tool_calls: Vec<AiToolCall>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub attachments: Vec<AiAttachment>,
    pub status: AiMessageStatus,
}

impl NewAiMessage {
    pub fn new(role: AiRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            context: String::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
            attachments: Vec::new(),
            status: AiMessageStatus::Done,
        }
    }
}

pub struct AiRepo<'a> {
    storage: &'a Storage,
    clock: &'a dyn Clock,
}

fn json<T: serde::de::DeserializeOwned>(row: &SqliteRow, col: &str) -> CoreResult<T> {
    serde_json::from_str(row.get(col)).map_err(db_err)
}

fn opt_json<T: serde::de::DeserializeOwned>(row: &SqliteRow, col: &str) -> CoreResult<Option<T>> {
    row.get::<Option<&str>, _>(col)
        .map(|s| serde_json::from_str(s).map_err(db_err))
        .transpose()
}

fn to_json<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value).map_err(db_err)
}

fn chat_from_row(row: &SqliteRow) -> CoreResult<AiChat> {
    Ok(AiChat {
        id: row.get::<&str, _>("id").parse()?,
        workspace_id: row.get::<&str, _>("workspace_id").parse()?,
        owner: row
            .get::<Option<&str>, _>("owner")
            .map(str::parse)
            .transpose()?,
        title: row.get("title"),
        provider_id: row.get("provider_id"),
        model: row.get("model"),
        auto_approve: row.get::<i64, _>("auto_approve") != 0,
        created_at: Timestamp(row.get("created_at")),
        updated_at: Timestamp(row.get("updated_at")),
    })
}

fn message_from_row(row: &SqliteRow) -> CoreResult<AiChatMessage> {
    let role = match row.get::<&str, _>("role") {
        "user" => AiRole::User,
        "tool" => AiRole::Tool,
        _ => AiRole::Assistant,
    };
    Ok(AiChatMessage {
        id: row.get::<&str, _>("id").parse()?,
        chat_id: row.get::<&str, _>("chat_id").parse()?,
        seq: row.get("seq"),
        role,
        content: row.get("content"),
        context: row.get("context"),
        reasoning: row.get("reasoning"),
        tool_calls: json(row, "tool_calls")?,
        tool_call_id: row.get("tool_call_id"),
        tool_name: row.get("tool_name"),
        attachments: json(row, "attachments")?,
        status: AiMessageStatus::parse(row.get("status")),
        error: opt_json(row, "error")?,
        usage: opt_json(row, "usage")?,
        created_at: Timestamp(row.get("created_at")),
    })
}

fn role_str(role: AiRole) -> &'static str {
    match role {
        AiRole::User => "user",
        AiRole::Assistant => "assistant",
        AiRole::Tool => "tool",
    }
}

fn not_found() -> CoreError {
    CoreError::new("not_found", "chat not found")
}

impl<'a> AiRepo<'a> {
    pub fn new(storage: &'a Storage, clock: &'a dyn Clock) -> Self {
        Self { storage, clock }
    }

    pub async fn create_chat(
        &self,
        workspace_id: Id,
        owner: Option<Id>,
        title: &str,
        provider_id: Option<&str>,
        model: Option<&str>,
    ) -> CoreResult<AiChat> {
        let now = self.clock.now();
        let chat = AiChat {
            id: Id::new(),
            workspace_id,
            owner,
            title: title.to_string(),
            provider_id: provider_id.map(str::to_string),
            model: model.map(str::to_string),
            auto_approve: false,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO ai_chats (id, workspace_id, owner, title, provider_id, model, auto_approve, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(chat.id.to_string())
        .bind(workspace_id.to_string())
        .bind(owner.map(|o| o.to_string()))
        .bind(&chat.title)
        .bind(&chat.provider_id)
        .bind(&chat.model)
        .bind(now.millis())
        .bind(now.millis())
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(chat)
    }

    /// Chats of a workspace that belong to `owner` (`None`: single-user
    /// setups), most recently active first.
    pub async fn list_chats(&self, workspace_id: Id, owner: Option<Id>) -> CoreResult<Vec<AiChat>> {
        let rows = sqlx::query(
            "SELECT * FROM ai_chats WHERE workspace_id = ? AND owner IS ? AND deleted_at IS NULL
             ORDER BY updated_at DESC, created_at DESC",
        )
        .bind(workspace_id.to_string())
        .bind(owner.map(|o| o.to_string()))
        .fetch_all(self.storage.pool())
        .await
        .map_err(db_err)?;
        rows.iter().map(chat_from_row).collect()
    }

    pub async fn get_chat(&self, id: Id) -> CoreResult<AiChat> {
        let row = sqlx::query("SELECT * FROM ai_chats WHERE id = ? AND deleted_at IS NULL")
            .bind(id.to_string())
            .fetch_optional(self.storage.pool())
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        chat_from_row(&row)
    }

    /// Runs an `UPDATE ai_chats ...` whose last two binds are `updated_at`
    /// and the id; the caller binds the rest.
    async fn update_chat<'q>(
        &self,
        id: Id,
        query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    ) -> CoreResult<AiChat> {
        let done = query
            .bind(self.clock.now().millis())
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await
            .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(not_found());
        }
        self.get_chat(id).await
    }

    pub async fn rename_chat(&self, id: Id, title: &str) -> CoreResult<AiChat> {
        let q = sqlx::query(
            "UPDATE ai_chats SET title = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(title.to_string());
        self.update_chat(id, q).await
    }

    pub async fn set_chat_model(
        &self,
        id: Id,
        provider_id: &str,
        model: &str,
    ) -> CoreResult<AiChat> {
        let q = sqlx::query(
            "UPDATE ai_chats SET provider_id = ?, model = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(provider_id.to_string())
        .bind(model.to_string());
        self.update_chat(id, q).await
    }

    pub async fn set_auto_approve(&self, id: Id, on: bool) -> CoreResult<AiChat> {
        let q = sqlx::query(
            "UPDATE ai_chats SET auto_approve = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(i64::from(on));
        self.update_chat(id, q).await
    }

    /// Moves the chat to the top of the list.
    pub async fn touch_chat(&self, id: Id) -> CoreResult<AiChat> {
        let q =
            sqlx::query("UPDATE ai_chats SET updated_at = ? WHERE id = ? AND deleted_at IS NULL");
        self.update_chat(id, q).await
    }

    pub async fn delete_chat(&self, id: Id) -> CoreResult<()> {
        let done =
            sqlx::query("UPDATE ai_chats SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
                .bind(self.clock.now().millis())
                .bind(id.to_string())
                .execute(self.storage.pool())
                .await
                .map_err(db_err)?;
        if done.rows_affected() == 0 {
            return Err(not_found());
        }
        Ok(())
    }

    /// A new chat (same workspace, owner and model) with copies of the
    /// messages of `source` up to `last_seq`; unfinished ones are cancelled.
    pub async fn fork_chat(
        &self,
        source: &AiChat,
        last_seq: i64,
        title: &str,
    ) -> CoreResult<AiChat> {
        let now = self.clock.now();
        let chat = AiChat {
            id: Id::new(),
            title: title.to_string(),
            auto_approve: false,
            created_at: now,
            updated_at: now,
            ..source.clone()
        };
        let mut tx = self.storage.begin_write().await?;
        sqlx::query(
            "INSERT INTO ai_chats (id, workspace_id, owner, title, provider_id, model, auto_approve, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(chat.id.to_string())
        .bind(chat.workspace_id.to_string())
        .bind(chat.owner.map(|o| o.to_string()))
        .bind(&chat.title)
        .bind(&chat.provider_id)
        .bind(&chat.model)
        .bind(now.millis())
        .bind(now.millis())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM ai_messages WHERE chat_id = ? AND seq <= ? ORDER BY seq",
        )
        .bind(source.id.to_string())
        .bind(last_seq)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        for old in ids {
            sqlx::query(
                "INSERT INTO ai_messages (id, chat_id, seq, role, content, context, reasoning, tool_calls, tool_call_id, tool_name, attachments, status, error, usage, created_at)
                 SELECT ?, ?, seq, role, content, context, reasoning, tool_calls, tool_call_id, tool_name, attachments,
                        CASE status WHEN 'pending' THEN 'cancelled' ELSE status END, error, usage, created_at
                 FROM ai_messages WHERE id = ?",
            )
            .bind(Id::new().to_string())
            .bind(chat.id.to_string())
            .bind(old)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(chat)
    }

    pub async fn add_message(&self, chat_id: Id, m: NewAiMessage) -> CoreResult<AiChatMessage> {
        let now = self.clock.now();
        let id = Id::new();
        let mut tx = self.storage.begin_write().await?;
        let seq: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM ai_messages WHERE chat_id = ?",
        )
        .bind(chat_id.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        sqlx::query(
            "INSERT INTO ai_messages (id, chat_id, seq, role, content, context, tool_calls, tool_call_id, tool_name, attachments, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(chat_id.to_string())
        .bind(seq)
        .bind(role_str(m.role))
        .bind(&m.content)
        .bind(&m.context)
        .bind(to_json(&m.tool_calls)?)
        .bind(&m.tool_call_id)
        .bind(&m.tool_name)
        .bind(to_json(&m.attachments)?)
        .bind(m.status.as_str())
        .bind(now.millis())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(AiChatMessage {
            id,
            chat_id,
            seq,
            role: m.role,
            content: m.content,
            context: m.context,
            reasoning: String::new(),
            tool_calls: m.tool_calls,
            tool_call_id: m.tool_call_id,
            tool_name: m.tool_name,
            attachments: m.attachments,
            status: m.status,
            error: None,
            usage: None,
            created_at: now,
        })
    }

    /// Writes the changing parts of a message (text, tool calls, status,
    /// error, usage).
    pub async fn save_message(&self, m: &AiChatMessage) -> CoreResult<()> {
        sqlx::query(
            "UPDATE ai_messages SET content = ?, reasoning = ?, tool_calls = ?, status = ?, error = ?, usage = ?
             WHERE id = ?",
        )
        .bind(&m.content)
        .bind(&m.reasoning)
        .bind(to_json(&m.tool_calls)?)
        .bind(m.status.as_str())
        .bind(m.error.as_ref().map(to_json).transpose()?)
        .bind(m.usage.as_ref().map(to_json).transpose()?)
        .bind(m.id.to_string())
        .execute(self.storage.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn messages(&self, chat_id: Id) -> CoreResult<Vec<AiChatMessage>> {
        let rows = sqlx::query("SELECT * FROM ai_messages WHERE chat_id = ? ORDER BY seq")
            .bind(chat_id.to_string())
            .fetch_all(self.storage.pool())
            .await
            .map_err(db_err)?;
        rows.iter().map(message_from_row).collect()
    }

    /// Marks messages left `pending` (the app stopped mid-answer) as
    /// cancelled; returns how many.
    pub async fn cancel_pending(&self) -> CoreResult<u64> {
        let done =
            sqlx::query("UPDATE ai_messages SET status = 'cancelled' WHERE status = 'pending'")
                .execute(self.storage.pool())
                .await
                .map_err(db_err)?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use reqly_core::FixedClock;

    use super::*;

    async fn setup() -> (Storage, FixedClock) {
        (Storage::in_memory().await.unwrap(), FixedClock::new(1_000))
    }

    #[tokio::test]
    async fn chats_per_workspace_and_owner_newest_first() {
        let (storage, clock) = setup().await;
        let repo = AiRepo::new(&storage, &clock);
        let ws = Id::new();
        let other_ws = Id::new();
        let ann = Id::new();
        let a = repo.create_chat(ws, None, "A", None, None).await.unwrap();
        clock.advance(10);
        let b = repo
            .create_chat(ws, None, "B", Some("p1"), Some("m"))
            .await
            .unwrap();
        repo.create_chat(other_ws, None, "C", None, None)
            .await
            .unwrap();
        repo.create_chat(ws, Some(ann), "Ann's", None, None)
            .await
            .unwrap();

        let titles = |list: Vec<AiChat>| list.into_iter().map(|c| c.title).collect::<Vec<_>>();
        assert_eq!(titles(repo.list_chats(ws, None).await.unwrap()), ["B", "A"]);
        assert_eq!(
            titles(repo.list_chats(ws, Some(ann)).await.unwrap()),
            ["Ann's"]
        );

        clock.advance(10);
        repo.touch_chat(a.id).await.unwrap();
        assert_eq!(titles(repo.list_chats(ws, None).await.unwrap()), ["A", "B"]);

        let renamed = repo.rename_chat(b.id, "Bee").await.unwrap();
        assert_eq!(renamed.title, "Bee");
        let moved = repo.set_chat_model(b.id, "p2", "m2").await.unwrap();
        assert_eq!(
            (moved.provider_id.as_deref(), moved.model.as_deref()),
            (Some("p2"), Some("m2"))
        );
        assert!(
            repo.set_auto_approve(b.id, true)
                .await
                .unwrap()
                .auto_approve
        );
    }

    #[tokio::test]
    async fn deleted_chats_disappear() {
        let (storage, clock) = setup().await;
        let repo = AiRepo::new(&storage, &clock);
        let ws = Id::new();
        let chat = repo.create_chat(ws, None, "A", None, None).await.unwrap();
        repo.delete_chat(chat.id).await.unwrap();
        assert!(repo.list_chats(ws, None).await.unwrap().is_empty());
        assert_eq!(repo.get_chat(chat.id).await.unwrap_err().code, "not_found");
        assert_eq!(
            repo.rename_chat(chat.id, "x").await.unwrap_err().code,
            "not_found"
        );
        assert_eq!(
            repo.delete_chat(chat.id).await.unwrap_err().code,
            "not_found"
        );
    }

    #[tokio::test]
    async fn messages_keep_their_order_and_fields() {
        let (storage, clock) = setup().await;
        let repo = AiRepo::new(&storage, &clock);
        let chat = repo
            .create_chat(Id::new(), None, "A", None, None)
            .await
            .unwrap();
        let mut user = NewAiMessage::new(AiRole::User, "Why 500?");
        user.context = "Request: GET /x".into();
        user.attachments = vec![AiAttachment {
            kind: "request".into(),
            id: Id::new(),
            label: "GET /x".into(),
        }];
        let u = repo.add_message(chat.id, user).await.unwrap();
        let mut a = repo
            .add_message(
                chat.id,
                NewAiMessage {
                    status: AiMessageStatus::Pending,
                    ..NewAiMessage::new(AiRole::Assistant, "")
                },
            )
            .await
            .unwrap();
        a.content = "Let me look.".into();
        a.reasoning = "hmm".into();
        a.tool_calls = vec![AiToolCall {
            id: "call_1".into(),
            name: "get_request".into(),
            arguments: "{}".into(),
        }];
        a.status = AiMessageStatus::Done;
        a.usage = Some(AiUsage {
            prompt_tokens: 5,
            completion_tokens: 7,
        });
        repo.save_message(&a).await.unwrap();
        let mut t = repo
            .add_message(
                chat.id,
                NewAiMessage {
                    tool_call_id: Some("call_1".into()),
                    tool_name: Some("get_request".into()),
                    ..NewAiMessage::new(AiRole::Tool, "{}")
                },
            )
            .await
            .unwrap();
        t.status = AiMessageStatus::Error;
        t.error = Some(CoreError::new("not_found", "gone"));
        repo.save_message(&t).await.unwrap();

        let list = repo.messages(chat.id).await.unwrap();
        assert_eq!(list.iter().map(|m| m.seq).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(list[0], u);
        assert_eq!(list[0].context, "Request: GET /x");
        assert_eq!(list[1], a);
        assert_eq!(list[2].error.as_ref().unwrap().code, "not_found");
        assert_eq!(list[2].tool_call_id.as_deref(), Some("call_1"));
    }

    #[tokio::test]
    async fn pending_messages_become_cancelled() {
        let (storage, clock) = setup().await;
        let repo = AiRepo::new(&storage, &clock);
        let chat = repo
            .create_chat(Id::new(), None, "A", None, None)
            .await
            .unwrap();
        repo.add_message(
            chat.id,
            NewAiMessage {
                status: AiMessageStatus::Pending,
                ..NewAiMessage::new(AiRole::Assistant, "half")
            },
        )
        .await
        .unwrap();
        repo.add_message(chat.id, NewAiMessage::new(AiRole::User, "hi"))
            .await
            .unwrap();
        assert_eq!(repo.cancel_pending().await.unwrap(), 1);
        let list = repo.messages(chat.id).await.unwrap();
        assert_eq!(list[0].status, AiMessageStatus::Cancelled);
        assert_eq!(list[1].status, AiMessageStatus::Done);
    }
}
