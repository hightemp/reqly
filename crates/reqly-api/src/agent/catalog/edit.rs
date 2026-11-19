//! Edit tools: create, change, rename, move, duplicate and delete requests
//! and folders. Changes go through the same commands as the editor, so open
//! editors merge them and deletions land in the trash.

use reqly_core::CoreResult;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use super::read::describe_node;
use super::{args, invalid, mask, tool};
use crate::agent::{Annotations, Tool, ToolContext};

/// Top-level request fields an agent may set.
const REQUEST_FIELDS: &[&str] = &[
    "method",
    "url",
    "params",
    "headers",
    "body",
    "auth",
    "description",
    "scripts",
    "assertions",
    "settings",
    "variables",
];

fn request_properties() -> Value {
    json!({
        "method": { "type": "string", "description": "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS or any other." },
        "url": { "type": "string", "description": "May use {{variables}}." },
        "params": rows("Query parameters."),
        "headers": rows("Headers."),
        "body": {
            "type": "object",
            "description": "kind: none, json, xml, yaml, text, html (with text), formUrlencoded or multipart (with fields: rows), graphql (with graphql: {query, variables}).",
            "properties": {
                "kind": { "type": "string", "enum": ["none", "json", "xml", "yaml", "text", "html", "formUrlencoded", "multipart", "binary", "graphql"] },
                "text": { "type": "string" },
                "fields": rows("Form fields.")
            }
        },
        "auth": {
            "type": "object",
            "description": "type: inherit, none, basic {username, password}, bearer {token, prefix}, apiKey {key, value, location: header|query}, digest, ntlm, oauth1, oauth2, awsSigV4, hawk, asap, netrc, edgeGrid, jwt. Values written as •••• keep the stored secret.",
            "properties": { "type": { "type": "string" } },
            "required": ["type"]
        },
        "description": { "type": "string", "description": "Markdown." },
        "scripts": {
            "type": "object",
            "properties": {
                "preRequest": { "type": "string" },
                "afterResponse": { "type": "string" }
            }
        },
        "assertions": { "type": "array", "items": { "type": "object" } },
        "settings": { "type": "object", "description": "Request settings (timeouts, redirects, TLS…)." },
        "variables": { "type": "object", "description": "Request-level variables." }
    })
}

fn rows(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": {
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "value": { "type": "string" },
                "enabled": { "type": "boolean", "default": true },
                "description": { "type": "string" }
            },
            "required": ["name"]
        }
    })
}

fn with_properties(mut extra: Value, required: &[&str]) -> Value {
    let mut props = request_properties();
    if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object_mut()) {
        p.append(e);
    }
    json!({ "type": "object", "properties": props, "required": required })
}

fn position_schema() -> Value {
    json!({ "type": "string", "enum": ["start", "end"], "default": "end" })
}

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "create_request",
            "Create an HTTP request in a collection, at the root or in a folder. Only name and \
             collectionId are required; the method defaults to GET.",
            with_properties(
                json!({
                    "collectionId": { "type": "string" },
                    "parentId": { "type": "string", "description": "Folder id (root when omitted)." },
                    "name": { "type": "string" },
                    "position": position_schema()
                }),
                &["collectionId", "name"],
            ),
            Annotations::default(),
            |cx, a| Box::pin(async move { create_request(cx, a).await }),
        ),
        tool(
            "update_request",
            "Change a request: only the given fields change (the rest stays as it is), like an \
             edit in the app; open editors merge the change. Lists (headers, params…) replace \
             the whole list.",
            with_properties(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" }
                }),
                &["id"],
            ),
            Annotations::default().idempotent(),
            |cx, a| Box::pin(async move { update_request(cx, a).await }),
        ),
        tool(
            "create_folder",
            "Create a folder in a collection, at the root or inside another folder.",
            json!({
                "type": "object",
                "properties": {
                    "collectionId": { "type": "string" },
                    "parentId": { "type": "string", "description": "Parent folder id (root when omitted)." },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "position": position_schema()
                },
                "required": ["collectionId", "name"]
            }),
            Annotations::default(),
            |cx, a| Box::pin(async move { create_folder(cx, args(a)?).await }),
        ),
        tool(
            "rename_item",
            "Rename a request or folder.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" }, "name": { "type": "string" } },
                "required": ["id", "name"]
            }),
            Annotations::default().idempotent(),
            |cx, a| Box::pin(async move { rename_item(cx, args(a)?).await }),
        ),
        tool(
            "move_item",
            "Move a request or folder into another folder (parentId), to the collection root \
             (parentId: null) or to another collection, at the start or the end.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "parentId": { "type": ["string", "null"], "description": "Target folder; null for the root; omitted keeps the current one." },
                    "collectionId": { "type": "string", "description": "Target collection (the current one when omitted)." },
                    "position": position_schema()
                },
                "required": ["id"]
            }),
            Annotations::default().idempotent(),
            |cx, a| Box::pin(async move { move_item(cx, args(a)?).await }),
        ),
        tool(
            "duplicate_item",
            "Copy a request or folder (with everything inside) next to the original.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
            Annotations::default(),
            |cx, a| Box::pin(async move { duplicate_item(cx, args(a)?).await }),
        ),
        tool(
            "delete_item",
            "Delete requests or folders (with everything inside). They go to the trash of the \
             collection and can be restored in the app.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "ids": { "type": "array", "items": { "type": "string" } }
                }
            }),
            Annotations::destructive().idempotent(),
            |cx, a| Box::pin(async move { delete_item(cx, args(a)?).await }),
        ),
    ]
}

