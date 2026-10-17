//! Postman Collection v2.0 / v2.1, environments and globals: import and export.

use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind,
    RequestSettings, Scripts,
};
use reqly_core::{CoreError, CoreResult};
use serde_json::{Map, Value, json};

use crate::detect::{Format, parse_structured};
use crate::model::{
    ExportData, ImportBundle, ImportedCollection, ImportedEnvironment, ImportedItem,
};
use crate::report::{ImportReport, ReportKind, join_path};

pub fn is_collection(value: &Value) -> bool {
    value
        .get("info")
        .and_then(|i| i.get("schema"))
        .and_then(Value::as_str)
        .is_some_and(|s| s.contains("postman.com") && s.contains("collection/v2"))
}

pub fn is_environment(value: &Value) -> bool {
    value.get("_postman_variable_scope").is_some()
        || (value.get("values").is_some_and(Value::is_array)
            && value.get("name").is_some()
            && value.get("info").is_none())
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn disabled(v: &Value) -> bool {
    v.get("disabled").and_then(Value::as_bool).unwrap_or(false)
}

/// Value of a variable: strings as-is, other JSON kept.
fn value_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn description(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(obj @ Value::Object(_)) => s(obj, "content").to_string(),
        _ => String::new(),
    }
}

// ----- variable syntax ---------------------------------------------------------------

