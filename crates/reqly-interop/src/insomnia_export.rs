//! Export to Insomnia v4 (JSON) and v5 (YAML).

use std::collections::HashMap;

use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind,
    RequestSettings, Scripts,
};
use reqly_core::{CoreError, CoreResult};
use reqly_template::ast::{Expr, Segment};
use serde_json::{Map, Value, json};

use crate::model::{ExportData, ImportedCollection, ImportedEnvironment, ImportedItem};

/// Source ids → Insomnia ids (`req_…`, `fld_…`).
struct Ids(HashMap<String, String>);

impl Ids {
    fn collect(items: &[ImportedItem], map: &mut HashMap<String, String>, counter: &mut usize) {
        for item in items {
            *counter += 1;
            let prefix = match item {
                ImportedItem::Folder { .. } => "fld_",
                ImportedItem::Request { .. } => "req_",
                ImportedItem::WebSocket { .. } => "ws-req_",
                ImportedItem::Grpc { .. } => "greq_",
            };
            if let Some(id) = item.id() {
                let new = if id.starts_with(prefix) {
                    id.to_string()
                } else {
                    format!("{prefix}{}", id.replace('-', ""))
                };
                map.insert(id.to_string(), new);
            }
            Self::collect(item.children(), map, counter);
        }
    }

    fn new(collections: &[ImportedCollection]) -> Self {
        let mut map = HashMap::new();
        let mut counter = 0;
        for c in collections {
            Self::collect(&c.items, &mut map, &mut counter);
        }
        Self(map)
    }