/// Fields of the given request data, auth defaults filled in.
/// Auth types, as models must name them in `auth.type`.
const AUTH_TYPES: &str = "inherit, none, basic, bearer, apiKey, digest, ntlm, oAuth1, oAuth2, \
                          awsSigV4, hawk, asap, netrc, edgeGrid, jwt";

/// The request fields of the arguments. Models tend to send every field of
/// the schema, so `null` and `{}` mean "leave the default".
fn request_fields(a: &Map<String, Value>) -> CoreResult<Map<String, Value>> {
    let mut fields: Map<String, Value> = REQUEST_FIELDS
        .iter()
        .filter_map(|k| a.get(*k).map(|v| (k.to_string(), v.clone())))
        .filter(|(_, v)| !v.is_null() && v.as_object().is_none_or(|o| !o.is_empty()))
        .collect();
    if let Some(auth) = fields.get_mut("auth") {
        if auth.get("type").and_then(Value::as_str).is_none() {
            return Err(invalid(&format!(
                "auth.type is required, one of: {AUTH_TYPES}"
            )));
        }
        complete_auth(auth);
    }
    Ok(fields)
}

/// Defaults of the simple auth types, so agents may leave fields out.
fn complete_auth(auth: &mut Value) {
    let Value::Object(map) = auth else { return };
    let defaults: &[(&str, &str)] = match map.get("type").and_then(Value::as_str) {
        Some("basic" | "digest") => &[("username", ""), ("password", "")],
        Some("ntlm") => &[("username", ""), ("password", ""), ("domain", "")],
        Some("bearer") => &[("token", ""), ("prefix", "Bearer")],
        Some("apiKey") => &[("key", ""), ("value", ""), ("location", "header")],
        Some("netrc") => &[("file", "")],
        _ => &[],
    };
    for (k, v) in defaults {
        map.entry(k.to_string())
            .or_insert_with(|| Value::String(v.to_string()));
    }
}

fn object(a: Value) -> CoreResult<Map<String, Value>> {
    match a {
        Value::Object(m) => Ok(m),
        _ => Err(invalid("arguments must be an object")),
    }
}

fn string<'a>(a: &'a Map<String, Value>, key: &str) -> CoreResult<&'a str> {
    a.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("missing {key}")))
}

fn placement(position: Option<&str>) -> Value {
    match position {
        Some("start") => json!({ "type": "start" }),
        _ => json!({ "type": "end" }),
    }
}

async fn create_request(cx: ToolContext<'_>, a: Value) -> CoreResult<Value> {
    let a = object(a)?;
    let mut data = request_fields(&a)?;
    data.insert("kind".into(), json!("request"));
    data.entry("method").or_insert_with(|| json!("GET"));
    data.entry("url").or_insert_with(|| json!(""));
    let node = cx
        .call(
            "node.create",
            json!({
                "collectionId": string(&a, "collectionId")?,
                "parentId": a.get("parentId").cloned().unwrap_or(Value::Null),
                "name": string(&a, "name")?,
                "data": data,
                "placement": placement(a.get("position").and_then(Value::as_str)),
            }),
        )
        .await?;
    describe_node(&cx, &node).await
}

