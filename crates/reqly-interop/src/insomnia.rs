//! Insomnia import: export format v3/v4 (JSON or YAML, a flat `resources`
//! list) and v5 (YAML, nested). v5 is first flattened into v4-style
//! resources so one converter handles both.

use std::collections::HashSet;

use reqly_core::CoreResult;
use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind,
    RequestSettings, Scripts,
};
use serde_json::{Map, Value, json};

use crate::detect::{Format, parse_structured};
use crate::model::{
    ImportBundle, ImportedCollection, ImportedCookie, ImportedEnvironment, ImportedItem,
};
use crate::report::{ImportReport, ReportKind, join_path};

// ----- detection -----------------------------------------------------------------

pub fn is_v4(value: &Value) -> bool {
    value.get("_type").and_then(Value::as_str) == Some("export")
        && matches!(
            value.get("__export_format").and_then(Value::as_u64),
            Some(3 | 4)
        )
        && value.get("resources").is_some_and(Value::is_array)
}

pub fn is_v5(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.contains(".insomnia.rest/5") || t.starts_with("mcpClient.insomnia/5"))
}

// ----- small accessors -----------------------------------------------------------

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A JSON field as text (booleans and numbers as written).
fn field_text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn b(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn sort_key(v: &Value) -> f64 {
    v.get("metaSortKey").and_then(Value::as_f64).unwrap_or(0.0)
}

fn rows(values: &[Value]) -> Vec<KeyValue> {
    values
        .iter()
        .filter(|p| !s(p, "name").is_empty() || !s(p, "value").is_empty())
        .map(|p| {
            let mut row = KeyValue::new(s(p, "name"), s(p, "value"));
            row.enabled = !b(p, "disabled").unwrap_or(false);
            row.description = s(p, "description").to_string();
            row
        })
        .collect()
}

// ----- v5 → v4 resources -----------------------------------------------------------

fn meta_into(meta: Option<&Value>, out: &mut Map<String, Value>, default_id: &str) {
    let meta = meta.cloned().unwrap_or(Value::Null);
    let id = meta.get("id").and_then(Value::as_str).unwrap_or(default_id);
    out.insert("_id".into(), json!(id));
    out.insert("description".into(), json!(s(&meta, "description")));
    out.insert(
        "metaSortKey".into(),
        meta.get("sortKey").cloned().unwrap_or(json!(0)),
    );
    out.insert(
        "isPrivate".into(),
        json!(b(&meta, "isPrivate").unwrap_or(false)),
    );
}

fn v5_items(items: &[Value], parent: &str, out: &mut Vec<Value>, counter: &mut usize) {
    for (index, item) in items.iter().enumerate() {
        *counter += 1;
        let mut r = Map::new();
        let id_hint = item
            .get("meta")
            .and_then(|m| m.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let is_group = item.get("method").is_none()
            && item.get("url").is_none()
            && item.get("reflectionApi").is_none();
        let kind = if is_group {
            "request_group"
        } else if id_hint.starts_with("ws-req") {
            "websocket_request"
        } else if id_hint.starts_with("greq") || item.get("protoMethodName").is_some() {
            "grpc_request"
        } else if id_hint.starts_with("socketio-req") {
            "socketio_request"
        } else if id_hint.starts_with("mcp-req") {
            "mcp_request"
        } else {
            "request"
        };
        meta_into(item.get("meta"), &mut r, &format!("__v5_{counter}"));
        if r.get("metaSortKey") == Some(&json!(0)) {
            r.insert("metaSortKey".into(), json!(index));
        }
        r.insert("_type".into(), json!(kind));
        r.insert("parentId".into(), json!(parent));
        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                match k.as_str() {
                    "meta" | "children" | "scripts" | "settings" => {}
                    _ => {
                        r.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if let Some(scripts) = item.get("scripts") {
            r.insert("preRequestScript".into(), json!(s(scripts, "preRequest")));
            r.insert(
                "afterResponseScript".into(),
                json!(s(scripts, "afterResponse")),
            );
        }
        if let Some(settings) = item.get("settings") {
            if let Some(v) = b(settings, "encodeUrl") {
                r.insert("settingEncodeUrl".into(), json!(v));
            }
            if let Some(v) = settings.get("followRedirects") {
                r.insert("settingFollowRedirects".into(), v.clone());
            }
            if let Some(cookies) = settings.get("cookies") {
                if let Some(v) = b(cookies, "send") {
                    r.insert("settingSendCookies".into(), json!(v));
                }
                if let Some(v) = b(cookies, "store") {
                    r.insert("settingStoreCookies".into(), json!(v));
                }
            }
            if b(settings, "renderRequestBody") == Some(false) {
                r.insert("settingDisableRenderRequestBody".into(), json!(true));
            }
        }
        let id = r["_id"].as_str().unwrap_or_default().to_string();
        out.push(Value::Object(r));
        if is_group {
            v5_items(arr(item, "children"), &id, out, counter);
        }
    }
}

/// Flattens a v5 file into v4 resources.
fn v5_resources(file: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let workspace_id = file
        .get("meta")
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("__WORKSPACE_ID__")
        .to_string();
    let ty = s(file, "type");
    let scope = if ty.starts_with("environment.") {
        "environment"
    } else if ty.starts_with("mock.") {
        "mock-server"
    } else if ty.starts_with("mcpClient.") {
        "mcp"
    } else {
        "collection"
    };
    out.push(json!({
        "_id": workspace_id,
        "_type": "workspace",
        "parentId": null,
        "name": s(file, "name"),
        "description": file.get("meta").map(|m| s(m, "description")).unwrap_or(""),
        "scope": scope,
    }));
    let mut counter = 0;
    v5_items(
        arr(file, "collection"),
        &workspace_id,
        &mut out,
        &mut counter,
    );
    if let Some(env) = file.get("environments") {
        let base_id = env
            .get("meta")
            .and_then(|m| m.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("__BASE_ENV__")
            .to_string();
        out.push(json!({
            "_id": base_id,
            "_type": "environment",
            "parentId": workspace_id,
            "name": s(env, "name"),
            "data": env.get("data").cloned().unwrap_or(json!({})),
            "color": env.get("color").cloned().unwrap_or(Value::Null),
        }));
        for (i, sub) in arr(env, "subEnvironments").iter().enumerate() {
            let meta = sub.get("meta").cloned().unwrap_or(Value::Null);
            out.push(json!({
                "_id": meta.get("id").and_then(Value::as_str).map(str::to_string).unwrap_or(format!("__SUB_ENV_{i}")),
                "_type": "environment",
                "parentId": base_id,
                "name": s(sub, "name"),
                "data": sub.get("data").cloned().unwrap_or(json!({})),
                "color": sub.get("color").cloned().unwrap_or(Value::Null),
                "isPrivate": b(&meta, "isPrivate").unwrap_or(false),
                "metaSortKey": meta.get("sortKey").cloned().unwrap_or(json!(i)),
            }));
        }
    }
    if let Some(jar) = file.get("cookieJar") {
        out.push(json!({
            "_id": "__COOKIE_JAR__",
            "_type": "cookie_jar",
            "parentId": workspace_id,
            "cookies": jar.get("cookies").cloned().unwrap_or(json!([])),
        }));
    }
    if let Some(spec) = file.get("spec") {
        out.push(json!({
            "_id": "__SPEC__",
            "_type": "api_spec",
            "parentId": workspace_id,
            "contents": spec.get("contents").map(|c| c.to_string()).unwrap_or_default(),
        }));
    }
    for suite in arr(file, "testSuites") {
        out.push(json!({ "_id": "__SUITE__", "_type": "unit_test_suite", "parentId": workspace_id, "name": s(suite, "name") }));
    }
    for route in arr(file, "routes") {
        out.push(json!({ "_id": "__ROUTE__", "_type": "mock_route", "parentId": workspace_id, "name": s(route, "name") }));
    }
    out
}

// ----- resources → bundle ----------------------------------------------------------

struct Converter<'a> {
    resources: &'a [Value],
    report: ImportReport,
    known_tags: HashSet<String>,
}

fn body_kind(mime: &str) -> Option<BodyKind> {
    let mime = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    Some(match mime.as_str() {
        "" => return None,
        "application/x-www-form-urlencoded" => BodyKind::FormUrlencoded,
        "multipart/form-data" => BodyKind::Multipart,
        "application/octet-stream" => BodyKind::Binary,
        "text/plain" => BodyKind::Text,
        "text/html" => BodyKind::Html,
        "application/xml" | "text/xml" => BodyKind::Xml,
        "application/yaml" | "application/x-yaml" | "text/yaml" => BodyKind::Yaml,
        m if m.ends_with("json") || m.ends_with("+json") || m == "application/graphql" => {
            BodyKind::Json
        }
        m if m.ends_with("+xml") => BodyKind::Xml,
        _ => BodyKind::Text,
    })
}

impl<'a> Converter<'a> {
    fn children(&self, parent: &str) -> Vec<&'a Value> {
        let resources: &'a [Value] = self.resources;
        let mut out: Vec<&'a Value> = resources
            .iter()
            .filter(|r| s(r, "parentId") == parent)
            .collect();
        out.sort_by(|a, b| sort_key(a).total_cmp(&sort_key(b)));
        out
    }

    /// Reports template tags Reqly does not implement.
    fn check_tags(&mut self, path: &str, text: &str) {
        if !text.contains("{%") {
            return;
        }
        if let Ok(segments) = reqly_template::parse(text) {
            for seg in segments {
                if let reqly_template::ast::Segment::Expr {
                    expr: reqly_template::ast::Expr::Call { name, .. },
                    span,
                } = seg
                    && !self.known_tags.contains(&name)
                {
                    self.report.push(
                        ReportKind::Unsupported,
                        "tag_unsupported",
                        path,
                        format!("template tag `{name}` is not supported: {}", &text[span]),
                    );
                }
            }
        }
    }

    fn check_all_tags(&mut self, path: &str, value: &Value) {
        match value {
            Value::String(t) => self.check_tags(path, t),
            Value::Array(items) => items.iter().for_each(|v| self.check_all_tags(path, v)),
            Value::Object(map) => map.values().for_each(|v| self.check_all_tags(path, v)),
            _ => {}
        }
    }

    fn auth(&mut self, path: &str, raw: Option<&Value>, headers: &mut Vec<KeyValue>) -> Auth {
        let Some(raw) = raw.filter(|v| v.as_object().is_some_and(|o| !o.is_empty())) else {
            return Auth::Inherit;
        };
        let ty = s(raw, "type");
        if b(raw, "disabled") == Some(true) && ty != "none" {
            self.report.push_raw(
                ReportKind::Converted,
                "auth_disabled",
                path,
                format!("disabled `{ty}` authentication was imported as “No auth”"),
                raw.clone(),
            );
            return Auth::None;
        }
        match ty {
            "" => Auth::Inherit,
            "none" => Auth::None,
            "basic" => {
                if b(raw, "useISO88591") == Some(true) {
                    self.report.push(
                        ReportKind::Converted,
                        "auth_encoding",
                        path,
                        "Basic auth ISO-8859-1 encoding is not supported; UTF-8 is used",
                    );
                }
                Auth::Basic {
                    username: s(raw, "username").into(),
                    password: s(raw, "password").into(),
                }
            }
            "bearer" => Auth::Bearer {
                token: s(raw, "token").into(),
                prefix: s(raw, "prefix").into(),
            },
            "apikey" => match s(raw, "addTo") {
                "queryParams" => Auth::ApiKey {
                    key: s(raw, "key").into(),
                    value: s(raw, "value").into(),
                    location: ApiKeyLocation::Query,
                },
                "cookie" => {
                    headers.push(KeyValue::new(
                        "Cookie",
                        format!("{}={}", s(raw, "key"), s(raw, "value")),
                    ));
                    self.report.push(
                        ReportKind::Converted,
                        "auth_converted",
                        path,
                        "API key in a cookie was imported as a Cookie header",
                    );
                    Auth::None
                }
                _ => Auth::ApiKey {
                    key: s(raw, "key").into(),
                    value: s(raw, "value").into(),
                    location: ApiKeyLocation::Header,
                },
            },
            other => {
                let get = |k: &str| field_text(raw, k);
                if let Some(auth) =
                    crate::auth_map::from_insomnia(other, &crate::auth_map::Fields { get: &get })
                {
                    return auth;
                }
                self.report.push_raw(
                    ReportKind::Unsupported,
                    "auth_unsupported",
                    path,
                    format!("`{other}` authentication is not supported yet; the request was imported without it"),
                    raw.clone(),
                );
                Auth::None
            }
        }
    }

    fn scripts(&mut self, path: &str, r: &Value) -> Scripts {
        let scripts = Scripts {
            pre_request: s(r, "preRequestScript").into(),
            after_response: s(r, "afterResponseScript").into(),
        };
        if !scripts.is_empty() {
            self.report.push(
                ReportKind::Unsupported,
                "script_kept",
                path,
                "scripts were kept but are not executed (scripting is not available yet)",
            );
        }
        scripts
    }

    fn settings(&mut self, path: &str, r: &Value) -> RequestSettings {
        let mut settings = RequestSettings::default();
        match s(r, "settingFollowRedirects") {
            "on" => settings.follow_redirects = Some(true),
            "off" => settings.follow_redirects = Some(false),
            _ => {}
        }
        settings.encode_url = b(r, "settingEncodeUrl").filter(|v| !v);
        settings.send_cookies = b(r, "settingSendCookies").filter(|v| !v);
        settings.store_cookies = b(r, "settingStoreCookies").filter(|v| !v);
        if b(r, "settingDisableRenderRequestBody") == Some(true) {
            self.report.push(
                ReportKind::Converted,
                "render_body",
                path,
                "“do not render the body” is not supported; templates in the body are rendered",
            );
        }
        settings
    }

    fn body(&mut self, path: &str, raw: Option<&Value>) -> Body {
        let Some(raw) = raw else {
            return Body::default();
        };
        let mime = s(raw, "mimeType");
        let text = s(raw, "text");
        let kind = match body_kind(mime) {
            Some(k) => k,
            None if !text.is_empty() => BodyKind::Text,
            None => return Body::default(),
        };
        if mime == "application/graphql" {
            self.report.push(
                ReportKind::Converted,
                "graphql_as_json",
                path,
                "GraphQL body was imported as JSON (GraphQL requests arrive in stage 5)",
            );
        }
        let mut body = Body {
            kind,
            text: text.into(),
            ..Body::default()
        };
        match kind {
            BodyKind::FormUrlencoded | BodyKind::Multipart => {
                body.text.clear();
                body.fields = arr(raw, "params")
                    .iter()
                    .map(|p| {
                        let file = s(p, "type") == "file";
                        let mut row = KeyValue::new(
                            s(p, "name"),
                            if file {
                                s(p, "fileName")
                            } else {
                                s(p, "value")
                            },
                        );
                        row.enabled = !b(p, "disabled").unwrap_or(false);
                        row.description = s(p, "description").to_string();
                        if file && kind == BodyKind::Multipart {
                            row.kind = KeyValueKind::File;
                        }
                        row
                    })
                    .collect();
            }
            BodyKind::Binary => {
                let file = s(raw, "fileName");
                body.text.clear();
                body.file = (!file.is_empty()).then(|| FileRef::Path { path: file.into() });
            }
            _ => {}
        }
        body
    }

    fn request(&mut self, parent_path: &str, r: &Value) -> ImportedItem {
        let name = s(r, "name");
        let path = join_path(parent_path, name);
        self.check_all_tags(&path, r);
        let mut headers = rows(arr(r, "headers"));
        let auth = self.auth(&path, r.get("authentication"), &mut headers);
        let request = HttpRequest {
            method: s(r, "method").to_uppercase(),
            url: s(r, "url").into(),
            params: rows(arr(r, "parameters")),
            path_params: rows(arr(r, "pathParameters")),
            body: self.body(&path, r.get("body")),
            settings: self.settings(&path, r),
            scripts: self.scripts(&path, r),
            description: s(r, "description").into(),
            headers,
            auth,
            ..HttpRequest::default()
        };
        ImportedItem::request(name, request).with_id(s(r, "_id"))
    }

    fn folder(&mut self, parent_path: &str, r: &Value) -> ImportedItem {
        let name = s(r, "name");
        let path = join_path(parent_path, name);
        let mut headers = rows(arr(r, "headers"));
        let auth = self.auth(&path, r.get("authentication"), &mut headers);
        self.check_all_tags(&path, r.get("environment").unwrap_or(&Value::Null));
        let data = FolderData {
            headers,
            auth,
            description: s(r, "description").into(),
            variables: r
                .get("environment")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            scripts: self.scripts(&path, r),
            ..FolderData::default()
        };
        let id = s(r, "_id").to_string();
        let children = self.items(&path, &id);
        ImportedItem::folder(name, data, children).with_id(id)
    }

    fn items(&mut self, parent_path: &str, parent_id: &str) -> Vec<ImportedItem> {
        let mut out = Vec::new();
        for r in self.children(parent_id) {
            let name = s(r, "name");
            match s(r, "_type") {
                "request" => out.push(self.request(parent_path, r)),
                "request_group" => out.push(self.folder(parent_path, r)),
                "websocket_request" | "grpc_request" | "socketio_request" | "mcp_request" => {
                    let protocol = match s(r, "_type") {
                        "websocket_request" => "WebSocket",
                        "grpc_request" => "gRPC",
                        "socketio_request" => "Socket.IO",
                        _ => "MCP",
                    };
                    self.report.push_raw(
                        ReportKind::Unsupported,
                        "protocol_unsupported",
                        &join_path(parent_path, name),
                        format!("{protocol} requests are not supported yet"),
                        r.clone(),
                    );
                }
                "environment" | "cookie_jar" | "api_spec" | "proto_file" | "proto_directory" => {}
                other => self.report.push(
                    ReportKind::Unsupported,
                    "resource_unsupported",
                    &join_path(parent_path, name),
                    format!("`{other}` items are not supported"),
                ),
            }
        }
        out
    }

    fn environment(&mut self, r: &Value) -> ImportedEnvironment {
        self.check_all_tags(
            &format!("Environment {}", s(r, "name")),
            r.get("data").unwrap_or(&Value::Null),
        );
        ImportedEnvironment {
            name: s(r, "name").into(),
            color: r.get("color").and_then(Value::as_str).map(str::to_string),
            data: r
                .get("data")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            secrets: Vec::new(),
        }
    }

    fn cookies(&self, jar: &Value) -> Vec<ImportedCookie> {
        arr(jar, "cookies")
            .iter()
            .map(|c| {
                let expires = match c.get("expires") {
                    Some(Value::String(t)) => parse_rfc3339_secs(t),
                    Some(Value::Number(n)) => n.as_i64().map(|ms| ms / 1000),
                    _ => None,
                };
                ImportedCookie {
                    domain: s(c, "domain").trim_start_matches('.').into(),
                    include_subdomains: !b(c, "hostOnly").unwrap_or(false),
                    path: if s(c, "path").is_empty() {
                        "/".into()
                    } else {
                        s(c, "path").into()
                    },
                    secure: b(c, "secure").unwrap_or(false),
                    http_only: b(c, "httpOnly").unwrap_or(false),
                    expires,
                    name: s(c, "key").into(),
                    value: s(c, "value").into(),
                }
            })
            .collect()
    }

    fn workspace(&mut self, ws: &Value, bundle: &mut ImportBundle) {
        let resources: &'a [Value] = self.resources;
        let id = s(ws, "_id").to_string();
        let name = s(ws, "name");
        let scope = s(ws, "scope");
        let envs: Vec<&Value> = resources
            .iter()
            .filter(|r| s(r, "_type") == "environment" && s(r, "parentId") == id)
            .collect();
        let base = envs.first().copied();
        let mut subs: Vec<&Value> = base
            .map(|base| {
                resources
                    .iter()
                    .filter(|r| {
                        s(r, "_type") == "environment" && s(r, "parentId") == s(base, "_id")
                    })
                    .collect()
            })
            .unwrap_or_default();
        subs.sort_by(|a, b| sort_key(a).total_cmp(&sort_key(b)));

        for jar in resources
            .iter()
            .filter(|r| s(r, "_type") == "cookie_jar" && s(r, "parentId") == id)
        {
            bundle.cookies.extend(self.cookies(jar));
        }

        if scope == "environment" {
            // A global environment workspace: base + subs become global environments.
            if let Some(base) = base {
                let mut env = self.environment(base);
                if env.name.is_empty() {
                    env.name = name.into();
                }
                bundle.environments.push(env);
            }
            for sub in subs {
                bundle.environments.push(self.environment(sub));
            }
            return;
        }
        if scope == "mock-server" || scope == "mcp" {
            self.report.push(
                ReportKind::Unsupported,
                "workspace_unsupported",
                name,
                format!("`{scope}` workspaces are not supported"),
            );
            return;
        }
        if resources.iter().any(|r| {
            s(r, "parentId") == id
                && s(r, "_type") == "api_spec"
                && !s(r, "contents").trim().is_empty()
        }) {
            self.report.push(
                ReportKind::Info,
                "spec_skipped",
                name,
                "the API design document was not imported (import the spec as OpenAPI instead)",
            );
        }
        if resources
            .iter()
            .any(|r| s(r, "parentId") == id && s(r, "_type") == "unit_test_suite")
        {
            self.report.push(
                ReportKind::Unsupported,
                "tests_unsupported",
                name,
                "unit test suites are not supported yet",
            );
        }

        let items = self.items(name, &id);
        let mut collection = ImportedCollection {
            name: name.into(),
            description: s(ws, "description").into(),
            items,
            ..ImportedCollection::default()
        };
        if let Some(base) = base {
            collection.variables = self.environment(base).data;
        }
        for sub in subs {
            collection.environments.push(self.environment(sub));
        }
        bundle.collections.push(collection);
    }
}

/// `2030-01-01T00:00:00.000Z` → Unix seconds (UTC only, which is what Insomnia writes).
fn parse_rfc3339_secs(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let time = time.trim_end_matches('Z');
    let time = time.split(['+']).next()?;
    let mut t = time.split(':');
    let (h, min) = (
        t.next()?.parse::<i64>().ok()?,
        t.next()?.parse::<i64>().ok()?,
    );
    let sec = t
        .next()
        .and_then(|s| s.split('.').next()?.parse::<i64>().ok())
        .unwrap_or(0);
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + min * 60 + sec)
}

fn known_tags() -> HashSet<String> {
    let mut set: HashSet<String> = reqly_template::catalog()
        .into_iter()
        .map(|f| f.name)
        .collect();
    // Insomnia's `timestamp` is an alias of `now`; handled by the renderer aliases.
    set.insert("timestamp".into());
    set
}

fn convert(resources: &[Value], format: Format) -> ImportBundle {
    let mut converter = Converter {
        resources,
        report: ImportReport {
            format,
            entries: Vec::new(),
        },
        known_tags: known_tags(),
    };
    let mut bundle = ImportBundle {
        format,
        ..ImportBundle::default()
    };
    let workspaces: Vec<&Value> = resources
        .iter()
        .filter(|r| s(r, "_type") == "workspace")
        .collect();
    if workspaces.is_empty() {
        // Old exports without a workspace: everything hangs off unknown parents.
        let roots: HashSet<&str> = resources.iter().map(|r| s(r, "parentId")).collect();
        let ids: HashSet<&str> = resources.iter().map(|r| s(r, "_id")).collect();
        for root in roots.into_iter().filter(|p| !ids.contains(p)) {
            let ws = json!({ "_id": root, "name": "Insomnia", "scope": "collection" });
            converter.workspace(&ws, &mut bundle);
        }
    } else {
        for ws in workspaces {
            converter.workspace(ws, &mut bundle);
        }
    }
    bundle.report = converter.report;
    bundle
}

pub fn import(text: &str) -> CoreResult<ImportBundle> {
    let value = parse_structured(text)
        .ok_or_else(|| crate::invalid(Format::Insomnia, "not JSON or YAML"))?;
    if is_v5(&value) {
        let resources = v5_resources(&value);
        return Ok(convert(&resources, Format::Insomnia));
    }
    if is_v4(&value) {
        return Ok(convert(arr(&value, "resources"), Format::Insomnia));
    }
    Err(crate::invalid(Format::Insomnia, "not an Insomnia export"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rfc3339() {
        assert_eq!(
            super::parse_rfc3339_secs("1970-01-01T00:00:00.000Z"),
            Some(0)
        );
        assert_eq!(
            super::parse_rfc3339_secs("2030-01-01T00:00:00.000Z"),
            Some(1_893_456_000)
        );
    }
}
