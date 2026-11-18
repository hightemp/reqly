//! Read tools: workspaces, collections, the tree, requests, search,
//! environments and the response history.

use std::collections::HashMap;

use base64::Engine;
use reqly_core::CoreResult;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{args, invalid, mask, tool};
use crate::{Annotations, Tool, ToolContext};

pub(super) const DEFAULT_BODY_BYTES: u64 = 16 * 1024;
pub const MAX_BODY_BYTES: u64 = 1024 * 1024;

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "list_workspaces",
            "List the workspaces (top-level containers of collections) with their ids.",
            json!({ "type": "object", "properties": {} }),
            Annotations::read_only(),
            |cx, _| Box::pin(async move { list_workspaces(cx).await }),
        ),
        tool(
            "list_collections",
            "List collections with their ids, in one workspace or in all of them.",
            json!({
                "type": "object",
                "properties": {
                    "workspaceId": { "type": "string", "description": "Only this workspace (all when omitted)." }
                }
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { list_collections(cx, args(a)?).await }),
        ),
        tool(
            "get_tree",
            "The folders and requests of a collection in display order. Each item has its id, \
             parentId, kind (folder or request), name, path (Folder/Sub/Name), depth and, for \
             requests, method, url and protocol.",
            json!({
                "type": "object",
                "properties": { "collectionId": { "type": "string" } },
                "required": ["collectionId"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { get_tree(cx, args(a)?).await }),
        ),
        tool(
            "get_request",
            "A request or folder with all its data: method, URL, params, headers, body, auth, \
             settings, scripts, assertions. Credentials are masked as •••• unless they are \
             template references such as {{token}}.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Request or folder id." } },
                "required": ["id"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { get_request(cx, args(a)?).await }),
        ),
        tool(
            "search_requests",
            "Full-text search of requests and folders by name, URL, method and description.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "workspaceId": { "type": "string", "description": "Only this workspace (all when omitted)." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 20 }
                },
                "required": ["query"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { search_requests(cx, args(a)?).await }),
        ),
        tool(
            "list_environments",
            "The environments a collection can use (its own and the global ones) with variable \
             names and which ones are active. Read values with get_variables.",
            json!({
                "type": "object",
                "properties": { "collectionId": { "type": "string" } },
                "required": ["collectionId"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { list_environments(cx, args(a)?).await }),
        ),
        tool(
            "get_history",
            "Recent responses, newest first: of one request (requestId) or of a whole workspace \
             (workspaceId). Each has the response id for get_response.",
            json!({
                "type": "object",
                "properties": {
                    "requestId": { "type": "string" },
                    "workspaceId": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 20 }
                }
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { get_history(cx, args(a)?).await }),
        ),
        tool(
            "get_response",
            "A stored response: status, headers, timing, body, test and assertion results. \
             Text bodies are returned as text, binary ones as base64; long bodies are cut to \
             maxBodyBytes (bodyTruncated tells).",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Response id from get_history or send_request." },
                    "maxBodyBytes": { "type": "integer", "minimum": 0, "maximum": MAX_BODY_BYTES, "default": DEFAULT_BODY_BYTES }
                },
                "required": ["id"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { get_response(cx, args(a)?).await }),
        ),
    ]
}

async fn list_workspaces(cx: ToolContext) -> CoreResult<Value> {
    let list = cx.call("workspace.list", json!({})).await?;
    let workspaces: Vec<_> = items(&list)
        .iter()
        .map(|w| json!({ "id": w["id"], "name": w["name"] }))
        .collect();
    Ok(json!({ "workspaces": workspaces }))
}

fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

