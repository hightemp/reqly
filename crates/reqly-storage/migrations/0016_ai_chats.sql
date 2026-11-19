-- AI chat conversations (ADR-015). Local data: not synced, not exported.
CREATE TABLE ai_chats (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    -- The user who owns the chat (multi-user server); NULL otherwise.
    owner TEXT,
    title TEXT NOT NULL,
    provider_id TEXT,
    model TEXT,
    -- "Don't ask again" for tool calls in this chat.
    auto_approve INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE INDEX ai_chats_by_workspace ON ai_chats (workspace_id, owner, updated_at);

CREATE TABLE ai_messages (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL REFERENCES ai_chats (id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    -- Text sent to the model with a user message (attachments), not shown.
    context TEXT NOT NULL DEFAULT '',
    reasoning TEXT NOT NULL DEFAULT '',
    tool_calls TEXT NOT NULL DEFAULT '[]',
    tool_call_id TEXT,
    tool_name TEXT,
    attachments TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'done',
    error TEXT,
    usage TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (chat_id, seq)
);