    fn id(&self, item: &ImportedItem, fallback: &mut usize) -> String {
        if let Some(id) = item.id().and_then(|id| self.0.get(id)) {
            return id.clone();
        }
        *fallback += 1;
        match item {
            ImportedItem::Folder { .. } => format!("fld_{fallback}"),
            ImportedItem::Request { .. } => format!("req_{fallback}"),
            ImportedItem::WebSocket { .. } => format!("ws-req_{fallback}"),
            ImportedItem::Grpc { .. } => format!("greq_{fallback}"),
        }
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn expr_text(expr: &Expr) -> Option<String> {
    Some(match expr {
        Expr::Lit(Value::String(s)) => quote(s),
        Expr::Lit(v) => v.to_string(),
        Expr::Var(path) => format!("_.{}", reqly_template::path_name(path)),
        Expr::Call { .. } => return None,
    })
}

/// Rewrites a template for Insomnia: `{{ $fn(args) }}` becomes `{% fn args %}`
/// and request ids in tags are mapped.
fn template(text: &str, ids: &Ids) -> String {
    let mut out = text.to_string();
    if text.contains("{{")
        && text.contains('$')
        && let Ok(segments) = reqly_template::parse(text)
    {
        let mut rebuilt = String::new();
        let mut last = 0;
        for seg in segments {
            if let Segment::Expr {
                expr: Expr::Call { name, args },
                span,
            } = &seg
                && text[span.clone()].starts_with("{{")
            {
                let (name, args) = match (name.as_str(), args.as_slice()) {
                    // Postman-style names exist in Insomnia's faker tag.
                    (alias, []) if alias.starts_with("random") => {
                        ("faker".to_string(), vec![quote(alias)])
                    }
                    ("guid" | "randomUUID", []) => ("uuid".into(), vec![quote("v4")]),
                    ("timestamp", []) => ("now".into(), vec![quote("unix")]),
                    ("isoTimestamp", []) => ("now".into(), vec![quote("iso-8601")]),
                    _ => match args.iter().map(expr_text).collect::<Option<Vec<_>>>() {
                        Some(args) => (name.clone(), args),
                        None => continue,
                    },
                };
                rebuilt.push_str(&text[last..span.start]);
                if args.is_empty() {
                    rebuilt.push_str(&format!("{{% {name} %}}"));
                } else {
                    rebuilt.push_str(&format!("{{% {name} {} %}}", args.join(", ")));
                }
                last = span.end;
            }
        }
        rebuilt.push_str(&text[last..]);
        out = rebuilt;
    }
    if out.contains("{%") {
        for (old, new) in &ids.0 {
            if old != new {
                for q in ['\'', '"'] {
                    out = out.replace(&format!("{q}{old}{q}"), &format!("{q}{new}{q}"));
                }
            }
        }
    }
    out
}

fn rows(rows: &[KeyValue], ids: &Ids) -> Vec<Value> {
    rows.iter()
        .map(|r| {
            let mut row =
                json!({ "name": template(&r.name, ids), "value": template(&r.value, ids) });
            if !r.enabled {
                row["disabled"] = json!(true);
            }
            if !r.description.is_empty() {
                row["description"] = json!(r.description);
            }
            row
        })
        .collect()
}

fn auth(auth: &Auth, ids: &Ids) -> Value {
    match auth {
        Auth::Inherit => json!({}),
        Auth::None => json!({ "type": "none" }),
        Auth::Basic { username, password } => json!({
            "type": "basic", "username": template(username, ids), "password": template(password, ids),
            "useISO88591": false, "disabled": false
        }),
        Auth::Bearer { token, prefix } => json!({
            "type": "bearer", "token": template(token, ids), "prefix": prefix, "disabled": false
        }),
        Auth::ApiKey {
            key,
            value,
            location,
        } => json!({
            "type": "apikey", "key": key, "value": template(value, ids), "disabled": false,
            "addTo": match location { ApiKeyLocation::Header => "header", ApiKeyLocation::Query => "queryParams" }
        }),
        // Schemes Insomnia lacks (EdgeGrid, JWT) are left out.
        other => crate::auth_map::to_insomnia(other, &|s| template(s, ids))
            .unwrap_or_else(|| json!({ "type": "none" })),
    }
}

fn body(body: &Body, ids: &Ids) -> Value {
    let text = |mime: &str| json!({ "mimeType": mime, "text": template(&body.text, ids) });
    match body.kind {
        BodyKind::None => json!({}),
        BodyKind::Json => text("application/json"),
        BodyKind::Xml => text("application/xml"),
        BodyKind::Yaml => text("text/yaml"),
        BodyKind::Text => text("text/plain"),
        BodyKind::Html => text("text/html"),
        BodyKind::Graphql => json!({
            "mimeType": "application/graphql",
            "text": template(&serde_json::to_string(&crate::graphql_json(body)).unwrap_or_default(), ids),
        }),
        BodyKind::FormUrlencoded => json!({
            "mimeType": "application/x-www-form-urlencoded",
            "params": rows(&body.fields, ids),
        }),
        BodyKind::Multipart => {
            let params: Vec<Value> = body
                .fields
                .iter()
                .zip(rows(&body.fields, ids))
                .map(|(field, mut row)| {
                    if field.kind == KeyValueKind::File {
                        row["type"] = json!("file");
                        row["fileName"] = json!(field.value);
                        row["value"] = json!("");
                    }
                    row
                })
                .collect();
            json!({ "mimeType": "multipart/form-data", "params": params })
        }
        BodyKind::Binary => json!({
            "mimeType": "application/octet-stream",
            "fileName": match &body.file {
                Some(FileRef::Path { path }) => path.clone(),
                _ => String::new(),
            },
        }),
    }
}

fn follow(settings: &RequestSettings) -> &'static str {
    match settings.follow_redirects {
        Some(true) => "on",
        Some(false) => "off",
        None => "global",
    }
}

fn scripts_v4(r: &mut Map<String, Value>, scripts: &Scripts) {
    r.insert("preRequestScript".into(), json!(scripts.pre_request));
    r.insert("afterResponseScript".into(), json!(scripts.after_response));
}

fn variables(map: &Map<String, Value>, ids: &Ids) -> Value {
    let mut out = Map::new();
    for (k, v) in map {
        out.insert(
            k.clone(),
            match v {
                Value::String(s) => json!(template(s, ids)),
                other => other.clone(),
            },
        );
    }
    Value::Object(out)
}

fn request_v4(id: &str, parent: &str, name: &str, r: &HttpRequest, sort: i64, ids: &Ids) -> Value {
    let mut out = Map::new();
    out.insert("_id".into(), json!(id));
    out.insert("parentId".into(), json!(parent));
    out.insert("url".into(), json!(template(&r.url, ids)));
    out.insert("name".into(), json!(name));
    out.insert("description".into(), json!(r.description));
    out.insert("method".into(), json!(r.method));
    out.insert("body".into(), body(&r.body, ids));
    out.insert("parameters".into(), json!(rows(&r.params, ids)));
    out.insert("headers".into(), json!(rows(&r.headers, ids)));
    out.insert("authentication".into(), auth(&r.auth, ids));
    out.insert("metaSortKey".into(), json!(sort));
    out.insert("isPrivate".into(), json!(false));
    out.insert("pathParameters".into(), json!(rows(&r.path_params, ids)));
    out.insert(
        "settingStoreCookies".into(),
        json!(r.settings.store_cookies.unwrap_or(true)),
    );
    out.insert(
        "settingSendCookies".into(),
        json!(r.settings.send_cookies.unwrap_or(true)),
    );
    out.insert("settingDisableRenderRequestBody".into(), json!(false));
    out.insert(
        "settingEncodeUrl".into(),
        json!(r.settings.encode_url.unwrap_or(true)),
    );
    out.insert("settingRebuildPath".into(), json!(true));
    out.insert("settingFollowRedirects".into(), json!(follow(&r.settings)));
    scripts_v4(&mut out, &r.scripts);
    out.insert("_type".into(), json!("request"));
    Value::Object(out)
}

fn folder_v4(id: &str, parent: &str, name: &str, f: &FolderData, sort: i64, ids: &Ids) -> Value {
    let mut out = Map::new();
    out.insert("_id".into(), json!(id));
    out.insert("parentId".into(), json!(parent));
    out.insert("name".into(), json!(name));
    out.insert("description".into(), json!(f.description));
    out.insert("environment".into(), variables(&f.variables, ids));
    out.insert("environmentPropertyOrder".into(), Value::Null);
    out.insert("metaSortKey".into(), json!(sort));
    out.insert("headers".into(), json!(rows(&f.headers, ids)));
    out.insert("authentication".into(), auth(&f.auth, ids));
    scripts_v4(&mut out, &f.scripts);
    out.insert("_type".into(), json!("request_group"));
    Value::Object(out)
}

fn items_v4(
    items: &[ImportedItem],
    parent: &str,
    ids: &Ids,
    fallback: &mut usize,
    out: &mut Vec<Value>,
) {
    for (i, item) in items.iter().enumerate() {
        let id = ids.id(item, fallback);
        let sort = (i as i64 + 1) * 1000;
        match item {
            ImportedItem::Folder {
                name,
                data,
                children,
                ..
            } => {
                out.push(folder_v4(&id, parent, name, data, sort, ids));
                items_v4(children, &id, ids, fallback, out);
            }
            ImportedItem::Request { name, data, .. } => {
                out.push(request_v4(&id, parent, name, data, sort, ids));
            }
            ImportedItem::WebSocket { name, data, .. } => {
                out.push(websocket_v4(&id, parent, name, data, sort, ids));
            }
            ImportedItem::Grpc { name, data, .. } => {
                out.push(grpc_v4(&id, parent, name, data, sort, ids));
            }
        }
    }
}

fn environment_v4(
    id: &str,
    parent: &str,
    env: &ImportedEnvironment,
    sort: i64,
    ids: &Ids,
) -> Value {
    json!({
        "_id": id, "parentId": parent, "name": env.name,
        "data": variables(&env.data, ids), "dataPropertyOrder": null,
        "color": env.color, "isPrivate": false, "metaSortKey": sort, "_type": "environment"
    })
}

fn cookies(data: &ExportData) -> Vec<Value> {
    data.cookies
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut cookie = json!({
                "key": c.name, "value": c.value, "domain": c.domain, "path": c.path,
                "secure": c.secure, "httpOnly": c.http_only, "hostOnly": !c.include_subdomains,
                "id": format!("cookie_{i}"),
            });
            if let Some(exp) = c.expires {
                cookie["expires"] = json!(rfc3339(exp));
            }
            cookie
        })
        .collect()
}

