//! Environment tools. Secret values are never read back (masked), but can
//! be written; writing the mask keeps the stored value.

use reqly_core::CoreResult;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{args, invalid, mask, tool};
use crate::{Annotations, Tool, ToolContext};

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "get_variables",
            "The variables of an environment. Secret values are shown as ••••; the secrets \
             list names them.",
            json!({
                "type": "object",
                "properties": { "environmentId": { "type": "string" } },
                "required": ["environmentId"]
            }),
            Annotations::read_only(),
            |cx, a| Box::pin(async move { get_variables(cx, args(a)?).await }),
        ),
        tool(
            "set_variables",
            "Set, add or remove variables of an environment. Values may be strings, numbers, \
             booleans, arrays or objects. Names in secret are stored as secrets (masked, \
             encrypted, not exported). A value of •••• keeps the stored one.",
            json!({
                "type": "object",
                "properties": {
                    "environmentId": { "type": "string" },
                    "values": { "type": "object", "description": "Variables to set: name → value." },
                    "secret": { "type": "array", "items": { "type": "string" }, "description": "Names to mark as secret." },
                    "remove": { "type": "array", "items": { "type": "string" }, "description": "Names to delete." }
                },
                "required": ["environmentId"]
            }),
            Annotations::default().idempotent(),
            |cx, a| Box::pin(async move { set_variables(cx, args(a)?).await }),
        ),
        tool(
            "create_environment",
            "Create an environment: of a collection (collectionId) or a global one shared by \
             the workspace (workspaceId).",
            json!({
                "type": "object",
                "properties": {
                    "collectionId": { "type": "string" },
                    "workspaceId": { "type": "string", "description": "For a global environment." },
                    "name": { "type": "string" },
                    "variables": { "type": "object" }
                },
                "required": ["name"]
            }),
            Annotations::default(),
            |cx, a| Box::pin(async move { create_environment(cx, args(a)?).await }),
        ),
        tool(
            "set_active_environment",
            "Make an environment the active one (of its collection, or the active global \
             one). environmentId null with collectionId (or workspaceId for global) \
             deactivates.",
            json!({
                "type": "object",
                "properties": {
                    "environmentId": { "type": ["string", "null"] },
                    "collectionId": { "type": "string" },
                    "workspaceId": { "type": "string" }
                },
                "required": ["environmentId"]
            }),
            Annotations::default().idempotent(),
            |cx, a| Box::pin(async move { set_active(cx, args(a)?).await }),
        ),
    ]
}

fn view(env: &Value) -> Value {
    let secrets: Vec<String> = env["secrets"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": env["id"],
        "name": env["name"],
        "kind": env["kind"],
        "collectionId": env["collectionId"],
        "workspaceId": env["workspaceId"],
        "variables": mask::variables(&env["data"], &secrets),
        "secrets": secrets,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvArg {
    environment_id: String,
}

async fn get_variables(cx: ToolContext, a: EnvArg) -> CoreResult<Value> {
    let env = cx
        .call("environment.get", json!({ "id": a.environment_id }))
        .await?;
    Ok(view(&env))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetArgs {
    environment_id: String,
    #[serde(default)]
    values: Map<String, Value>,
    #[serde(default)]
    secret: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

async fn set_variables(cx: ToolContext, a: SetArgs) -> CoreResult<Value> {
    let env = cx
        .call("environment.get", json!({ "id": a.environment_id }))
        .await?;
    let mut data = env["data"].as_object().cloned().unwrap_or_default();
    for (k, v) in a.values {
        if v.as_str() != Some(mask::HIDDEN) {
            data.insert(k, v);
        }
    }
    let mut secrets: Vec<String> = env["secrets"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for name in a.secret {
        if !secrets.contains(&name) {
            secrets.push(name);
        }
    }
    for name in &a.remove {
        data.remove(name);
        secrets.retain(|s| s != name);
    }
    let env = cx
        .call(
            "environment.update",
            json!({ "id": a.environment_id, "data": data, "secrets": secrets }),
        )
        .await?;
    Ok(view(&env))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    collection_id: Option<String>,
    workspace_id: Option<String>,
    name: String,
    #[serde(default)]
    variables: Map<String, Value>,
}

async fn create_environment(cx: ToolContext, a: CreateArgs) -> CoreResult<Value> {
    let (workspace, kind) = match (&a.collection_id, a.workspace_id) {
        (Some(col), _) => {
            let col = cx.call("collection.get", json!({ "id": col })).await?;
            (col["workspaceId"].clone(), "sub")
        }
        (None, Some(ws)) => (json!(ws), "global"),
        (None, None) => return Err(invalid("give collectionId or workspaceId")),
    };
    let env = cx
        .call(
            "environment.create",
            json!({
                "workspaceId": workspace,
                "collectionId": a.collection_id,
                "kind": kind,
                "name": a.name,
                "data": a.variables,
            }),
        )
        .await?;
    Ok(view(&env))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveArgs {
    environment_id: Option<String>,
    collection_id: Option<String>,
    workspace_id: Option<String>,
}

async fn set_active(cx: ToolContext, a: ActiveArgs) -> CoreResult<Value> {
    match a.environment_id {
        Some(id) => {
            let env = cx.call("environment.get", json!({ "id": id })).await?;
            if env["collectionId"].is_null() {
                cx.call(
                    "environment.setActiveGlobal",
                    json!({ "workspaceId": env["workspaceId"], "environmentId": id }),
                )
                .await?;
            } else {
                cx.call(
                    "environment.setActive",
                    json!({ "collectionId": env["collectionId"], "environmentId": id }),
                )
                .await?;
            }
            Ok(json!({ "active": view(&env)["name"], "environmentId": id }))
        }
        None => {
            if let Some(col) = a.collection_id {
                cx.call(
                    "environment.setActive",
                    json!({ "collectionId": col, "environmentId": null }),
                )
                .await?;
            } else if let Some(ws) = a.workspace_id {
                cx.call(
                    "environment.setActiveGlobal",
                    json!({ "workspaceId": ws, "environmentId": null }),
                )
                .await?;
            } else {
                return Err(invalid("give collectionId or workspaceId to deactivate"));
            }
            Ok(json!({ "active": null }))
        }
    }
}
