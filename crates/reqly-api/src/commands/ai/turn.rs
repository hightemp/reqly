//! One turn of a chat (ADR-015): the user's message, then model calls and
//! tool calls until the model answers without tools (at most
//! [`MAX_STEPS`] model calls). Progress goes out as events; the command
//! returns when the turn ends.

use std::sync::{Arc, Mutex};

use reqly_ai::{ChatRequest, Delta, Message, ToolSpec, tool_error_text, tool_result_text};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::CancelToken;
use reqly_storage::ai::{
    AiAttachment, AiChat, AiChatMessage, AiMessageStatus, AiRepo, AiRole, AiToolCall, AiUsage,
    NewAiMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::chats::{chats_changed, own_chat};
use super::config::{AiConfig, load_config, needs_confirmation, provider_client};
use super::transport::HttpTransport;
use super::turns::Decision;
use crate::access::{self, Caller};
use crate::agent::{ToolContext, Tools, catalog_tools};
use crate::commands::AppContext;

/// Model calls per turn.
pub const MAX_STEPS: usize = 20;
/// Tool results of earlier turns are sent cut to this many bytes.
const OLD_TOOL_RESULT: usize = 2048;
/// Response bodies attached to a message are cut to this many bytes.
const ATTACHED_BODY: u64 = 16 * 1024;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiAttachmentRef {
    /// `request` (a request id) or `response` (a history record id).
    pub kind: String,
    pub id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiSendInput {
    pub chat_id: Id,
    pub text: String,
    #[serde(default)]
    #[ts(as = "Option<Vec<AiAttachmentRef>>", optional)]
    pub attachments: Vec<AiAttachmentRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AiTurnStatus {
    Done,
    Cancelled,
    Error,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiSendResult {
    pub status: AiTurnStatus,
}

/// Where a turn's events go: the chat's owner.
#[derive(Clone)]
struct Out {
    events: crate::events::EventBus,
    chat: Id,
    actor: Option<Id>,
}

impl Out {
    fn publish(&self, topic: &str, payload: Value) {
        self.events
            .publish(crate::Event::new(topic, payload).by(self.actor));
    }

    fn message(&self, m: &AiChatMessage) {
        self.publish("ai.message", json!({ "chatId": self.chat, "message": m }));
    }
}

pub(crate) async fn send(ctx: &AppContext, input: AiSendInput) -> CoreResult<AiSendResult> {
    let text = input.text.trim().to_string();
    if text.is_empty() {
        return Err(CoreError::new("ai.empty_message", "the message is empty"));
    }
    let chat = own_chat(ctx, input.chat_id).await?;
    let config = load_config(&ctx.storage).await?;
    let (Some(provider_id), Some(model)) = (chat.provider_id.clone(), chat.model.clone()) else {
        return Err(CoreError::new(
            "ai.no_model",
            "choose a model for this chat",
        ));
    };
    let provider = provider_client(ctx, &config, &provider_id).await?;
    let caller = access::current();
    let cx = ToolContext::new(ctx, caller);
    let tools = catalog_tools();
    let (context, attachments) = attachments(cx, &tools, &input.attachments).await?;
    let Some(cancel) = ctx.ai_turns.start(chat.id) else {
        return Err(CoreError::new(
            "ai.busy",
            "this chat is still answering; stop it first",
        ));
    };
    let out = Out {
        events: ctx.events.clone(),
        chat: chat.id,
        actor: caller.user_id(),
    };
    let turn = Turn {
        ctx,
        cx,
        caller,
        tools,
        config,
        provider,
        model,
        chat: chat.clone(),
        cancel,
        out: out.clone(),
    };
    let result = turn.run(text, context, attachments).await;
    ctx.ai_turns.finish(chat.id);
    let status = match &result {
        Ok(s) => *s,
        Err(_) => AiTurnStatus::Error,
    };
    let repo = AiRepo::new(&ctx.storage, ctx.clock.as_ref());
    if repo.touch_chat(chat.id).await.is_ok() {
        chats_changed(ctx, chat.workspace_id);
    }
    out.publish("ai.done", json!({ "chatId": chat.id, "status": status }));
    Ok(AiSendResult { status: result? })
}

/// The attached request or response as masked JSON for the model, and what
/// the chat shows of it.
async fn attachments(
    cx: ToolContext<'_>,
    tools: &Tools,
    refs: &[AiAttachmentRef],
) -> CoreResult<(String, Vec<AiAttachment>)> {
    let mut context = String::new();
    let mut shown = Vec::new();
    for r in refs {
        let (tool, args, title) = match r.kind.as_str() {
            "request" => ("get_request", json!({ "id": r.id }), "Attached request"),
            "response" => (
                "get_response",
                json!({ "id": r.id, "maxBodyBytes": ATTACHED_BODY }),
                "Attached response",
            ),
            _ => {
                return Err(
                    CoreError::new("ai.invalid_attachment", "unknown attachment kind")
                        .with_param("kind", r.kind.as_str()),
                );
            }
        };
        let value = tools
            .get(tool)
            .ok_or_else(|| CoreError::internal("attachment tool missing"))?
            .run(cx, args)
            .await?;
        let label = match r.kind.as_str() {
            "request" => format!(
                "{} {}",
                value["data"]["method"].as_str().unwrap_or("REQ"),
                value["name"].as_str().unwrap_or("")
            ),
            _ => match value["status"].as_u64() {
                Some(s) => format!("Response {s}"),
                None => "Response".to_string(),
            },
        };
        context.push_str(&format!(
            "\n\n{title} (id {}):\n```json\n{}\n```",
            r.id,
            serde_json::to_string_pretty(&value).unwrap_or_default()
        ));
        shown.push(AiAttachment {
            kind: r.kind.clone(),
            id: r.id,
            label: label.trim().to_string(),
        });
    }
    Ok((context.trim_start().to_string(), shown))
}

/// The first line of the first message, shortened.
fn title_of(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    match line.char_indices().nth(60) {
        Some((i, _)) => format!("{}…", line[..i].trim_end()),
        None => line.to_string(),
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}… (shortened; call the tool again for everything)",
        &text[..end]
    )
}

/// The stored chat in the wire roles. Failed or unfinished answers are left
/// out; every tool call gets a result right after it (a stopped one says
/// so); results of earlier turns are shortened.
fn history(system: &str, messages: &[AiChatMessage]) -> Vec<Message> {
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == AiRole::User)
        .map(|m| m.seq)
        .unwrap_or(0);
    let mut out = vec![Message::System {
        content: system.to_string(),
    }];
    for m in messages {
        match m.role {
            AiRole::User => out.push(Message::User {
                content: if m.context.is_empty() {
                    m.content.clone()
                } else {
                    format!("{}\n\n{}", m.content, m.context)
                },
            }),
            AiRole::Assistant => {
                let usable = match m.status {
                    AiMessageStatus::Done => true,
                    AiMessageStatus::Cancelled => m.tool_calls.is_empty(),
                    _ => false,
                };
                if !usable || (m.content.is_empty() && m.tool_calls.is_empty()) {
                    continue;
                }
                out.push(Message::Assistant {
                    content: (!m.content.is_empty()).then(|| m.content.clone()),
                    tool_calls: m
                        .tool_calls
                        .iter()
                        .map(|c| reqly_ai::ToolCall {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            arguments: c.arguments.clone(),
                        })
                        .collect(),
                });
                for call in &m.tool_calls {
                    let result = messages.iter().find(|t| {
                        t.role == AiRole::Tool && t.tool_call_id.as_deref() == Some(&call.id)
                    });
                    let content = match result {
                        Some(t) if !t.content.is_empty() => {
                            if t.seq < last_user {
                                cut(&t.content, OLD_TOOL_RESULT)
                            } else {
                                t.content.clone()
                            }
                        }
                        _ => tool_error_text("cancelled", "The call was not run."),
                    };
                    out.push(Message::Tool {
                        tool_call_id: call.id.clone(),
                        content,
                    });
                }
            }
            // Sent right after their call.
            AiRole::Tool => {}
        }
    }
    out
}

struct Turn<'a> {
    ctx: &'a AppContext,
    cx: ToolContext<'a>,
    caller: Caller,
    tools: Tools,
    config: AiConfig,
    provider: reqly_ai::Provider,
    model: String,
    chat: AiChat,
    cancel: CancelToken,
    out: Out,
}

impl Turn<'_> {
    fn repo(&self) -> AiRepo<'_> {
        AiRepo::new(&self.ctx.storage, self.ctx.clock.as_ref())
    }

    async fn system_prompt(&self) -> String {
        let ws = self.chat.workspace_id;
        let workspaces = self
            .cx
            .call("workspace.list", json!({}))
            .await
            .unwrap_or_default();
        let ws_name = workspaces
            .as_array()
            .into_iter()
            .flatten()
            .find(|w| w["id"] == json!(ws))
            .and_then(|w| w["name"].as_str())
            .unwrap_or("")
            .to_string();
        let mut collections = String::new();
        if let Some(tool) = self.tools.get("list_collections")
            && let Ok(v) = tool.run(self.cx, json!({ "workspaceId": ws })).await
        {
            for c in v["collections"].as_array().into_iter().flatten() {
                collections.push_str(&format!(
                    "\n- \"{}\" (id {})",
                    c["name"].as_str().unwrap_or(""),
                    c["id"].as_str().unwrap_or("")
                ));
            }
        }
        if collections.is_empty() {
            collections.push_str("\n(none yet)");
        }
        format!(
            "You are the assistant built into Reqly, an HTTP and API client. You help the user \
             design, send, test and debug HTTP requests (also GraphQL, WebSocket, gRPC), \
             organise collections and environments, and read responses.\n\n\
             You can use tools to read and change the user's requests, folders and \
             environments, send requests and run collections. Prefer tools over guessing: look \
             things up by id before changing them. Calls that change data may need the user's \
             approval; if one is declined, do not try it again unless asked. Ids are opaque: \
             use the ones tools return. Secret values appear as ••••; never ask for them.\n\n\
             Current workspace: \"{ws_name}\" (id {ws}).\nIts collections:{collections}\n\n\
             Answer in the language the user writes in. Be concise. Use Markdown; put code, \
             JSON and HTTP in fenced blocks."
        )
    }

    async fn run(
        mut self,
        text: String,
        context: String,
        attachments: Vec<AiAttachment>,
    ) -> CoreResult<AiTurnStatus> {
        let repo = self.repo();
        let user = repo
            .add_message(
                self.chat.id,
                NewAiMessage {
                    context,
                    attachments,
                    ..NewAiMessage::new(AiRole::User, text.clone())
                },
            )
            .await?;
        self.out.message(&user);
        if self.chat.title.is_empty() {
            self.chat = repo.rename_chat(self.chat.id, &title_of(&text)).await?;
            chats_changed(self.ctx, self.chat.workspace_id);
        }
        let specs: Vec<ToolSpec> = self
            .tools
            .iter()
            .filter(|t| self.config.tool_enabled(t.name))
            .map(|t| ToolSpec {
                name: t.name.to_string(),
                description: t.description.to_string(),
                parameters: t.input_schema.clone(),
            })
            .collect();
        let system = self.system_prompt().await;

        for _ in 0..MAX_STEPS {
            let messages = history(&system, &self.repo().messages(self.chat.id).await?);
            let mut answer = self
                .repo()
                .add_message(
                    self.chat.id,
                    NewAiMessage {
                        status: AiMessageStatus::Pending,
                        ..NewAiMessage::new(AiRole::Assistant, "")
                    },
                )
                .await?;
            self.out.message(&answer);

            let so_far = Arc::new(Mutex::new((String::new(), String::new())));
            let on_delta = {
                let so_far = so_far.clone();
                let out = self.out.clone();
                let message = answer.id;
                move |d: Delta| {
                    let (kind, text) = match d {
                        Delta::Text(t) => {
                            if let Ok(mut s) = so_far.lock() {
                                s.0.push_str(&t);
                            }
                            ("text", t)
                        }
                        Delta::Reasoning(t) => {
                            if let Ok(mut s) = so_far.lock() {
                                s.1.push_str(&t);
                            }
                            ("reasoning", t)
                        }
                        Delta::ToolCall { name } => ("toolCall", name),
                    };
                    out.publish(
                        "ai.delta",
                        json!({ "chatId": out.chat, "messageId": message, "kind": kind, "text": text }),
                    );
                }
            };
            let request = ChatRequest {
                model: self.model.clone(),
                messages,
                tools: specs.clone(),
                temperature: None,
            };
            let transport = HttpTransport::new(self.ctx, self.cancel.clone());
            let result = reqly_ai::chat(&transport, &self.provider, &request, on_delta).await;
            let completion = match result {
                Ok(c) => c,
                Err(e) => {
                    let (text, reasoning) = so_far.lock().map(|s| s.clone()).unwrap_or_default();
                    answer.content = text;
                    answer.reasoning = reasoning;
                    let status = if e.code == "ai.cancelled" || self.cancel.is_cancelled() {
                        answer.status = AiMessageStatus::Cancelled;
                        AiTurnStatus::Cancelled
                    } else {
                        answer.status = AiMessageStatus::Error;
                        answer.error = Some(e);
                        AiTurnStatus::Error
                    };
                    self.repo().save_message(&answer).await?;
                    self.out.message(&answer);
                    return Ok(status);
                }
            };
            answer.content = completion.content;
            answer.reasoning = completion.reasoning;
            answer.tool_calls = completion
                .tool_calls
                .iter()
                .map(|c| AiToolCall {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    arguments: c.arguments.clone(),
                })
                .collect();
            answer.usage = completion.usage.map(|u| AiUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
            });
            answer.status = AiMessageStatus::Done;
            self.repo().save_message(&answer).await?;
            self.out.message(&answer);
            if answer.tool_calls.is_empty() {
                return Ok(AiTurnStatus::Done);
            }
            for call in &answer.tool_calls {
                self.tool_call(call).await?;
            }
            if self.cancel.is_cancelled() {
                return Ok(AiTurnStatus::Cancelled);
            }
        }

        let mut stop = self
            .repo()
            .add_message(self.chat.id, NewAiMessage::new(AiRole::Assistant, ""))
            .await?;
        stop.status = AiMessageStatus::Error;
        stop.error = Some(
            CoreError::new("ai.step_limit", "the answer took too many steps")
                .with_param("steps", MAX_STEPS),
        );
        self.repo().save_message(&stop).await?;
        self.out.message(&stop);
        Ok(AiTurnStatus::Error)
    }

    /// Runs one tool call (asking first when needed) and stores its result.
    async fn tool_call(&mut self, call: &AiToolCall) -> CoreResult<()> {
        let mut msg = self
            .repo()
            .add_message(
                self.chat.id,
                NewAiMessage {
                    status: AiMessageStatus::Pending,
                    tool_call_id: Some(call.id.clone()),
                    tool_name: Some(call.name.clone()),
                    ..NewAiMessage::new(AiRole::Tool, "")
                },
            )
            .await?;
        self.out.message(&msg);
        let (status, content, error) = self.outcome(call, msg.id).await;
        msg.status = status;
        msg.content = content;
        msg.error = error;
        self.repo().save_message(&msg).await?;
        self.out.message(&msg);
        Ok(())
    }

    async fn outcome(
        &mut self,
        call: &AiToolCall,
        message: Id,
    ) -> (AiMessageStatus, String, Option<CoreError>) {
        let stopped = || {
            (
                AiMessageStatus::Cancelled,
                tool_error_text("cancelled", "The user stopped the answer."),
                None,
            )
        };
        let failed = |e: CoreError| {
            (
                AiMessageStatus::Error,
                tool_error_text(&e.code, &e.message),
                Some(e),
            )
        };
        if self.cancel.is_cancelled() {
            return stopped();
        }
        let Some(tool) = self
            .tools
            .get(&call.name)
            .filter(|t| self.config.tool_enabled(t.name))
        else {
            return failed(
                CoreError::new("ai.tool_unavailable", "this tool is not available")
                    .with_param("tool", call.name.as_str()),
            );
        };
        let args: Value = match serde_json::from_str(&call.arguments) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                return failed(CoreError::new(
                    "mcp.invalid_arguments",
                    "the arguments are not a JSON object",
                ));
            }
        };
        let ask = needs_confirmation(&self.config, tool.group, tool.annotations.read_only_hint)
            && !self.chat.auto_approve;
        if ask {
            let Some(rx) = self.ctx.ai_turns.wait(self.chat.id, &call.id) else {
                return stopped();
            };
            self.out.publish(
                "ai.confirm",
                json!({
                    "chatId": self.chat.id,
                    "messageId": message,
                    "callId": call.id,
                    "toolName": call.name,
                    "arguments": args,
                }),
            );
            match rx.await {
                Ok(Decision::Allow) => {}
                Ok(Decision::AllowAlways) => {
                    if let Ok(chat) = self.repo().set_auto_approve(self.chat.id, true).await {
                        self.chat = chat;
                    }
                }
                Ok(Decision::Deny) | Err(_) => {
                    if self.cancel.is_cancelled() {
                        return stopped();
                    }
                    return (
                        AiMessageStatus::Denied,
                        tool_error_text("declined", "The user declined this call."),
                        None,
                    );
                }
            }
        }
        let cx = ToolContext::new(self.ctx, self.caller);
        match access::scope(self.caller, tool.run(cx, args)).await {
            Ok(v) => (AiMessageStatus::Done, tool_result_text(&v), None),
            Err(e) => failed(e),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AiDecision {
    Allow,
    AllowAlways,
    Deny,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiConfirmInput {
    pub chat_id: Id,
    pub call_id: String,
    pub decision: AiDecision,
}

pub(crate) async fn confirm(ctx: &AppContext, input: AiConfirmInput) -> CoreResult<()> {
    own_chat(ctx, input.chat_id).await?;
    let decision = match input.decision {
        AiDecision::Allow => Decision::Allow,
        AiDecision::AllowAlways => Decision::AllowAlways,
        AiDecision::Deny => Decision::Deny,
    };
    if !ctx.ai_turns.decide(input.chat_id, &input.call_id, decision) {
        return Err(CoreError::new(
            "ai.not_waiting",
            "no tool call waits for this answer",
        ));
    }
    Ok(())
}

pub(crate) async fn cancel(ctx: &AppContext, chat_id: Id) -> CoreResult<()> {
    own_chat(ctx, chat_id).await?;
    ctx.ai_turns.cancel(chat_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use reqly_core::Timestamp;

    use super::*;

    fn msg(seq: i64, role: AiRole, content: &str) -> AiChatMessage {
        AiChatMessage {
            id: Id::new(),
            chat_id: Id::new(),
            seq,
            role,
            content: content.into(),
            context: String::new(),
            reasoning: String::new(),
            tool_calls: vec![],
            tool_call_id: None,
            tool_name: None,
            attachments: vec![],
            status: AiMessageStatus::Done,
            error: None,
            usage: None,
            created_at: Timestamp(0),
        }
    }

    #[test]
    fn titles_are_the_first_line_shortened() {
        assert_eq!(title_of("Why 500?\nmore"), "Why 500?");
        let long = "a".repeat(80);
        assert_eq!(title_of(&long).chars().count(), 61);
        assert!(title_of(&long).ends_with('…'));
    }

    #[test]
    fn earlier_tool_results_are_shortened() {
        let big = "x".repeat(5000);
        let mut call = msg(2, AiRole::Assistant, "");
        call.tool_calls = vec![AiToolCall {
            id: "c1".into(),
            name: "get_tree".into(),
            arguments: "{}".into(),
        }];
        let mut result = msg(3, AiRole::Tool, &big);
        result.tool_call_id = Some("c1".into());
        let mut now_call = msg(5, AiRole::Assistant, "");
        now_call.tool_calls = vec![AiToolCall {
            id: "c2".into(),
            name: "get_tree".into(),
            arguments: "{}".into(),
        }];
        let mut now_result = msg(6, AiRole::Tool, &big);
        now_result.tool_call_id = Some("c2".into());
        let list = vec![
            msg(1, AiRole::User, "a"),
            call,
            result,
            msg(4, AiRole::User, "b"),
            now_call,
            now_result,
        ];
        let wire = history("sys", &list);
        let tool_lens: Vec<usize> = wire
            .iter()
            .filter_map(|m| match m {
                Message::Tool { content, .. } => Some(content.len()),
                _ => None,
            })
            .collect();
        assert_eq!(tool_lens.len(), 2);
        assert!(tool_lens[0] < 2200, "{}", tool_lens[0]);
        assert_eq!(tool_lens[1], 5000);
    }
}