async fn update_request(cx: ToolContext<'_>, a: Value) -> CoreResult<Value> {
    let a = object(a)?;
    let id = string(&a, "id")?;
    let current = cx.call("node.get", json!({ "id": id })).await?;
    let mut fields = request_fields(&a)?;
    for (k, v) in fields.iter_mut() {
        restore_hidden(v, &current["data"][k.as_str()]);
    }
    let mut input = json!({ "id": id, "fields": fields });
    if let Some(name) = a.get("name") {
        input["name"] = name.clone();
    }
    let node = cx.call("node.update", input).await?;
    describe_node(&cx, &node).await
}

/// Puts the stored value back wherever the agent sent a masked one.
fn restore_hidden(new: &mut Value, current: &Value) {
    match new {
        Value::String(s) if s == mask::HIDDEN => {
            if let Some(stored) = current.as_str() {
                *s = stored.to_string();
            }
        }
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                restore_hidden(v, &current[k.as_str()]);
            }
        }
        Value::Array(items) => {
            let stored = current.as_array();
            for (i, item) in items.iter_mut().enumerate() {
                // Rows are matched by name: the agent may reorder them.
                let by_name = item["name"].as_str().and_then(|name| {
                    stored?.iter().find(|r| {
                        r["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    })
                });
                let other = by_name
                    .or_else(|| stored.and_then(|s| s.get(i)))
                    .cloned()
                    .unwrap_or(Value::Null);
                restore_hidden(item, &other);
            }
        }
        _ => {}
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderArgs {
    collection_id: String,
    parent_id: Option<String>,
    name: String,
    description: Option<String>,
    position: Option<String>,
}

async fn create_folder(cx: ToolContext<'_>, a: FolderArgs) -> CoreResult<Value> {
    let mut data = json!({ "kind": "folder" });
    if let Some(d) = a.description {
        data["description"] = json!(d);
    }
    let node = cx
        .call(
            "node.create",
            json!({
                "collectionId": a.collection_id,
                "parentId": a.parent_id,
                "name": a.name,
                "data": data,
                "placement": placement(a.position.as_deref()),
            }),
        )
        .await?;
    describe_node(&cx, &node).await
}

#[derive(Deserialize)]
struct RenameArgs {
    id: String,
    name: String,
}

async fn rename_item(cx: ToolContext<'_>, a: RenameArgs) -> CoreResult<Value> {
    let node = cx
        .call("node.update", json!({ "id": a.id, "name": a.name }))
        .await?;
    describe_node(&cx, &node).await
}

/// `Some(None)` for an explicit `null`, `None` when the key is missing.
fn double_option<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveArgs {
    id: String,
    #[serde(default, deserialize_with = "double_option")]
    parent_id: Option<Option<String>>,
    collection_id: Option<String>,
    position: Option<String>,
}

async fn move_item(cx: ToolContext<'_>, a: MoveArgs) -> CoreResult<Value> {
    let node = cx.call("node.get", json!({ "id": a.id })).await?;
    let parent = match a.parent_id {
        Some(p) => json!(p),
        None => node["parentId"].clone(),
    };
    let collection = a
        .collection_id
        .map(Value::String)
        .unwrap_or_else(|| node["collectionId"].clone());
    cx.call(
        "tree.move",
        json!({
            "ids": [a.id],
            "collectionId": collection,
            "parentId": parent,
            "placement": placement(a.position.as_deref()),
        }),
    )
    .await?;
    let moved = cx.call("node.get", json!({ "id": a.id })).await?;
    describe_node(&cx, &moved).await
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

async fn duplicate_item(cx: ToolContext<'_>, a: IdArg) -> CoreResult<Value> {
    let copies = cx.call("tree.duplicate", json!({ "ids": [a.id] })).await?;
    let copy = copies
        .as_array()
        .and_then(|c| c.first())
        .cloned()
        .ok_or_else(|| invalid("nothing was copied"))?;
    describe_node(&cx, &copy).await
}

#[derive(Deserialize)]
struct DeleteArgs {
    id: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
}

async fn delete_item(cx: ToolContext<'_>, a: DeleteArgs) -> CoreResult<Value> {
    let mut ids = a.ids;
    ids.extend(a.id);
    if ids.is_empty() {
        return Err(invalid("give id or ids"));
    }
    cx.call("tree.delete", json!({ "ids": ids })).await?;
    Ok(json!({
        "deleted": ids,
        "note": "Moved to the trash of the collection; the user can restore it in the app.",
    }))
}