/// Unix seconds → `YYYY-MM-DDTHH:MM:SS.000Z`.
pub(crate) fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil from days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Insomnia v4 JSON export. `exported_at` is Unix milliseconds.
pub fn export_v4(data: &ExportData, exported_at: i64) -> CoreResult<String> {
    let ids = Ids::new(&data.collections);
    let mut resources = Vec::new();
    let mut fallback = 0;
    for (ci, c) in data.collections.iter().enumerate() {
        let ws = format!("wrk_{ci}");
        resources.push(json!({
            "_id": ws, "parentId": null, "name": c.name, "description": c.description,
            "scope": "collection", "_type": "workspace"
        }));
        let base = format!("env_{ci}");
        resources.push(environment_v4(
            &base,
            &ws,
            &ImportedEnvironment {
                name: "Base Environment".into(),
                data: c.variables.clone(),
                ..ImportedEnvironment::default()
            },
            0,
            &ids,
        ));
        for (ei, env) in c.environments.iter().enumerate() {
            resources.push(environment_v4(
                &format!("env_{ci}_{ei}"),
                &base,
                env,
                ei as i64 + 1,
                &ids,
            ));
        }
        if !data.cookies.is_empty() {
            resources.push(json!({
                "_id": format!("jar_{ci}"), "parentId": ws, "name": "Default Jar",
                "cookies": cookies(data), "_type": "cookie_jar"
            }));
        }
        items_v4(&c.items, &ws, &ids, &mut fallback, &mut resources);
    }
    if !data.environments.is_empty() {
        let ws = "wrk_globals".to_string();
        resources.push(json!({
            "_id": ws, "parentId": null, "name": format!("{} globals", data.workspace).trim(),
            "scope": "environment", "_type": "workspace"
        }));
        let (first, rest) = data.environments.split_first().expect("not empty");
        resources.push(environment_v4("env_globals", &ws, first, 0, &ids));
        for (i, env) in rest.iter().enumerate() {
            resources.push(environment_v4(
                &format!("env_globals_{i}"),
                "env_globals",
                env,
                i as i64 + 1,
                &ids,
            ));
        }
    }
    let doc = json!({
        "_type": "export",
        "__export_format": 4,
        "__export_date": rfc3339(exported_at.div_euclid(1000)),
        "__export_source": format!("reqly:{}", env!("CARGO_PKG_VERSION")),
        "resources": resources,
    });
    serde_json::to_string_pretty(&doc).map_err(|e| CoreError::new("export.failed", e.to_string()))
}

