//! OpenAPI 3.0 / 3.1 and Swagger 2.0 (JSON or YAML) import.
//!
//! Tags become folders, the first server becomes the `base_url` variable,
//! parameters and request bodies get example values (from `example`,
//! `default`, `enum` or generated from the schema), and security schemes
//! become auth with variables to fill in.

use std::collections::HashSet;

use reqly_core::CoreResult;
use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FolderData, HttpRequest, KeyValue, KeyValueKind,
};
use serde_json::{Map, Value, json};

use crate::detect::{Format, parse_structured};
use crate::model::{ImportBundle, ImportedCollection, ImportedItem};
use crate::report::{ImportReport, ReportKind};

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
const MAX_DEPTH: usize = 8;

pub fn is_openapi(value: &Value) -> bool {
    value
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|v| v.starts_with('3'))
        || value
            .get("swagger")
            .and_then(Value::as_str)
            .is_some_and(|v| v.starts_with('2'))
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

struct Spec<'a> {
    root: &'a Value,
    swagger: bool,
    report: ImportReport,
    variables: Map<String, Value>,
}

impl<'a> Spec<'a> {
    /// Follows local `$ref`s (`#/components/...`, `#/definitions/...`).
    fn deref<'v>(&'v self, value: &'v Value) -> &'v Value
    where
        'a: 'v,
    {
        let mut current = value;
        for _ in 0..16 {
            let Some(r) = current.get("$ref").and_then(Value::as_str) else {
                return current;
            };
            let Some(pointer) = r.strip_prefix('#') else {
                return current;
            };
            match self.root.pointer(pointer) {
                Some(target) => current = target,
                None => return current,
            }
        }
        current
    }