fn is_plain(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `{{api.host}}` → `{{ _['api.host'] }}`; plain names and `{{$dynamic}}` stay.
pub fn vars_to_reqly(text: &str) -> String {
    if !text.contains("{{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &after[..end];
        let name = inner.trim();
        if name.starts_with('$') || is_plain(name) || name.contains('{') {
            out.push_str("{{");
            out.push_str(inner);
            out.push_str("}}");
        } else {
            out.push_str(&format!(
                "{{{{ _['{}'] }}}}",
                name.replace('\\', "\\\\").replace('\'', "\\'")
            ));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Inverse of [`vars_to_reqly`] for export: `{{ _['a.b'] }}` → `{{a.b}}`,
/// `{{ name }}` → `{{name}}`, `{{ $uuid }}` → `{{$guid}}`.
pub fn vars_to_postman(text: &str) -> String {
    if !text.contains("{{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = after[..end].trim();
        let bracket = inner
            .strip_prefix("_['")
            .or_else(|| inner.strip_prefix("_[\""))
            .and_then(|r| r.strip_suffix("']").or_else(|| r.strip_suffix("\"]")));
        let name = match bracket {
            Some(name) => name.replace("\\'", "'"),
            None => {
                let n = inner.strip_prefix("_.").unwrap_or(inner);
                match n {
                    "$uuid" | "$uuid()" => "$guid".into(),
                    other => other.to_string(),
                }
            }
        };
        out.push_str(&format!("{{{{{name}}}}}"));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

// ----- import --------------------------------------------------------------------------

struct Importer {
    report: ImportReport,
}

fn kv(name: &str, value: &str, enabled: bool) -> KeyValue {
    let mut row = KeyValue::new(vars_to_reqly(name), vars_to_reqly(value));
    row.enabled = enabled;
    row
}

/// Postman auth parameters: v2.1 `[{key, value}]`, v2.0 `{key: value}`.
fn auth_params(auth: &Value, ty: &str) -> Map<String, Value> {
    match auth.get(ty) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|p| {
                (
                    s(p, "key").to_string(),
                    p.get("value").cloned().unwrap_or(Value::Null),
                )
            })
            .collect(),
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

impl Importer {
    fn auth(&mut self, path: &str, auth: Option<&Value>) -> Auth {
        let Some(auth) = auth.filter(|a| !a.is_null()) else {
            return Auth::Inherit;
        };
        let ty = s(auth, "type");
        let p = auth_params(auth, ty);
        let get = |k: &str| vars_to_reqly(&value_text(p.get(k)));
        match ty {
            "" | "inherit" => Auth::Inherit,
            "noauth" => Auth::None,
            "basic" => Auth::Basic {
                username: get("username"),
                password: get("password"),
            },
            "bearer" => Auth::Bearer {
                token: get("token"),
                prefix: String::new(),
            },
            "apikey" => Auth::ApiKey {
                key: get("key"),
                value: get("value"),
                location: if value_text(p.get("in")) == "query" {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            },
            other => {
                // OAuth 2.0 audience/resource are `{id: value}` maps in Postman.
                let get = |k: &str| match p.get(k) {
                    Some(Value::Object(map)) => map
                        .values()
                        .map(|v| value_text(Some(v)))
                        .collect::<Vec<_>>()
                        .join(" "),
                    v => vars_to_reqly(&value_text(v)),
                };
                if let Some(auth) =
                    crate::auth_map::from_postman(other, &crate::auth_map::Fields { get: &get })
                {
                    return auth;
                }
                self.report.push_raw(
                    ReportKind::Unsupported,
                    "auth_unsupported",
                    path,
                    format!("`{other}` authentication is not supported yet; the request was imported without it"),
                    auth.clone(),
                );
                Auth::None
            }
        }
    }

    fn scripts(&mut self, path: &str, events: &[Value]) -> Scripts {
        let text = |listen: &str| {
            events
                .iter()
                .filter(|e| s(e, "listen") == listen && !disabled(e))
                .filter_map(|e| e.get("script"))
                .map(|script| match script.get("exec") {
                    Some(Value::Array(lines)) => lines
                        .iter()
                        .map(|l| l.as_str().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        };
        let scripts = Scripts {
            pre_request: text("prerequest"),
            after_response: text("test"),
        };
        if !scripts.is_empty() {
            self.report.push(
                ReportKind::Unsupported,
                "script_kept",
                path,
                "Postman scripts were kept but are not executed (scripting arrives in stage 6)",
            );
        }
        scripts
    }

    fn url(&mut self, url: Option<&Value>) -> (String, Vec<KeyValue>, Vec<KeyValue>) {
        let Some(url) = url else {
            return (String::new(), vec![], vec![]);
        };
        let (raw, query, variables) = match url {
            Value::String(raw) => (raw.clone(), Vec::new(), Vec::new()),
            obj => {
                let mut raw = s(obj, "raw").to_string();
                if raw.is_empty() {
                    // Build from parts.
                    let host = arr(obj, "host")
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    let path = arr(obj, "path")
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("/");
                    let protocol = s(obj, "protocol");
                    raw = format!(
                        "{}{host}{}{}",
                        if protocol.is_empty() {
                            String::new()
                        } else {
                            format!("{protocol}://")
                        },
                        if s(obj, "port").is_empty() {
                            String::new()
                        } else {
                            format!(":{}", s(obj, "port"))
                        },
                        if path.is_empty() {
                            String::new()
                        } else {
                            format!("/{path}")
                        },
                    );
                }
                (
                    raw,
                    arr(obj, "query").to_vec(),
                    arr(obj, "variable").to_vec(),
                )
            }
        };
        let (base, raw_query) = match raw.split_once('?') {
            Some((b, q)) => (b.to_string(), q.to_string()),
            None => (raw.clone(), String::new()),
        };
        let params = if !query.is_empty() {
            query
                .iter()
                .filter(|q| !s(q, "key").is_empty() || q.get("value").is_some())
                .map(|q| kv(s(q, "key"), &value_text(q.get("value")), !disabled(q)))
                .collect()
        } else {
            raw_query
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    kv(k, v, true)
                })
                .collect()
        };
        let path_params = variables
            .iter()
            .map(|v| {
                let mut row = kv(s(v, "key"), &value_text(v.get("value")), !disabled(v));
                row.description = description(v.get("description"));
                row
            })
            .collect();
        (vars_to_reqly(&base), params, path_params)
    }

    fn body(&mut self, path: &str, body: Option<&Value>, headers: &[KeyValue]) -> Body {
        let Some(body) = body.filter(|b| !b.is_null() && !disabled(b)) else {
            return Body::default();
        };
        match s(body, "mode") {
            "raw" => {
                let raw = vars_to_reqly(s(body, "raw"));
                if raw.is_empty() {
                    return Body::default();
                }
                let language = body
                    .get("options")
                    .and_then(|o| o.get("raw"))
                    .and_then(|r| r.get("language"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let ct = crate::text::content_type(headers).unwrap_or("").to_ascii_lowercase();
                let kind = match language {
                    "json" => BodyKind::Json,
                    "xml" => BodyKind::Xml,
                    "html" => BodyKind::Html,
                    "text" | "javascript" => BodyKind::Text,
                    _ if ct.contains("json") => BodyKind::Json,
                    _ if ct.contains("xml") => BodyKind::Xml,
                    _ if ct.contains("yaml") => BodyKind::Yaml,
                    _ if ct.contains("html") => BodyKind::Html,
                    _ => BodyKind::Text,
                };
                Body { kind, text: raw, ..Body::default() }
            }
            "urlencoded" => Body {
                kind: BodyKind::FormUrlencoded,
                fields: arr(body, "urlencoded")
                    .iter()
                    .map(|f| kv(s(f, "key"), &value_text(f.get("value")), !disabled(f)))
                    .collect(),
                ..Body::default()
            },
            "formdata" => Body {
                kind: BodyKind::Multipart,
                fields: arr(body, "formdata")
                    .iter()
                    .map(|f| {
                        if s(f, "type") == "file" {
                            let src = match f.get("src") {
                                Some(Value::Array(srcs)) => {
                                    if srcs.len() > 1 {
                                        self.report.push(
                                            ReportKind::Converted,
                                            "multiple_files",
                                            path,
                                            format!("form field `{}` has several files; only the first was kept", s(f, "key")),
                                        );
                                    }
                                    srcs.first().and_then(Value::as_str).unwrap_or("").to_string()
                                }
                                other => value_text(other),
                            };
                            let mut row = kv(s(f, "key"), &src, !disabled(f));
                            row.value = src;
                            row.kind = KeyValueKind::File;
                            row
                        } else {
                            kv(s(f, "key"), &value_text(f.get("value")), !disabled(f))
                        }
                    })
                    .collect(),
                ..Body::default()
            },
            "file" => {
                let src = body.get("file").map(|f| s(f, "src")).unwrap_or("");
                Body {
                    kind: BodyKind::Binary,
                    file: (!src.is_empty()).then(|| FileRef::Path { path: src.into() }),
                    ..Body::default()
                }
            }
            "graphql" => {
                let gql = body.get("graphql").cloned().unwrap_or(Value::Null);
                let variables = match gql.get("variables") {
                    Some(Value::String(v)) => Value::String(vars_to_reqly(v)),
                    Some(v) => Value::String(vars_to_reqly(&serde_json::to_string_pretty(v).unwrap_or_default())),
                    None => Value::Null,
                };
                crate::graphql_body(&vars_to_reqly(s(&gql, "query")), &variables, "")
            }
            other => {
                self.report.push_raw(
                    ReportKind::Unsupported,
                    "body_unsupported",
                    path,
                    format!("`{other}` bodies are not supported"),
                    body.clone(),
                );
                Body::default()
            }
        }
    }

    fn variables(&mut self, path: &str, vars: &[Value]) -> (Map<String, Value>, Vec<String>) {
        let mut data = Map::new();
        let mut secrets = Vec::new();
        for v in vars {
            let key = s(v, "key");
            if key.is_empty() {
                continue;
            }
            if disabled(v) || v.get("enabled") == Some(&Value::Bool(false)) {
                self.report.push(
                    ReportKind::Info,
                    "variable_disabled",
                    path,
                    format!("disabled variable `{key}` was not imported"),
                );
                continue;
            }
            let value = match v.get("value") {
                Some(Value::String(s)) => Value::String(vars_to_reqly(s)),
                Some(other) => other.clone(),
                None => Value::String(String::new()),
            };
            data.insert(key.to_string(), value);
            if s(v, "type") == "secret" {
                secrets.push(key.to_string());
            }
        }
        (data, secrets)
    }

    fn request(&mut self, parent: &str, item: &Value) -> ImportedItem {
        let name = s(item, "name");
        let path = join_path(parent, name);
        let request = item.get("request").cloned().unwrap_or(Value::Null);
        let request = match request {
            Value::String(url) => json!({ "method": "GET", "url": url }),
            other => other,
        };
        let headers: Vec<KeyValue> = match request.get("header") {
            Some(Value::Array(h)) => h
                .iter()
                .map(|h| {
                    let mut row = kv(s(h, "key"), &value_text(h.get("value")), !disabled(h));
                    row.description = description(h.get("description"));
                    row
                })
                .collect(),
            Some(Value::String(raw)) => raw
                .lines()
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| kv(k.trim(), v.trim(), true))
                .collect(),
            _ => Vec::new(),
        };
        let (url, params, path_params) = self.url(request.get("url"));
        let body = self.body(&path, request.get("body"), &headers);
        let mut settings = RequestSettings::default();
        if let Some(ppb) = item.get("protocolProfileBehavior") {
            if let Some(f) = ppb.get("followRedirects").and_then(Value::as_bool) {
                settings.follow_redirects = Some(f);
            }
            if ppb.get("disableUrlEncoding").and_then(Value::as_bool) == Some(true) {
                settings.encode_url = Some(false);
            }
            if ppb.get("strictSSL").and_then(Value::as_bool) == Some(false) {
                settings.verify_tls = Some(false);
            }
            if let Some(n) = ppb.get("maxRedirects").and_then(Value::as_u64) {
                settings.max_redirects = Some(n as u32);
            }
        }
        let examples = arr(item, "response").len();
        if examples > 0 {
            self.report.push(
                ReportKind::Info,
                "examples_skipped",
                &path,
                format!("{examples} saved example response(s) were not imported"),
            );
        }
        let (variables, _) = self.variables(&path, arr(item, "variable"));
        let data = HttpRequest {
            method: match s(&request, "method") {
                "" => "GET".into(),
                m => m.to_uppercase(),
            },
            url,
            params,
            path_params,
            auth: self.auth(&path, request.get("auth")),
            body,
            settings,
            description: description(request.get("description")),
            variables,
            scripts: self.scripts(&path, arr(item, "event")),
            headers,
            event_stream: false,
        };
        let mut out = ImportedItem::request(name, data);
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            out = out.with_id(id);
        }
        out
    }

    fn items(&mut self, parent: &str, items: &[Value]) -> Vec<ImportedItem> {
        items
            .iter()
            .map(|item| {
                if item.get("item").is_some() {
                    let name = s(item, "name");
                    let path = join_path(parent, name);
                    let (variables, _) = self.variables(&path, arr(item, "variable"));
                    let data = FolderData {
                        auth: self.auth(&path, item.get("auth")),
                        description: description(item.get("description")),
                        scripts: self.scripts(&path, arr(item, "event")),
                        variables,
                        ..FolderData::default()
                    };
                    let children = self.items(&path, arr(item, "item"));
                    ImportedItem::folder(name, data, children)
                } else {
                    self.request(parent, item)
                }
            })
            .collect()
    }

    fn collection(&mut self, value: &Value) -> ImportedCollection {
        let info = value.get("info").cloned().unwrap_or(Value::Null);
        let name = s(&info, "name").to_string();
        let (variables, secrets) = self.variables(&name, arr(value, "variable"));
        let mut items = self.items(&name, arr(value, "item"));

        // Collection-level auth: Reqly collections have none, so top-level
        // items that inherit get it.
        let auth = self.auth(&name, value.get("auth"));
        if auth != Auth::Inherit {
            for item in &mut items {
                match item {
                    ImportedItem::Folder { data, .. } if data.auth == Auth::Inherit => {
                        data.auth = auth.clone()
                    }
                    ImportedItem::Request { data, .. } if data.auth == Auth::Inherit => {
                        data.auth = auth.clone()
                    }
                    _ => {}
                }
            }
            self.report.push(
                ReportKind::Converted,
                "collection_auth",
                &name,
                "collection authentication was applied to the top-level folders and requests",
            );
        }
        let scripts = self.scripts_raw(arr(value, "event"));
        if let Some(raw) = scripts {
            self.report.push_raw(
                ReportKind::Unsupported,
                "collection_scripts",
                &name,
                "collection-level scripts are not supported; they are listed here",
                raw,
            );
        }
        ImportedCollection {
            name,
            description: description(info.get("description")),
            variables,
            variable_secrets: secrets,
            items,
            ..ImportedCollection::default()
        }
    }

    /// Non-empty collection events, for the report.
    fn scripts_raw(&self, events: &[Value]) -> Option<Value> {
        let non_empty: Vec<&Value> = events
            .iter()
            .filter(|e| {
                e.get("script")
                    .and_then(|s| s.get("exec"))
                    .is_some_and(|exec| match exec {
                        Value::Array(lines) => lines
                            .iter()
                            .any(|l| l.as_str().is_some_and(|l| !l.trim().is_empty())),
                        Value::String(s) => !s.trim().is_empty(),
                        _ => false,
                    })
            })
            .collect();
        (!non_empty.is_empty()).then(|| json!(non_empty))
    }

    fn environment(&mut self, value: &Value) -> ImportedEnvironment {
        let name =
            if s(value, "_postman_variable_scope") == "globals" && s(value, "name").is_empty() {
                "Globals".to_string()
            } else if s(value, "name").is_empty() {
                "Postman environment".to_string()
            } else {
                s(value, "name").to_string()
            };
        let (data, secrets) = self.variables(&name, arr(value, "values"));
        ImportedEnvironment {
            name,
            color: None,
            data,
            secrets,
        }
    }
}

pub fn import(text: &str) -> CoreResult<ImportBundle> {
    let value =
        parse_structured(text).ok_or_else(|| crate::invalid(Format::Postman, "not JSON"))?;
    let mut importer = Importer {
        report: ImportReport {
            format: Format::Postman,
            entries: Vec::new(),
        },
    };
    let mut bundle = ImportBundle {
        format: Format::Postman,
        ..ImportBundle::default()
    };
    if is_collection(&value) {
        bundle.collections.push(importer.collection(&value));
    } else if is_environment(&value) {
        bundle.environments.push(importer.environment(&value));
    } else {
        return Err(crate::invalid(
            Format::Postman,
            "not a Postman collection or environment",
        ));
    }
    bundle.report = importer.report;
    Ok(bundle)
}

// ----- export --------------------------------------------------------------------------

fn kv_out(rows: &[KeyValue]) -> Vec<Value> {
    rows.iter()
        .map(|r| {
            let mut v =
                json!({ "key": vars_to_postman(&r.name), "value": vars_to_postman(&r.value) });
            if !r.enabled {
                v["disabled"] = json!(true);
            }
            if !r.description.is_empty() {
                v["description"] = json!(r.description);
            }
            v
        })
        .collect()
}

fn auth_out(auth: &Auth) -> Option<Value> {
    let param =
        |k: &str, v: &str| json!({ "key": k, "value": vars_to_postman(v), "type": "string" });
    Some(match auth {
        Auth::Inherit => return None,
        Auth::None => json!({ "type": "noauth" }),
        Auth::Basic { username, password } => {
            json!({ "type": "basic", "basic": [param("username", username), param("password", password)] })
        }
        Auth::Bearer { token, .. } => {
            json!({ "type": "bearer", "bearer": [param("token", token)] })
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => json!({
            "type": "apikey",
            "apikey": [param("key", key), param("value", value), param("in", match location { ApiKeyLocation::Header => "header", ApiKeyLocation::Query => "query" })]
        }),
        other => {
            let (ty, params) = crate::auth_map::to_postman(other, &vars_to_postman)?;
            let params: Vec<Value> = params
                .into_iter()
                .map(|(k, v)| json!({ "key": k, "value": v, "type": if v.is_boolean() { "boolean" } else { "string" } }))
                .collect();
            json!({ "type": ty, ty: params })
        }
    })
}

fn events_out(scripts: &Scripts) -> Vec<Value> {
    let mut out = Vec::new();
    for (listen, text) in [
        ("prerequest", &scripts.pre_request),
        ("test", &scripts.after_response),
    ] {
        if !text.trim().is_empty() {
            out.push(json!({ "listen": listen, "script": { "type": "text/javascript", "exec": text.lines().collect::<Vec<_>>() } }));
        }
    }
    out
}

fn body_out(body: &Body) -> Option<Value> {
    let raw = |language: &str| json!({ "mode": "raw", "raw": vars_to_postman(&body.text), "options": { "raw": { "language": language } } });
    Some(match body.kind {
        BodyKind::None => return None,
        BodyKind::Json => raw("json"),
        BodyKind::Xml => raw("xml"),
        BodyKind::Html => raw("html"),
        BodyKind::Graphql => {
            let g = &body.graphql;
            json!({ "mode": "graphql", "graphql": { "query": vars_to_postman(&g.query), "variables": vars_to_postman(&g.variables) } })
        }
        BodyKind::Text | BodyKind::Yaml => raw("text"),
        BodyKind::FormUrlencoded => {
            json!({ "mode": "urlencoded", "urlencoded": kv_out(&body.fields) })
        }
        BodyKind::Multipart => json!({
            "mode": "formdata",
            "formdata": body.fields.iter().map(|f| {
                let mut v = if f.kind == KeyValueKind::File {
                    json!({ "key": vars_to_postman(&f.name), "type": "file", "src": f.value })
                } else {
                    json!({ "key": vars_to_postman(&f.name), "value": vars_to_postman(&f.value), "type": "text" })
                };
                if !f.enabled { v["disabled"] = json!(true); }
                v
            }).collect::<Vec<_>>()
        }),
        BodyKind::Binary => json!({
            "mode": "file",
            "file": { "src": match &body.file { Some(FileRef::Path { path }) => path.clone(), _ => String::new() } }
        }),
    })
}

fn url_out(r: &HttpRequest) -> Value {
    let base = vars_to_postman(&r.url);
    let enabled: Vec<&KeyValue> = r.params.iter().filter(|p| p.enabled).collect();
    let query: String = enabled
        .iter()
        .map(|p| {
            if p.value.is_empty() {
                vars_to_postman(&p.name)
            } else {
                format!("{}={}", vars_to_postman(&p.name), vars_to_postman(&p.value))
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    let raw = if query.is_empty() {
        base.clone()
    } else {
        format!("{base}?{query}")
    };
    let (scheme, rest) = match base.split_once("://") {
        Some((s, r)) => (Some(s.to_string()), r.to_string()),
        None => (None, base.clone()),
    };
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (
            h.to_string(),
            p.split('/').map(str::to_string).collect::<Vec<_>>(),
        ),
        None => (rest.clone(), Vec::new()),
    };
    let mut url = json!({ "raw": raw, "host": [host], "path": path });
    if let Some(scheme) = scheme {
        url["protocol"] = json!(scheme);
    }
    if !r.params.is_empty() {
        url["query"] = json!(kv_out(&r.params));
    }
    if !r.path_params.is_empty() {
        url["variable"] = json!(kv_out(&r.path_params));
    }
    url
}

/// `None` for WebSocket and gRPC requests: Postman collections cannot hold them.
fn item_out(item: &ImportedItem) -> Option<Value> {
    Some(match item {
        ImportedItem::Folder {
            name,
            data,
            children,
            ..
        } => {
            let mut v = json!({ "name": name, "item": children.iter().filter_map(item_out).collect::<Vec<_>>() });
            if !data.description.is_empty() {
                v["description"] = json!(data.description);
            }
            if let Some(auth) = auth_out(&data.auth) {
                v["auth"] = auth;
            }
            let events = events_out(&data.scripts);
            if !events.is_empty() {
                v["event"] = json!(events);
            }
            if !data.variables.is_empty() {
                v["variable"] = json!(vars_out(&data.variables, &[]));
            }
            v
        }
        ImportedItem::Request { name, data: r, .. } => {
            let mut request = json!({
                "method": r.method,
                "header": kv_out(&r.headers),
                "url": url_out(r),
            });
            if let Some(body) = body_out(&r.body) {
                request["body"] = body;
            }
            if let Some(auth) = auth_out(&r.auth) {
                request["auth"] = auth;
            }
            if !r.description.is_empty() {
                request["description"] = json!(r.description);
            }
            let mut v = json!({ "name": name, "request": request, "response": [] });
            let events = events_out(&r.scripts);
            if !events.is_empty() {
                v["event"] = json!(events);
            }
            let mut ppb = Map::new();
            if let Some(f) = r.settings.follow_redirects {
                ppb.insert("followRedirects".into(), json!(f));
            }
            if r.settings.encode_url == Some(false) {
                ppb.insert("disableUrlEncoding".into(), json!(true));
            }
            if r.settings.verify_tls == Some(false) {
                ppb.insert("strictSSL".into(), json!(false));
            }
            if !ppb.is_empty() {
                v["protocolProfileBehavior"] = Value::Object(ppb);
            }
            if !r.variables.is_empty() {
                v["variable"] = json!(vars_out(&r.variables, &[]));
            }
            v
        }
        ImportedItem::WebSocket { .. } | ImportedItem::Grpc { .. } => return None,
    })
}

fn vars_out(data: &Map<String, Value>, secrets: &[String]) -> Vec<Value> {
    data.iter()
        .map(|(k, v)| {
            json!({
                "key": k,
                "value": match v { Value::String(s) => json!(vars_to_postman(s)), other => json!(other.to_string()) },
                "type": if secrets.contains(k) { "secret" } else { "string" },
            })
        })
        .collect()
}

/// A Postman environment file.
pub fn environment_out(env: &ImportedEnvironment, scope: &str) -> Value {
    json!({
        "name": env.name,
        "values": env.data.iter().map(|(k, v)| json!({
            "key": k,
            "value": match v { Value::String(s) => vars_to_postman(s), other => other.to_string() },
            "type": if env.secrets.contains(k) { "secret" } else { "default" },
            "enabled": true,
        })).collect::<Vec<_>>(),
        "_postman_variable_scope": scope,
        "_postman_exported_using": format!("Reqly/{}", env!("CARGO_PKG_VERSION")),
    })
}

pub fn collection_out(c: &ImportedCollection) -> Value {
    let mut v = json!({
        "info": {
            "name": c.name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
        },
        "item": c.items.iter().filter_map(item_out).collect::<Vec<_>>(),
    });
    if !c.description.is_empty() {
        v["info"]["description"] = json!(c.description);
    }
    if !c.variables.is_empty() {
        v["variable"] = json!(vars_out(&c.variables, &c.variable_secrets));
    }
    v
}

/// Files of a Postman export: `(file name suffix, content)`.
pub fn export(data: &ExportData) -> CoreResult<Vec<(String, String)>> {
    let pretty = |v: &Value| {
        serde_json::to_string_pretty(v).map_err(|e| CoreError::new("export.failed", e.to_string()))
    };
    let mut files = Vec::new();
    let many = data.collections.len() > 1;
    for c in &data.collections {
        let stem = if many {
            format!("{}.", crate::file_stem(&c.name))
        } else {
            String::new()
        };
        files.push((
            format!("{stem}postman_collection.json"),
            pretty(&collection_out(c))?,
        ));
        for env in &c.environments {
            files.push((
                format!(
                    "{stem}{}.postman_environment.json",
                    crate::file_stem(&env.name)
                ),
                pretty(&environment_out(env, "environment"))?,
            ));
        }
    }
    for env in &data.environments {
        files.push((
            format!("{}.postman_environment.json", crate::file_stem(&env.name)),
            pretty(&environment_out(env, "environment"))?,
        ));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_names() {
        assert_eq!(
            vars_to_reqly("{{api.host}}/x/{{page size}}?a={{a}}&g={{$guid}}"),
            "{{ _['api.host'] }}/x/{{ _['page size'] }}?a={{a}}&g={{$guid}}"
        );
        assert_eq!(
            vars_to_postman("{{ _['api.host'] }}/x/{{ a }}/{{ _.b }}/{{ $uuid }}"),
            "{{api.host}}/x/{{a}}/{{b}}/{{$guid}}"
        );
        assert_eq!(vars_to_reqly("{{ unterminated"), "{{ unterminated");
    }
}