// ----- v5 ----------------------------------------------------------------------------

fn settings_v5(s: &RequestSettings) -> Value {
    json!({
        "renderRequestBody": true,
        "encodeUrl": s.encode_url.unwrap_or(true),
        "followRedirects": follow(s),
        "cookies": { "send": s.send_cookies.unwrap_or(true), "store": s.store_cookies.unwrap_or(true) },
        "rebuildPath": true,
    })
}

fn scripts_v5(out: &mut Map<String, Value>, scripts: &Scripts) {
    if scripts.is_empty() {
        return;
    }
    let mut s = Map::new();
    if !scripts.pre_request.is_empty() {
        s.insert("preRequest".into(), json!(scripts.pre_request));
    }
    if !scripts.after_response.is_empty() {
        s.insert("afterResponse".into(), json!(scripts.after_response));
    }
    out.insert("scripts".into(), Value::Object(s));
}

fn items_v5(items: &[ImportedItem], ids: &Ids, fallback: &mut usize) -> Vec<Value> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let id = ids.id(item, fallback);
            let mut out = Map::new();
            match item {
                ImportedItem::Folder {
                    name,
                    data,
                    children,
                    ..
                } => {
                    out.insert("name".into(), json!(name));
                    let mut meta = json!({ "id": id, "sortKey": (i as i64 + 1) * 1000 });
                    if !data.description.is_empty() {
                        meta["description"] = json!(data.description);
                    }
                    out.insert("meta".into(), meta);
                    out.insert("children".into(), json!(items_v5(children, ids, fallback)));
                    scripts_v5(&mut out, &data.scripts);
                    out.insert("authentication".into(), auth(&data.auth, ids));
                    if !data.variables.is_empty() {
                        out.insert("environment".into(), variables(&data.variables, ids));
                    }
                    out.insert("headers".into(), json!(rows(&data.headers, ids)));
                }
                ImportedItem::Request { name, data: r, .. } => {
                    out.insert("url".into(), json!(template(&r.url, ids)));
                    out.insert("name".into(), json!(name));
                    let mut meta =
                        json!({ "id": id, "isPrivate": false, "sortKey": (i as i64 + 1) * 1000 });
                    if !r.description.is_empty() {
                        meta["description"] = json!(r.description);
                    }
                    out.insert("meta".into(), meta);
                    out.insert("method".into(), json!(r.method));
                    out.insert("body".into(), body(&r.body, ids));
                    out.insert("parameters".into(), json!(rows(&r.params, ids)));
                    out.insert("headers".into(), json!(rows(&r.headers, ids)));
                    out.insert("authentication".into(), auth(&r.auth, ids));
                    scripts_v5(&mut out, &r.scripts);
                    out.insert("settings".into(), settings_v5(&r.settings));
                    out.insert("pathParameters".into(), json!(rows(&r.path_params, ids)));
                }
                // Insomnia tells request types apart by the id prefix.
                ImportedItem::WebSocket { name, data: r, .. } => {
                    out.insert("url".into(), json!(template(&r.url, ids)));
                    out.insert("name".into(), json!(name));
                    out.insert("meta".into(), json!({ "id": id, "sortKey": (i as i64 + 1) * 1000, "description": r.description }));
                    out.insert("parameters".into(), json!(rows(&r.params, ids)));
                    out.insert("headers".into(), json!(rows(&r.headers, ids)));
                    out.insert("authentication".into(), auth(&r.auth, ids));
                }
                ImportedItem::Grpc { name, data: r, .. } => {
                    out.insert("url".into(), json!(template(&r.url, ids)));
                    out.insert("name".into(), json!(name));
                    out.insert("meta".into(), json!({ "id": id, "sortKey": (i as i64 + 1) * 1000, "description": r.description }));
                    let method = if r.service.is_empty() { String::new() } else { format!("/{}/{}", r.service, r.method) };
                    out.insert("protoMethodName".into(), json!(method));
                    out.insert("metadata".into(), json!(rows(&r.metadata, ids)));
                    out.insert("body".into(), json!({ "text": template(&r.message, ids) }));
                }
            }
            Value::Object(out)
        })
        .collect()
}

