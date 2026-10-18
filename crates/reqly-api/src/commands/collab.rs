//! Collaborative editing of a node's text fields with Yjs (ADR-012).
//!
//! The server keeps one `yrs` document per (node, field) while people edit
//! it, relays updates and awareness between clients as events, and writes
//! the text back into the node so requests are sent with it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqly_core::model::Node;
use reqly_core::{CoreError, CoreResult, Id};
use reqly_storage::tree::TreeRepo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use yrs::updates::decoder::Decode;
use yrs::{
    Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact, Update,
};

use super::{AppContext, Empty};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<CollabJoin>()
        .register::<CollabUpdate>()
        .register::<CollabAwareness>();
}

/// Text fields that can be edited together, as dotted paths into node data.
pub const FIELDS: &[&str] = &[
    "url",
    "description",
    "body.text",
    "body.graphql.query",
    "body.graphql.variables",
    "scripts.preRequest",
    "scripts.afterResponse",
];

/// Documents nobody touched for this long are dropped.
const IDLE: Duration = Duration::from_secs(10 * 60);

struct Live {
    doc_id: String,
    doc: Doc,
    text: TextRef,
    used: Instant,
}

/// The live documents of the server.
#[derive(Default)]
pub struct Collab {
    docs: Mutex<HashMap<(Id, String), Live>>,
}

fn path_get<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(data, |v, key| v.get(key))
}

fn path_set(data: &mut Value, path: &str, value: String) -> bool {
    let mut cur = data;
    let mut keys = path.split('.').peekable();
    while let Some(key) = keys.next() {
        let Some(obj) = cur.as_object_mut() else {
            return false;
        };
        if keys.peek().is_none() {
            obj.insert(key.to_string(), Value::String(value));
            return true;
        }
        let Some(next) = obj.get_mut(key) else {
            return false;
        };
        cur = next;
    }
    false
}

/// Sets `path` (which starts with the key of `top`) inside `top`.
fn path_set_nested(top: &mut Value, path: &str, value: String) {
    if let Some((_, rest)) = path.split_once('.') {
        path_set(top, rest, value);
    }
}

fn field_error(field: &str) -> CoreError {
    CoreError::new("collab.field", "this field cannot be edited together")
        .with_param("field", field)
}

/// The current text of `field` in the node.
fn field_text(node: &Node, field: &str) -> CoreResult<String> {
    if !FIELDS.contains(&field) {
        return Err(field_error(field));
    }
    let data = serde_json::to_value(&node.data).map_err(|e| CoreError::internal(e.to_string()))?;
    path_get(&data, field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| field_error(field))
}

fn new_doc(text: &str) -> (Doc, TextRef) {
    // UTF-16 offsets, as Yjs in the browser.
    let doc = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    });
    let t = doc.get_or_insert_text("t");
    t.insert(&mut doc.transact_mut(), 0, text);
    (doc, t)
}

fn decode(b64: &str) -> CoreResult<Vec<u8>> {
    STANDARD
        .decode(b64)
        .map_err(|e| CoreError::new("command.invalid_input", e.to_string()))
}