    /// An example value for a schema.
    fn example(&self, schema: &Value, depth: usize, seen: &mut HashSet<String>) -> Value {
        if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
            if !seen.insert(r.to_string()) || depth > MAX_DEPTH {
                return Value::Null;
            }
            let out = self.example(self.deref(schema), depth + 1, seen);
            seen.remove(r);
            return out;
        }
        if let Some(e) = schema.get("example") {
            return e.clone();
        }
        if let Some(Value::Array(examples)) = schema.get("examples")
            && let Some(first) = examples.first()
        {
            return first.clone();
        }
        if let Some(d) = schema.get("default") {
            return d.clone();
        }
        if let Some(Value::Array(e)) = schema.get("enum")
            && let Some(first) = e.first()
        {
            return first.clone();
        }
        if let Some(c) = schema.get("const") {
            return c.clone();
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(first) = arr(schema, key).first() {
                return self.example(first, depth + 1, seen);
            }
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            for part in all {
                if let Value::Object(o) = self.example(part, depth + 1, seen) {
                    merged.extend(o);
                }
            }
            return Value::Object(merged);
        }
        let ty = match schema.get("type") {
            Some(Value::String(t)) => t.as_str(),
            // 3.1: `type: [string, "null"]`
            Some(Value::Array(types)) => types
                .iter()
                .filter_map(Value::as_str)
                .find(|t| *t != "null")
                .unwrap_or("string"),
            _ if schema.get("properties").is_some() => "object",
            _ if schema.get("items").is_some() => "array",
            _ => "",
        };
        match ty {
            "object" => {
                let mut out = Map::new();
                if depth < MAX_DEPTH
                    && let Some(Value::Object(props)) = schema.get("properties")
                {
                    for (k, v) in props {
                        out.insert(k.clone(), self.example(v, depth + 1, seen));
                    }
                }
                Value::Object(out)
            }
            "array" => match schema.get("items") {
                Some(items) if depth < MAX_DEPTH => json!([self.example(items, depth + 1, seen)]),
                _ => json!([]),
            },
            "integer" => json!(0),
            "number" => json!(0.0),
            "boolean" => json!(true),
            "string" => json!(match s(schema, "format") {
                "date-time" => "2024-01-01T00:00:00Z",
                "date" => "2024-01-01",
                "email" => "user@example.com",
                "uuid" => "00000000-0000-0000-0000-000000000000",
                "uri" | "url" => "https://example.com",
                "binary" | "byte" => "",
                _ => "string",
            }),
            _ => Value::Null,
        }
    }

    fn sample_text(&self, schema: Option<&Value>) -> String {
        match schema.map(|s| self.example(s, 0, &mut HashSet::new())) {
            Some(Value::String(s)) => s,
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        }
    }

    fn param_value(&self, p: &Value) -> String {
        if let Some(e) = p.get("example") {
            return match e {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
        }
        if let Some(Value::Object(examples)) = p.get("examples")
            && let Some((_, first)) = examples.iter().next()
        {
            let first = self.deref(first);
            if let Some(v) = first.get("value") {
                return match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
            }
        }
        // Swagger 2: type/default on the parameter itself.
        let schema = p.get("schema").unwrap_or(p);
        match self.example(schema, 0, &mut HashSet::new()) {
            Value::String(s) if s == "string" => String::new(),
            Value::String(s) => s,
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    fn base_url(&mut self) -> String {
        if self.swagger {
            let host = s(self.root, "host");
            let base = s(self.root, "basePath");
            let scheme = arr(self.root, "schemes")
                .first()
                .and_then(Value::as_str)
                .unwrap_or("https");
            return if host.is_empty() {
                base.to_string()
            } else {
                format!("{scheme}://{host}{base}")
            };
        }
        let Some(server) = arr(self.root, "servers").first() else {
            return String::new();
        };
        let mut url = s(server, "url").to_string();
        if let Some(Value::Object(vars)) = server.get("variables") {
            for (name, var) in vars {
                // Server variables become environment variables.
                self.variables
                    .insert(name.clone(), json!(s(var, "default")));
                url = url.replace(&format!("{{{name}}}"), &format!("{{{{ {name} }}}}"));
            }
        }
        url.trim_end_matches('/').to_string()
    }

    fn security_auth(&mut self, path: &str, requirements: Option<&Value>) -> Auth {
        let Some(Value::Array(reqs)) = requirements else {
            return Auth::Inherit;
        };
        if reqs.is_empty()
            || reqs
                .iter()
                .all(|r| r.as_object().is_some_and(|o| o.is_empty()))
        {
            return Auth::None;
        }
        let schemes = if self.swagger {
            self.root.get("securityDefinitions")
        } else {
            self.root
                .get("components")
                .and_then(|c| c.get("securitySchemes"))
        }
        .cloned()
        .unwrap_or(Value::Null);
        for req in reqs {
            let Some(name) = req.as_object().and_then(|o| o.keys().next()) else {
                continue;
            };
            let Some(scheme) = schemes.get(name) else {
                continue;
            };
            let scheme = self.deref(scheme).clone();
            let ty = s(&scheme, "type");
            let http_scheme = s(&scheme, "scheme").to_ascii_lowercase();
            match (ty, http_scheme.as_str()) {
                ("basic", _) | ("http", "basic") => {
                    self.variables.entry("username").or_insert(json!(""));
                    self.variables.entry("password").or_insert(json!(""));
                    return Auth::Basic {
                        username: "{{ username }}".into(),
                        password: "{{ password }}".into(),
                    };
                }
                ("http", "bearer") => {
                    self.variables.entry("bearer_token").or_insert(json!(""));
                    return Auth::Bearer {
                        token: "{{ bearer_token }}".into(),
                        prefix: String::new(),
                    };
                }
                ("apiKey", _) => {
                    let var = format!("{}_key", crate::file_stem(name).replace('-', "_"));
                    self.variables.entry(var.clone()).or_insert(json!(""));
                    let location = match s(&scheme, "in") {
                        "query" => ApiKeyLocation::Query,
                        "header" => ApiKeyLocation::Header,
                        other => {
                            self.report.push(
                                ReportKind::Converted,
                                "api_key_cookie",
                                path,
                                format!("API key in `{other}` was imported as a header"),
                            );
                            ApiKeyLocation::Header
                        }
                    };
                    return Auth::ApiKey {
                        key: s(&scheme, "name").into(),
                        value: format!("{{{{ {var} }}}}"),
                        location,
                    };
                }
                _ => {}
            }
            self.report.push_raw(
                ReportKind::Unsupported,
                "auth_unsupported",
                path,
                format!("`{name}` ({ty}) security is not supported yet"),
                scheme.clone(),
            );
        }
        Auth::None
    }

    fn body_v3(&mut self, path: &str, op: &Value, headers: &mut Vec<KeyValue>) -> Body {
        let Some(body) = op.get("requestBody").map(|b| self.deref(b).clone()) else {
            return Body::default();
        };
        let Some(Value::Object(content)) = body.get("content") else {
            return Body::default();
        };
        let Some((mime, media)) = content
            .iter()
            .find(|(m, _)| m.contains("json"))
            .or_else(|| content.iter().next())
        else {
            return Body::default();
        };
        let media = media.clone();
        let example = media.get("example").cloned().or_else(|| {
            media
                .get("examples")
                .and_then(Value::as_object)
                .and_then(|e| e.values().next())
                .map(|e| self.deref(e).get("value").cloned().unwrap_or(Value::Null))
        });
        let schema = media.get("schema").map(|s| self.deref(s).clone());
        self.body_from(path, mime, example, schema.as_ref(), headers)
    }

    fn body_from(
        &mut self,
        path: &str,
        mime: &str,
        example: Option<Value>,
        schema: Option<&Value>,
        headers: &mut Vec<KeyValue>,
    ) -> Body {
        let mime_l = mime.to_ascii_lowercase();
        let fields = |spec: &Self| -> Vec<KeyValue> {
            let props = schema
                .and_then(|s| s.get("properties"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            props
                .iter()
                .map(|(name, prop)| {
                    let prop = spec.deref(prop);
                    let binary =
                        matches!(s(prop, "format"), "binary" | "byte") || s(prop, "type") == "file";
                    let mut row = KeyValue::new(
                        name.clone(),
                        if binary {
                            String::new()
                        } else {
                            spec.sample_text(Some(prop))
                        },
                    );
                    if binary && mime_l.starts_with("multipart") {
                        row.kind = KeyValueKind::File;
                    }
                    row
                })
                .collect()
        };
        if mime_l == "application/x-www-form-urlencoded" {
            return Body {
                kind: BodyKind::FormUrlencoded,
                fields: fields(self),
                ..Body::default()
            };
        }
        if mime_l.starts_with("multipart/") {
            return Body {
                kind: BodyKind::Multipart,
                fields: fields(self),
                ..Body::default()
            };
        }
        if mime_l == "application/octet-stream" {
            return Body {
                kind: BodyKind::Binary,
                ..Body::default()
            };
        }
        let value = example.unwrap_or_else(|| {
            schema
                .map(|s| self.example(s, 0, &mut HashSet::new()))
                .unwrap_or(Value::Null)
        });
        let kind = if mime_l.contains("json") {
            BodyKind::Json
        } else if mime_l.contains("xml") {
            BodyKind::Xml
        } else if mime_l.contains("yaml") {
            BodyKind::Yaml
        } else if mime_l.contains("html") {
            BodyKind::Html
        } else {
            BodyKind::Text
        };
        if kind != BodyKind::Json && !mime_l.is_empty() {
            headers.push(KeyValue::new("Content-Type", mime));
        }
        let text = match (&kind, &value) {
            (_, Value::Null) => String::new(),
            (BodyKind::Json, v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            (_, Value::String(s)) => s.clone(),
            (_, v) => v.to_string(),
        };
        if kind == BodyKind::Xml && text.is_empty() {
            self.report.push(
                ReportKind::Info,
                "xml_example",
                path,
                "no XML example; the body is empty",
            );
        }
        Body {
            kind,
            text,
            ..Body::default()
        }
    }

    fn operation(
        &mut self,
        url_path: &str,
        method: &str,
        op: &Value,
        path_item: &Value,
        base: &str,
    ) -> ImportedItem {
        let name = [s(op, "summary"), s(op, "operationId")]
            .into_iter()
            .find(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} {url_path}", method.to_uppercase()));
        let report_path = format!("{} {url_path}", method.to_uppercase());
        let mut params = Vec::new();
        let mut headers = Vec::new();
        let mut path_params = Vec::new();
        let mut cookies = Vec::new();
        let mut form = Vec::new();
        let mut swagger_body: Option<Value> = None;
        // Operation parameters override path-level ones with the same name+in.
        let mut all: Vec<Value> = Vec::new();
        for p in arr(path_item, "parameters")
            .iter()
            .chain(arr(op, "parameters"))
        {
            let p = self.deref(p).clone();
            all.retain(|q| !(s(q, "name") == s(&p, "name") && s(q, "in") == s(&p, "in")));
            all.push(p);
        }
        for p in &all {
            let name = s(p, "name");
            let value = self.param_value(p);
            let mut row = KeyValue::new(name, value);
            row.description = s(p, "description").into();
            if !p.get("required").and_then(Value::as_bool).unwrap_or(false) && s(p, "in") == "query"
            {
                row.enabled = false;
            }
            match s(p, "in") {
                "query" => params.push(row),
                "header" => headers.push(row),
                "path" => path_params.push(row),
                "cookie" => cookies.push(format!("{name}={}", row.value)),
                "body" => swagger_body = p.get("schema").cloned(),
                "formData" => {
                    if s(p, "type") == "file" {
                        row.kind = KeyValueKind::File;
                        row.value.clear();
                    }
                    row.enabled = true;
                    form.push(row);
                }
                _ => {}
            }
        }
        if !cookies.is_empty() {
            headers.push(KeyValue::new("Cookie", cookies.join("; ")));
        }
        let body = if self.swagger {
            let consumes = arr(op, "consumes")
                .first()
                .or_else(|| arr(self.root, "consumes").first())
                .and_then(Value::as_str)
                .unwrap_or("application/json")
                .to_string();
            if !form.is_empty() {
                let kind = if consumes.starts_with("multipart")
                    || form.iter().any(|f| f.kind == KeyValueKind::File)
                {
                    BodyKind::Multipart
                } else {
                    BodyKind::FormUrlencoded
                };
                Body {
                    kind,
                    fields: form,
                    ..Body::default()
                }
            } else if let Some(schema) = swagger_body {
                let schema = self.deref(&schema).clone();
                self.body_from(&report_path, &consumes, None, Some(&schema), &mut headers)
            } else {
                Body::default()
            }
        } else {
            self.body_v3(&report_path, op, &mut headers)
        };
        // `{id}` → `:id`
        let mut url = url_path.to_string();
        for p in &path_params {
            url = url.replace(&format!("{{{}}}", p.name), &format!(":{}", p.name));
        }
        let auth = self.security_auth(&report_path, op.get("security"));
        let description = s(op, "description").to_string();
        let request = HttpRequest {
            method: method.to_uppercase(),
            url: format!("{base}{url}"),
            params,
            path_params,
            headers,
            body,
            auth,
            description,
            ..HttpRequest::default()
        };
        let mut item = ImportedItem::request(name, request);
        if !s(op, "operationId").is_empty() {
            item = item.with_id(s(op, "operationId"));
        }
        item
    }
}

pub fn import(text: &str) -> CoreResult<ImportBundle> {
    let root = parse_structured(text)
        .ok_or_else(|| crate::invalid(Format::OpenApi, "not JSON or YAML"))?;
    if !is_openapi(&root) {
        return Err(crate::invalid(
            Format::OpenApi,
            "not an OpenAPI or Swagger document",
        ));
    }
    let mut spec = Spec {
        root: &root,
        swagger: root.get("swagger").is_some(),
        report: ImportReport {
            format: Format::OpenApi,
            entries: Vec::new(),
        },
        variables: Map::new(),
    };
    let base = spec.base_url();
    spec.variables.insert("base_url".into(), json!(base));
    let global_auth = spec.security_auth("", root.get("security"));

    let info = root.get("info").cloned().unwrap_or(Value::Null);
    let title = if s(&info, "title").is_empty() {
        "OpenAPI import".to_string()
    } else {
        s(&info, "title").to_string()
    };

    // Folders in the order tags are declared, then in order of first use.
    let mut folder_names: Vec<String> = arr(&root, "tags")
        .iter()
        .map(|t| s(t, "name").to_string())
        .collect();
    let mut tagged: Vec<(String, ImportedItem)> = Vec::new();
    let mut untagged = Vec::new();
    if let Some(Value::Object(paths)) = root.get("paths") {
        for (url_path, item) in paths {
            let item = spec.deref(item).clone();
            for method in METHODS {
                let Some(op) = item.get(method) else { continue };
                let imported = spec.operation(url_path, method, op, &item, "{{ base_url }}");
                match op
                    .get("tags")
                    .and_then(Value::as_array)
                    .and_then(|t| t.first())
                    .and_then(Value::as_str)
                {
                    Some(tag) => {
                        if !folder_names.iter().any(|f| f == tag) {
                            folder_names.push(tag.to_string());
                        }
                        tagged.push((tag.to_string(), imported));
                    }
                    None => untagged.push(imported),
                }
            }
            for key in item
                .as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                if key.starts_with("x-") && key != "x-summary" {
                    spec.report.push(
                        ReportKind::Info,
                        "extension_skipped",
                        url_path,
                        format!("extension `{key}` was ignored"),
                    );
                }
            }
        }
    }
    let mut items = Vec::new();
    for name in folder_names {
        let children: Vec<ImportedItem> = tagged
            .iter()
            .filter(|(t, _)| *t == name)
            .map(|(_, i)| i.clone())
            .collect();
        if children.is_empty() {
            continue;
        }
        let description = arr(&root, "tags")
            .iter()
            .find(|t| s(t, "name") == name)
            .map(|t| s(t, "description").to_string())
            .unwrap_or_default();
        items.push(ImportedItem::folder(
            name,
            FolderData {
                description,
                ..FolderData::default()
            },
            children,
        ));
    }
    items.extend(untagged);

    // Global security applies to everything that does not override it.
    if global_auth != Auth::Inherit {
        fn apply(items: &mut [ImportedItem], auth: &Auth) {
            for i in items {
                match i {
                    ImportedItem::Folder { children, .. } => apply(children, auth),
                    ImportedItem::Request { data, .. } if data.auth == Auth::Inherit => {
                        data.auth = auth.clone()
                    }
                    _ => {}
                }
            }
        }
        apply(&mut items, &global_auth);
    }

    let mut description = s(&info, "description").to_string();
    if !s(&info, "version").is_empty() {
        if !description.is_empty() {
            description.push_str("\n\n");
        }
        description.push_str(&format!("Version: {}", s(&info, "version")));
    }
    let collection = ImportedCollection {
        name: title,
        description,
        variables: spec.variables.clone(),
        items,
        ..ImportedCollection::default()
    };
    Ok(ImportBundle {
        format: Format::OpenApi,
        collections: vec![collection],
        report: spec.report,
        ..ImportBundle::default()
    })
}
