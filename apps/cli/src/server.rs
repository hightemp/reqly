//! Exports a collection from a Reqly server (`--server`).

use serde_json::{Value, json};

/// Calls a server command, as the web UI does.
async fn call(
    server: &str,
    token: Option<&str>,
    name: &str,
    input: Value,
) -> Result<Value, String> {
    let url = format!("{}/api/v1/cmd/{name}", server.trim_end_matches('/'));
    let token = token.map(str::to_string);
    let body = input.to_string();
    tokio::task::spawn_blocking(move || post(&url, token.as_deref(), &body))
        .await
        .map_err(|e| e.to_string())?
}

fn post(url: &str, token: Option<&str>, body: &str) -> Result<Value, String> {
    let mut easy = curl::easy::Easy::new();
    let mut headers = curl::easy::List::new();
    let fail = |e: curl::Error| format!("{url}: {e}");
    headers
        .append("Content-Type: application/json")
        .map_err(fail)?;
    if let Some(token) = token {
        headers
            .append(&format!("Authorization: Bearer {token}"))
            .map_err(fail)?;
    }
    easy.url(url).map_err(fail)?;
    easy.post(true).map_err(fail)?;
    easy.post_fields_copy(body.as_bytes()).map_err(fail)?;
    easy.http_headers(headers).map_err(fail)?;
    let mut response = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer
            .write_function(|data| {
                response.extend_from_slice(data);
                Ok(data.len())
            })
            .map_err(fail)?;
        transfer.perform().map_err(fail)?;
    }
    let status = easy.response_code().map_err(fail)?;
    let value: Value = serde_json::from_slice(&response).unwrap_or(Value::Null);
    match status {
        200..=299 => Ok(value),
        401 => Err("the server refused the token (use --token or REQLY_TOKEN)".into()),
        _ => Err(match (value["code"].as_str(), value["message"].as_str()) {
            (Some(code), Some(message)) => format!("server: {code}: {message}"),
            _ => format!("server answered {status}"),
        }),
    }
}

/// The collection named or with the id `wanted`, as a Reqly export (JSON).
pub async fn export_collection(
    server: &str,
    token: Option<&str>,
    wanted: &str,
) -> Result<(String, String), String> {
    let workspaces = call(server, token, "workspace.list", json!({})).await?;
    let mut found = None;
    let mut names = Vec::new();
    for ws in workspaces.as_array().into_iter().flatten() {
        let collections = call(
            server,
            token,
            "collection.list",
            json!({ "workspaceId": ws["id"] }),
        )
        .await?;
        for c in collections.as_array().into_iter().flatten() {
            let name = c["name"].as_str().unwrap_or_default();
            names.push(name.to_string());
            if found.is_none() && (c["id"] == wanted || name == wanted) {
                found = Some((c["id"].clone(), name.to_string()));
            }
        }
    }
    let Some((id, name)) = found else {
        return Err(format!(
            "no collection `{wanted}` on the server (there are: {})",
            names.join(", ")
        ));
    };
    let export = |secrets: bool| {
        call(
            server,
            token,
            "export.run",
            json!({
                "scope": { "kind": "collection", "collectionId": id },
                "format": "reqlyJson",
                "options": { "environments": true, "secrets": secrets },
            }),
        )
    };
    // Viewers may not read secrets: export without them then.
    let out = match export(true).await {
        Ok(out) => out,
        Err(_) => export(false).await?,
    };
    let content = out["files"][0]["content"]
        .as_str()
        .ok_or("the server returned an empty export")?
        .to_string();
    Ok((format!("{name}.reqly.json"), content))
}