/// Insomnia v5 YAML export of one collection.
pub fn export_v5(data: &ExportData) -> CoreResult<String> {
    let collection = match data.collections.as_slice() {
        [one] => one,
        [] => return Err(CoreError::new("export.empty", "nothing to export")),
        _ => {
            return Err(CoreError::new(
                "export.single_collection",
                "Insomnia v5 files hold one collection; export a collection instead of the workspace",
            ));
        }
    };
    let ids = Ids::new(&data.collections);
    let mut fallback = 0;
    let mut doc = Map::new();
    doc.insert("type".into(), json!("collection.insomnia.rest/5.0"));
    doc.insert("schema_version".into(), json!("5.1"));
    doc.insert("name".into(), json!(collection.name));
    let mut meta = json!({ "id": "wrk_0" });
    if !collection.description.is_empty() {
        meta["description"] = json!(collection.description);
    }
    doc.insert("meta".into(), meta);
    doc.insert(
        "collection".into(),
        json!(items_v5(&collection.items, &ids, &mut fallback)),
    );
    if !data.cookies.is_empty() {
        doc.insert(
            "cookieJar".into(),
            json!({ "name": "Default Jar", "meta": { "id": "jar_0" }, "cookies": cookies(data) }),
        );
    }
    let subs: Vec<Value> = collection
        .environments
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut env = json!({
                "name": e.name, "meta": { "id": format!("env_0_{i}"), "isPrivate": false, "sortKey": i + 1 },
                "data": variables(&e.data, &ids),
            });
            if let Some(color) = &e.color {
                env["color"] = json!(color);
            }
            env
        })
        .collect();
    doc.insert(
        "environments".into(),
        json!({
            "name": "Base Environment",
            "meta": { "id": "env_0", "isPrivate": false },
            "data": variables(&collection.variables, &ids),
            "subEnvironments": subs,
        }),
    );
    serde_yaml_ng::to_string(&Value::Object(doc))
        .map_err(|e| CoreError::new("export.failed", e.to_string()))
}

