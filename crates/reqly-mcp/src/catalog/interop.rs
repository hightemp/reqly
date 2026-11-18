//! Import and export tools.

use reqly_core::CoreResult;
use serde::Deserialize;
use serde_json::{Value, json};

use super::read::MAX_BODY_BYTES;
use super::{args, invalid, tool};
use crate::{Annotations, Tool, ToolContext};

/// The source name of pasted text: cURL commands are not wrapped in a folder.
const PASTED: &str = "clipboard";

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "import_text",
            "Import requests from text: cURL commands, .http files (JetBrains, VS Code REST \
             Client), raw HTTP, OpenAPI, Postman, Insomnia, HAR, Bruno or Reqly exports. The \
             format is detected. With collectionId (and folderId) the requests are added \
             there; with workspaceId a new collection is created.",
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "collectionId": { "type": "string" },
                    "folderId": { "type": "string", "description": "Folder of collectionId to import into." },
                    "workspaceId": { "type": "string", "description": "Create new collections in this workspace." },
                    "format": { "type": "string", "enum": ["reqly", "insomnia", "postman", "curl", "har", "openApi", "bruno", "http"] }
                },
                "required": ["text"]
            }),
            Annotations::default(),
            |cx, a| Box::pin(async move { import_text(cx, args(a)?).await }),
        ),
        tool(
            "export_collection",
            "Export a collection (or some of its requests) as Reqly JSON/YAML, Insomnia, \
             Postman, HAR or cURL. Secret values are not exported.",
            json!({
                "type": "object",
                "properties": {
                    "collectionId": { "type": "string" },
                    "requestIds": { "type": "array", "items": { "type": "string" }, "description": "Only these requests or folders." },
                    "format": {
                        "type": "string",
                        "enum": ["reqlyJson", "reqlyYaml", "insomnia4", "insomnia5", "postman", "har", "curl"],
                        "default": "reqlyJson"
                    }
                },
                "required": ["collectionId"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { export_collection(cx, args(a)?).await }),
        ),
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportArgs {
    text: String,
    collection_id: Option<String>,
    folder_id: Option<String>,
    workspace_id: Option<String>,
    format: Option<String>,
}

async fn import_text(cx: ToolContext, a: ImportArgs) -> CoreResult<Value> {
    let (workspace, target) = match (a.collection_id, a.workspace_id) {
        (Some(col), _) => {
            let collection = cx.call("collection.get", json!({ "id": col })).await?;
            let mut target =
                json!({ "kind": "collection", "collectionId": col, "strategy": "append" });
            if let Some(folder) = a.folder_id {
                target["folderId"] = json!(folder);
            }
            (collection["workspaceId"].clone(), target)
        }
        (None, Some(ws)) => (json!(ws), json!({ "kind": "newCollections" })),
        (None, None) => return Err(invalid("give collectionId or workspaceId")),
    };
    let mut input = json!({
        "sources": [{ "name": PASTED, "content": a.text }],
        "workspaceId": workspace,
        "target": target,
        "environments": true,
        "cookies": false,
    });
    if let Some(format) = a.format {
        input["format"] = json!(format);
    }
    let out = cx.call("import.apply", input).await?;
    Ok(json!({
        "collectionIds": out["collectionIds"],
        "requests": out["requests"],
        "environments": out["environments"],
        "format": out["report"]["format"],
        "notes": out["report"]["entries"],
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    collection_id: String,
    request_ids: Option<Vec<String>>,
    format: Option<String>,
}

async fn export_collection(cx: ToolContext, a: ExportArgs) -> CoreResult<Value> {
    let scope = match a.request_ids {
        Some(ids) => json!({ "kind": "nodes", "nodeIds": ids }),
        None => json!({ "kind": "collection", "collectionId": a.collection_id }),
    };
    let out = cx
        .call(
            "export.run",
            json!({
                "scope": scope,
                "format": a.format.as_deref().unwrap_or("reqlyJson"),
                "options": { "environments": true, "secrets": false, "cookies": false, "history": false },
            }),
        )
        .await?;
    let files: Vec<Value> = out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let content = f["content"].as_str().unwrap_or("");
            let truncated = content.len() as u64 > MAX_BODY_BYTES;
            let content: String = if truncated {
                content
                    .char_indices()
                    .take_while(|(i, _)| (*i as u64) < MAX_BODY_BYTES)
                    .map(|(_, c)| c)
                    .collect()
            } else {
                content.to_string()
            };
            json!({ "name": f["name"], "mime": f["mime"], "content": content, "truncated": truncated })
        })
        .collect();
    Ok(json!({ "files": files }))
}