/// Replaces `text` with `target` by the smallest single edit, so concurrent
/// edits elsewhere in the text survive. Returns the Yjs update, if any.
fn replace_text(doc: &Doc, text: &TextRef, target: &str) -> Option<Vec<u8>> {
    let current = text.get_string(&doc.transact());
    if current == target {
        return None;
    }
    let a: Vec<u16> = current.encode_utf16().collect();
    let b: Vec<u16> = target.encode_utf16().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let inserted = String::from_utf16_lossy(&b[prefix..b.len() - suffix]);
    let before = doc.transact().state_vector();
    {
        let mut txn = doc.transact_mut();
        let removed = (a.len() - suffix - prefix) as u32;
        if removed > 0 {
            text.remove_range(&mut txn, prefix as u32, removed);
        }
        if !inserted.is_empty() {
            text.insert(&mut txn, prefix as u32, &inserted);
        }
    }
    Some(doc.transact().encode_state_as_update_v1(&before))
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabUpdateEvent {
    pub node_id: Id,
    pub field: String,
    pub doc_id: String,
    /// Base64 Yjs update (v1).
    pub update: String,
    /// The Yjs client that made it (0 for the server); clients skip their own.
    pub client_id: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabAwarenessEvent {
    pub node_id: Id,
    pub field: String,
    /// Base64 y-protocols awareness update.
    pub update: String,
    pub client_id: u64,
}

impl Collab {
    /// Keeps live text inside objects a `node.update` replaces as a whole:
    /// a client changing `body.kind` sends the body with its stale text.
    /// A field sent on its own (`url`) is an explicit change and stays.
    pub fn keep_live_text(&self, node_id: Id, fields: &mut serde_json::Map<String, Value>) {
        let Ok(docs) = self.docs.lock() else {
            return;
        };
        for ((id, field), live) in docs.iter() {
            let Some((top, _)) = field.split_once('.') else {
                continue;
            };
            if *id == node_id
                && let Some(value) = fields.get_mut(top)
            {
                path_set_nested(value, field, live.text.get_string(&live.doc.transact()));
            }
        }
    }

    fn evict_idle(docs: &mut HashMap<(Id, String), Live>) {
        docs.retain(|_, live| live.used.elapsed() < IDLE);
    }

    /// Applies changes made to the node another way to its live documents
    /// and tells the editors.
    pub fn sync_from_node(&self, ctx: &AppContext, node: &Node) {
        let Ok(mut docs) = self.docs.lock() else {
            return;
        };
        for ((id, field), live) in docs.iter_mut() {
            if *id != node.meta.id {
                continue;
            }
            let Ok(target) = field_text(node, field) else {
                continue;
            };
            if let Some(update) = replace_text(&live.doc, &live.text, &target) {
                live.used = Instant::now();
                ctx.events.publish(Event::new(
                    "collab.update",
                    CollabUpdateEvent {
                        node_id: *id,
                        field: field.clone(),
                        doc_id: live.doc_id.clone(),
                        update: STANDARD.encode(update),
                        client_id: 0,
                    },
                ));
            }
        }
    }
}

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabJoinInput {
    pub node_id: Id,
    pub field: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabJoinOutput {
    pub doc_id: String,
    /// The whole document as a base64 Yjs update.
    pub update: String,
}

command!(
    /// Opens (creating from the stored text) the document of a field.
    CollabJoin,
    "collab.join",
    CollabJoinInput,
    CollabJoinOutput,
    |ctx, input| {
        let node = TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
            .get_node(input.node_id)
            .await?;
        let text = field_text(&node, &input.field)?;
        let mut docs = ctx
            .collab
            .docs
            .lock()
            .map_err(|_| CoreError::internal("collab lock"))?;
        Collab::evict_idle(&mut docs);
        let live = docs
            .entry((input.node_id, input.field.clone()))
            .or_insert_with(|| {
                let (doc, text) = new_doc(&text);
                Live {
                    doc_id: Id::new().to_string(),
                    doc,
                    text,
                    used: Instant::now(),
                }
            });
        live.used = Instant::now();
        let update = live
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        Ok(CollabJoinOutput {
            doc_id: live.doc_id.clone(),
            update: STANDARD.encode(update),
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabUpdateInput {
    pub node_id: Id,
    pub field: String,
    pub doc_id: String,
    pub update: String,
    pub client_id: u64,
}

command!(
    /// Applies a client's edit, stores the text and relays the edit.
    CollabUpdate,
    "collab.update",
    CollabUpdateInput,
    Empty,
    |ctx, input| {
        let bytes = decode(&input.update)?;
        let update =
            Update::decode_v1(&bytes).map_err(|e| CoreError::new("command.invalid_input", e.to_string()))?;
        let text = {
            let mut docs = ctx
                .collab
                .docs
                .lock()
                .map_err(|_| CoreError::internal("collab lock"))?;
            let live = docs
                .get_mut(&(input.node_id, input.field.clone()))
                .filter(|l| l.doc_id == input.doc_id)
                .ok_or_else(|| CoreError::new("collab.stale", "the document was closed; join again"))?;
            live.doc
                .transact_mut()
                .apply_update(update)
                .map_err(|e| CoreError::new("command.invalid_input", e.to_string()))?;
            live.used = Instant::now();
            live.text.get_string(&live.doc.transact())
        };
        ctx.events.publish(Event::new(
            "collab.update",
            CollabUpdateEvent {
                node_id: input.node_id,
                field: input.field.clone(),
                doc_id: input.doc_id,
                update: input.update,
                client_id: input.client_id,
            },
        ));
        store_text(ctx, input.node_id, &input.field, text).await
    }
);

/// Writes the document's text into the node (field-level, no event: the
/// editors already have it).
async fn store_text(ctx: &AppContext, node_id: Id, field: &str, text: String) -> CoreResult<Empty> {
    let repo = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = repo.get_node(node_id).await?;
    if field_text(&node, field)? == text {
        return Ok(Empty {});
    }
    let mut data =
        serde_json::to_value(&node.data).map_err(|e| CoreError::internal(e.to_string()))?;
    let top = field.split('.').next().unwrap_or(field);
    if !path_set(&mut data, field, text) {
        return Err(field_error(field));
    }
    let mut fields = serde_json::Map::new();
    fields.insert(top.to_string(), data[top].take());
    // No author: a text merged from several people overwrites nobody.
    repo.patch_node(node_id, None, node.meta.version, None, fields)
        .await?;
    Ok(Empty {})
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollabAwarenessInput {
    pub node_id: Id,
    pub field: String,
    pub update: String,
    pub client_id: u64,
}

command!(
    /// Relays cursors and selections to the other editors.
    CollabAwareness,
    "collab.awareness",
    CollabAwarenessInput,
    Empty,
    |ctx, input| {
        if !FIELDS.contains(&input.field.as_str()) {
            return Err(field_error(&input.field));
        }
        ctx.events.publish(Event::new(
            "collab.awareness",
            CollabAwarenessEvent {
                node_id: input.node_id,
                field: input.field,
                update: input.update,
                client_id: input.client_id,
            },
        ));
        Ok(Empty {})
    }
);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn apply(doc: &Doc, update: &[u8]) {
        doc.transact_mut()
            .apply_update(Update::decode_v1(update).unwrap())
            .unwrap();
    }

    #[test]
    fn edits_from_both_sides_converge() {
        let (server, text) = new_doc("https://api/users");
        // A client starts from the server's state.
        let client = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let ctext = client.get_or_insert_text("t");
        apply(
            &client,
            &server
                .transact()
                .encode_state_as_update_v1(&StateVector::default()),
        );
        // The client appends while the server replaces "api" with "v2".
        let before = client.transact().state_vector();
        ctext.insert(&mut client.transact_mut(), 17, "/1");
        let from_client = client.transact().encode_state_as_update_v1(&before);
        let from_server = replace_text(&server, &text, "https://v2/users").unwrap();
        apply(&server, &from_client);
        apply(&client, &from_server);
        assert_eq!(text.get_string(&server.transact()), "https://v2/users/1");
        assert_eq!(ctext.get_string(&client.transact()), "https://v2/users/1");
        assert!(replace_text(&server, &text, "https://v2/users/1").is_none());
    }

    #[test]
    fn replaces_with_utf16_offsets() {
        let (doc, text) = new_doc("😀 a");
        replace_text(&doc, &text, "😀 b").unwrap();
        assert_eq!(text.get_string(&doc.transact()), "😀 b");
    }

    #[test]
    fn paths_into_node_data() {
        let mut data = json!({ "url": "u", "body": { "text": "t", "graphql": { "query": "q" } } });
        assert_eq!(path_get(&data, "body.graphql.query"), Some(&json!("q")));
        assert!(path_set(&mut data, "body.text", "new".into()));
        assert_eq!(data["body"]["text"], "new");
        assert!(!path_set(&mut data, "missing.text", "x".into()));
    }
}