fn websocket_v4(
    id: &str,
    parent: &str,
    name: &str,
    r: &reqly_core::model::WebSocketRequest,
    sort: i64,
    ids: &Ids,
) -> Value {
    json!({
        "_id": id, "parentId": parent, "name": name, "url": template(&r.url, ids),
        "metaSortKey": sort, "headers": rows(&r.headers, ids), "parameters": rows(&r.params, ids),
        "authentication": auth(&r.auth, ids), "description": r.description,
        "settingEncodeUrl": true, "settingStoreCookies": true, "settingSendCookies": true,
        "settingFollowRedirects": "global", "_type": "websocket_request"
    })
}

fn grpc_v4(
    id: &str,
    parent: &str,
    name: &str,
    r: &reqly_core::model::GrpcRequest,
    sort: i64,
    ids: &Ids,
) -> Value {
    let method = if r.service.is_empty() {
        String::new()
    } else {
        format!("/{}/{}", r.service, r.method)
    };
    json!({
        "_id": id, "parentId": parent, "name": name, "url": template(&r.url, ids),
        "description": r.description, "protoFileId": null, "protoMethodName": method,
        "metadata": rows(&r.metadata, ids), "body": { "text": template(&r.message, ids) },
        "reflectionApi": { "enabled": r.proto.reflection, "url": "", "apiKey": "", "module": "" },
        "metaSortKey": sort, "_type": "grpc_request"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_893_456_000), "2030-01-01T00:00:00.000Z");
    }

    #[test]
    fn templates_become_tags() {
        let ids = Ids(HashMap::from([("abc".to_string(), "req_abc".to_string())]));
        assert_eq!(template("{{ $uuid('v4') }}", &ids), "{% uuid 'v4' %}");
        assert_eq!(
            template("a {{ $randomEmail }} b", &ids),
            "a {% faker 'randomEmail' %} b"
        );
        assert_eq!(
            template("{{ $faker.person.firstName }}", &ids),
            "{% faker 'person.firstName' %}"
        );
        assert_eq!(
            template("{% response 'body', 'abc', '$.t' %}", &ids),
            "{% response 'body', 'req_abc', '$.t' %}"
        );
        assert_eq!(template("{{ host }}", &ids), "{{ host }}");
    }
}