async fn workspace_ids(cx: &ToolContext, only: Option<String>) -> CoreResult<Vec<Value>> {
    if let Some(id) = only {
        return Ok(vec![Value::String(id)]);
    }
    let list = cx.call("workspace.list", json!({})).await?;
    Ok(items(&list).iter().map(|w| w["id"].clone()).collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceArg {
    workspace_id: Option<String>,
}

async fn list_collections(cx: ToolContext, a: WorkspaceArg) -> CoreResult<Value> {
    let mut out = Vec::new();
    for ws in workspace_ids(&cx, a.workspace_id).await? {
        let list = cx
            .call("collection.list", json!({ "workspaceId": ws }))
            .await?;
        out.extend(
            items(&list).iter().map(
                |c| json!({ "id": c["id"], "name": c["name"], "workspaceId": c["workspaceId"] }),
            ),
        );
    }
    Ok(json!({ "collections": out }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionArg {
    collection_id: String,
}

/// Tree rows in display order: (node summary, depth, path).
fn ordered(nodes: &[Value]) -> Vec<(&Value, usize, String)> {
    let mut children: HashMap<Option<&str>, Vec<&Value>> = HashMap::new();
    for n in nodes {
        children.entry(n["parentId"].as_str()).or_default().push(n);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| a["sortKey"].as_str().cmp(&b["sortKey"].as_str()));
    }
    let mut out = Vec::new();
    let mut stack: Vec<(&Value, usize, String)> = children
        .get(&None)
        .map(|roots| {
            roots
                .iter()
                .rev()
                .map(|n| (*n, 0, name(n).to_string()))
                .collect()
        })
        .unwrap_or_default();
    while let Some((node, depth, path)) = stack.pop() {
        if let Some(kids) = children.get(&node["id"].as_str()) {
            for kid in kids.iter().rev() {
                stack.push((kid, depth + 1, format!("{path}/{}", name(kid))));
            }
        }
        out.push((node, depth, path));
    }
    out
}

fn name(n: &Value) -> &str {
    n["name"].as_str().unwrap_or("")
}

async fn get_tree(cx: ToolContext, a: CollectionArg) -> CoreResult<Value> {
    let collection = cx
        .call("collection.get", json!({ "id": a.collection_id }))
        .await?;
    let nodes = cx
        .call("tree.list", json!({ "collectionId": a.collection_id }))
        .await?;
    let rows: Vec<_> = ordered(items(&nodes))
        .into_iter()
        .map(|(n, depth, path)| {
            let mut row = json!({
                "id": n["id"], "parentId": n["parentId"], "kind": n["kind"],
                "name": n["name"], "path": path, "depth": depth,
            });
            if n["kind"] == "request" {
                row["method"] = n["method"].clone();
                row["url"] = n["url"].clone();
                row["protocol"] = n["protocol"].clone();
            }
            row
        })
        .collect();
    Ok(json!({
        "collection": { "id": collection["id"], "name": collection["name"], "workspaceId": collection["workspaceId"] },
        "items": rows,
    }))
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

/// `Folder/Sub/Name` of a node.
pub(super) async fn path_of(cx: &ToolContext, node: &Value) -> CoreResult<String> {
    let nodes = cx
        .call("tree.list", json!({ "collectionId": node["collectionId"] }))
        .await?;
    Ok(ordered(items(&nodes))
        .into_iter()
        .find(|(n, _, _)| n["id"] == node["id"])
        .map(|(_, _, p)| p)
        .unwrap_or_else(|| name(node).to_string()))
}

/// What the request tools return for a node.
pub(super) async fn describe_node(cx: &ToolContext, node: &Value) -> CoreResult<Value> {
    Ok(json!({
        "id": node["id"],
        "name": node["name"],
        "path": path_of(cx, node).await?,
        "collectionId": node["collectionId"],
        "parentId": node["parentId"],
        "version": node["version"],
        "data": mask::node_data(&node["data"]),
    }))
}

async fn get_request(cx: ToolContext, a: IdArg) -> CoreResult<Value> {
    let node = cx.call("node.get", json!({ "id": a.id })).await?;
    describe_node(&cx, &node).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    query: String,
    workspace_id: Option<String>,
    limit: Option<u32>,
}

async fn search_requests(cx: ToolContext, a: SearchArgs) -> CoreResult<Value> {
    let limit = a.limit.unwrap_or(20).clamp(1, 100) as usize;
    let mut out = Vec::new();
    for ws in workspace_ids(&cx, a.workspace_id).await? {
        let hits = cx
            .call(
                "search.nodes",
                json!({ "workspaceId": ws, "query": a.query, "limit": limit }),
            )
            .await?;
        out.extend(items(&hits).iter().map(|h| {
            json!({
                "id": h["id"], "name": h["name"], "kind": h["kind"], "method": h["method"],
                "url": h["url"], "protocol": h["protocol"], "collectionId": h["collectionId"],
                "parentId": h["parentId"],
            })
        }));
    }
    out.truncate(limit);
    Ok(json!({ "results": out }))
}

async fn list_environments(cx: ToolContext, a: CollectionArg) -> CoreResult<Value> {
    let list = cx
        .call(
            "environment.list",
            json!({ "collectionId": a.collection_id }),
        )
        .await?;
    let envs: Vec<_> = items(&list["collection"])
        .iter()
        .chain(items(&list["global"]))
        .map(|e| {
            let names: Vec<_> = e["data"]
                .as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            json!({
                "id": e["id"], "name": e["name"], "kind": e["kind"],
                "collectionId": e["collectionId"], "variables": names,
            })
        })
        .collect();
    Ok(json!({
        "environments": envs,
        "activeId": list["activeId"],
        "activeGlobalId": list["activeGlobalId"],
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryArgs {
    request_id: Option<String>,
    workspace_id: Option<String>,
    limit: Option<u32>,
}

async fn get_history(cx: ToolContext, a: HistoryArgs) -> CoreResult<Value> {
    let limit = a.limit.unwrap_or(20).clamp(1, 100);
    let list = match (a.request_id, a.workspace_id) {
        (Some(id), _) => {
            cx.call("response.list", json!({ "requestId": id, "limit": limit }))
                .await?
        }
        (None, Some(ws)) => {
            cx.call(
                "history.workspace",
                json!({ "workspaceId": ws, "limit": limit }),
            )
            .await?
        }
        (None, None) => return Err(invalid("give requestId or workspaceId")),
    };
    Ok(json!({ "responses": list }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseArgs {
    id: String,
    max_body_bytes: Option<u64>,
}

async fn get_response(cx: ToolContext, a: ResponseArgs) -> CoreResult<Value> {
    let record = cx.call("response.get", json!({ "id": a.id })).await?;
    let node = cx
        .call("node.get", json!({ "id": record["requestId"] }))
        .await?;
    let scrub = mask::Scrub::for_request(&cx, &node).await?;
    response_view(&cx, &record, body_limit(a.max_body_bytes), &scrub).await
}

/// `maxBodyBytes` with its default and ceiling.
pub(super) fn body_limit(asked: Option<u64>) -> u64 {
    asked.unwrap_or(DEFAULT_BODY_BYTES).min(MAX_BODY_BYTES)
}

/// A response record for an agent, with the body cut to `max` bytes.
pub(super) async fn response_view(
    cx: &ToolContext,
    record: &Value,
    max: u64,
    scrub: &mask::Scrub,
) -> CoreResult<Value> {
    let size = record["bodySize"].as_u64().unwrap_or(0);
    let (body, encoding, truncated) = if record["bodyHash"].is_null() || size == 0 {
        (Value::Null, "none", false)
    } else {
        let chunk = cx
            .call(
                "response.body",
                json!({ "id": record["id"], "offset": 0, "limit": max }),
            )
            .await?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk["base64"].as_str().unwrap_or(""))
            .unwrap_or_default();
        let truncated = (bytes.len() as u64) < size;
        match text_of(&bytes, truncated) {
            Some(text) => (Value::String(text), "text", truncated),
            None => (
                Value::String(base64::engine::general_purpose::STANDARD.encode(&bytes)),
                "base64",
                truncated,
            ),
        }
    };
    let mut view = json!({
        "id": record["id"],
        "requestId": record["requestId"],
        "createdAt": record["createdAt"],
        "status": record["status"],
        "statusText": record["statusText"],
        "httpVersion": record["httpVersion"],
        "headers": record["headers"],
        "contentType": record["contentType"],
        "totalMs": record["timings"]["totalMs"],
        "bodySize": size,
        "body": body,
        "bodyEncoding": encoding,
        "bodyTruncated": truncated,
        "error": record["error"],
        "tests": record["tests"],
        "assertions": record["assertions"],
        "request": {
            "method": record["requestSnapshot"]["method"],
            "url": record["requestSnapshot"]["url"],
        },
        "finalUrl": record["finalUrl"],
    });
    scrub.apply(&mut view);
    Ok(view)
}

/// UTF-8 text, allowing a character cut in half at the end of a truncated body.
fn text_of(bytes: &[u8], truncated: bool) -> Option<String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Some(s.to_string()),
        Err(e) if truncated && e.error_len().is_none() => {
            Some(String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned())
        }
        Err(_) => None,
    }
}
