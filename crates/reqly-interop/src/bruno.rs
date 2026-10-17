//! Bruno collections: `.bru` files in a directory (or zip) with
//! `bruno.json`, `collection.bru`, `folder.bru` and `environments/*.bru`.

use std::collections::BTreeMap;

use reqly_core::CoreResult;
use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind,
    Scripts,
};
use serde_json::{Map, Value};

use crate::Source;
use crate::detect::Format;
use crate::model::{ImportBundle, ImportedCollection, ImportedEnvironment, ImportedItem};
use crate::postman::vars_to_reqly;
use crate::report::{ImportReport, ReportKind, join_path};

// ----- .bru parsing ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Dict(Vec<Pair>),
    Text(String),
    List(Vec<String>),
}

/// Blocks whose content is free text.
fn is_text_block(name: &str) -> bool {
    name.starts_with("body")
        && name != "body:form-urlencoded"
        && name != "body:multipart-form"
        && name != "body:file"
        || name.starts_with("script:")
        || name == "tests"
        || name == "docs"
}

fn unquote_key(key: &str) -> String {
    let key = key.trim();
    if let Some(inner) = key.strip_prefix('"').and_then(|k| k.strip_suffix('"')) {
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        key.to_string()
    }
}

/// Splits `key: value` honouring quoted keys.
fn split_pair(line: &str) -> Option<(String, String)> {
    if let Some(rest) = line.strip_prefix('"') {
        let mut escaped = false;
        for (i, c) in rest.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => {
                    let key = format!("\"{}\"", &rest[..i]);
                    let after = rest[i + 1..].trim_start();
                    let value = after
                        .strip_prefix(':')?
                        .strip_prefix(' ')
                        .unwrap_or(after.strip_prefix(':')?);
                    return Some((unquote_key(&key), value.to_string()));
                }
                _ => escaped = false,
            }
        }
        return None;
    }
    let (k, v) = line.split_once(':')?;
    Some((
        k.trim().to_string(),
        v.strip_prefix(' ').unwrap_or(v).to_string(),
    ))
}

/// Parses a `.bru` document into named blocks (repeated names keep the last).
pub fn parse(text: &str) -> BTreeMap<String, Block> {
    let mut blocks = BTreeMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        let header = line.trim();
        if header.is_empty() {
            i += 1;
            continue;
        }
        let (name, open) = if let Some(n) = header.strip_suffix('{') {
            (n.trim().to_string(), '{')
        } else if let Some(n) = header.strip_suffix('[') {
            (n.trim().to_string(), '[')
        } else {
            i += 1;
            continue;
        };
        let close = if open == '{' { "}" } else { "]" };
        i += 1;
        let mut body: Vec<&str> = Vec::new();
        while i < lines.len() && lines[i].trim_end() != close {
            body.push(lines[i]);
            i += 1;
        }
        i += 1; // closing line
        let block = if open == '[' {
            Block::List(
                body.iter()
                    .flat_map(|l| l.split(','))
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            )
        } else if is_text_block(&name) {
            Block::Text(
                body.iter()
                    .map(|l| l.strip_prefix("  ").unwrap_or(l.trim_start()))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            let mut pairs = Vec::new();
            let mut j = 0;
            while j < body.len() {
                let raw = body[j].trim();
                j += 1;
                if raw.is_empty() || raw.starts_with('@') {
                    continue;
                }
                let (enabled, raw) = match raw.strip_prefix('~') {
                    Some(r) => (false, r),
                    None => (true, raw),
                };
                let Some((key, mut value)) = split_pair(raw) else {
                    continue;
                };
                if value.trim() == "'''" {
                    let mut multi = Vec::new();
                    while j < body.len() && body[j].trim() != "'''" {
                        multi.push(body[j].trim_start());
                        j += 1;
                    }
                    j += 1;
                    value = multi.join("\n");
                }
                pairs.push(Pair {
                    key,
                    value,
                    enabled,
                });
            }
            Block::Dict(pairs)
        };
        blocks.insert(name, block);
    }
    blocks
}

fn dict<'a>(blocks: &'a BTreeMap<String, Block>, name: &str) -> &'a [Pair] {
    match blocks.get(name) {
        Some(Block::Dict(pairs)) => pairs,
        _ => &[],
    }
}

fn get<'a>(blocks: &'a BTreeMap<String, Block>, block: &str, key: &str) -> &'a str {
    dict(blocks, block)
        .iter()
        .find(|p| p.key == key)
        .map(|p| p.value.as_str())
        .unwrap_or("")
}

fn text<'a>(blocks: &'a BTreeMap<String, Block>, name: &str) -> &'a str {
    match blocks.get(name) {
        Some(Block::Text(t)) => t.as_str(),
        _ => "",
    }
}

fn rows(pairs: &[Pair]) -> Vec<KeyValue> {
    pairs
        .iter()
        .map(|p| {
            let mut row = KeyValue::new(vars_to_reqly(&p.key), vars_to_reqly(&p.value));
            row.enabled = p.enabled;
            row
        })
        .collect()
}

// ----- conversion ----------------------------------------------------------------------

const METHODS: [&str; 9] = [
    "get", "post", "put", "delete", "patch", "options", "head", "connect", "trace",
];

struct Importer {
    report: ImportReport,
}

impl Importer {
    fn auth(&mut self, path: &str, blocks: &BTreeMap<String, Block>, mode: &str) -> Auth {
        let v = |k: &str| vars_to_reqly(get(blocks, &format!("auth:{mode}"), k));
        match mode {
            "" | "inherit" => Auth::Inherit,
            "none" => Auth::None,
            "basic" => Auth::Basic {
                username: v("username"),
                password: v("password"),
            },
            "bearer" => Auth::Bearer {
                token: v("token"),
                prefix: String::new(),
            },
            "apikey" => Auth::ApiKey {
                key: v("key"),
                value: v("value"),
                location: if v("placement").eq_ignore_ascii_case("queryparams") {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            },
            other => {
                let raw: Map<String, Value> = dict(blocks, &format!("auth:{other}"))
                    .iter()
                    .map(|p| (p.key.clone(), Value::String(p.value.clone())))
                    .collect();
                self.report.push_raw(
                    ReportKind::Unsupported,
                    "auth_unsupported",
                    path,
                    format!("`{other}` authentication is not supported yet; the request was imported without it"),
                    Value::Object(raw),
                );
                Auth::None
            }
        }
    }

    fn scripts(&mut self, path: &str, blocks: &BTreeMap<String, Block>) -> Scripts {
        let mut after = text(blocks, "script:post-response").to_string();
        let tests = text(blocks, "tests");
        if !tests.trim().is_empty() {
            if !after.is_empty() {
                after.push_str("\n\n");
            }
            after.push_str("// tests\n");
            after.push_str(tests);
        }
        let scripts = Scripts {
            pre_request: text(blocks, "script:pre-request").into(),
            after_response: after,
        };
        if !scripts.is_empty() {
            self.report.push(
                ReportKind::Unsupported,
                "script_kept",
                path,
                "scripts and tests were kept but are not executed yet",
            );
        }
        if !dict(blocks, "assert").is_empty() {
            self.report.push(
                ReportKind::Unsupported,
                "assert_unsupported",
                path,
                "assertions are not supported yet",
            );
        }
        if dict(blocks, "vars:post-response").iter().any(|p| p.enabled) {
            self.report.push(
                ReportKind::Unsupported,
                "post_vars_unsupported",
                path,
                "post-response variables need scripting and were not imported",
            );
        }
        scripts
    }

    fn body(&mut self, path: &str, blocks: &BTreeMap<String, Block>, mode: &str) -> Body {
        let text_body = |kind: BodyKind, name: &str| Body {
            kind,
            text: vars_to_reqly(text(blocks, name)),
            ..Body::default()
        };
        match mode {
            "" | "none" => Body::default(),
            "json" => text_body(BodyKind::Json, "body:json"),
            "text" => text_body(BodyKind::Text, "body:text"),
            "xml" => text_body(BodyKind::Xml, "body:xml"),
            "sparql" => {
                self.report.push(
                    ReportKind::Converted,
                    "sparql_as_text",
                    path,
                    "SPARQL body was imported as text",
                );
                text_body(BodyKind::Text, "body:sparql")
            }
            "formUrlEncoded" => Body {
                kind: BodyKind::FormUrlencoded,
                fields: rows(dict(blocks, "body:form-urlencoded")),
                ..Body::default()
            },
            "multipartForm" => Body {
                kind: BodyKind::Multipart,
                fields: dict(blocks, "body:multipart-form")
                    .iter()
                    .map(|p| {
                        let mut row = KeyValue::new(vars_to_reqly(&p.key), vars_to_reqly(&p.value));
                        row.enabled = p.enabled;
                        if let Some(file) = p
                            .value
                            .strip_prefix("@file(")
                            .and_then(|f| f.split(')').next())
                        {
                            row.kind = KeyValueKind::File;
                            row.value = file.split('|').next().unwrap_or(file).to_string();
                        }
                        row
                    })
                    .collect(),
                ..Body::default()
            },
            "file" => {
                let file = dict(blocks, "body:file")
                    .iter()
                    .find(|p| p.enabled)
                    .and_then(|p| {
                        p.value
                            .strip_prefix("@file(")
                            .and_then(|f| f.split(')').next())
                    })
                    .unwrap_or("");
                Body {
                    kind: BodyKind::Binary,
                    file: (!file.is_empty()).then(|| FileRef::Path { path: file.into() }),
                    ..Body::default()
                }
            }
            "graphql" => {
                let vars: Value = serde_json::from_str(text(blocks, "body:graphql:vars"))
                    .unwrap_or(Value::Object(Map::new()));
                self.report.push(
                    ReportKind::Converted,
                    "graphql_as_json",
                    path,
                    "GraphQL body was imported as JSON (GraphQL requests arrive in stage 5)",
                );
                let json =
                    serde_json::json!({ "query": text(blocks, "body:graphql"), "variables": vars });
                Body {
                    kind: BodyKind::Json,
                    text: vars_to_reqly(&serde_json::to_string_pretty(&json).unwrap_or_default()),
                    ..Body::default()
                }
            }
            other => {
                self.report.push(
                    ReportKind::Unsupported,
                    "body_unsupported",
                    path,
                    format!("`{other}` bodies are not supported"),
                );
                Body::default()
            }
        }
    }

    fn variables(blocks: &BTreeMap<String, Block>) -> Map<String, Value> {
        dict(blocks, "vars:pre-request")
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.key.clone(), Value::String(vars_to_reqly(&p.value))))
            .collect()
    }

    /// A request file; `None` for non-HTTP requests (reported).
    fn request(
        &mut self,
        parent: &str,
        file_name: &str,
        content: &str,
    ) -> Option<(f64, ImportedItem)> {
        let blocks = parse(content);
        let name = match get(&blocks, "meta", "name") {
            "" => file_name.trim_end_matches(".bru").to_string(),
            n => n.to_string(),
        };
        let path = join_path(parent, &name);
        let seq: f64 = get(&blocks, "meta", "seq").parse().unwrap_or(f64::MAX);
        let ty = get(&blocks, "meta", "type");
        if !matches!(ty, "" | "http" | "graphql") {
            self.report.push(
                ReportKind::Unsupported,
                "protocol_unsupported",
                &path,
                format!("`{ty}` requests are not supported yet"),
            );
            return None;
        }
        let (method, block) = METHODS
            .iter()
            .find(|m| blocks.contains_key(**m))
            .map(|m| (m.to_uppercase(), m.to_string()))
            .or_else(|| {
                blocks.contains_key("http").then(|| {
                    (
                        get(&blocks, "http", "method").to_uppercase(),
                        "http".to_string(),
                    )
                })
            })?;
        let raw_url = vars_to_reqly(get(&blocks, &block, "url"));
        let (url, query_from_url) = match raw_url.split_once('?') {
            Some((u, q)) => (u.to_string(), q.to_string()),
            None => (raw_url.clone(), String::new()),
        };
        let params = if blocks.contains_key("params:query") {
            rows(dict(&blocks, "params:query"))
        } else {
            query_from_url
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    KeyValue::new(k, v)
                })
                .collect()
        };
        let mut settings = reqly_core::model::RequestSettings::default();
        if get(&blocks, "settings", "encodeUrl") == "false" {
            settings.encode_url = Some(false);
        }
        let request = HttpRequest {
            method,
            url,
            params,
            path_params: rows(dict(&blocks, "params:path")),
            headers: rows(dict(&blocks, "headers")),
            body: self.body(&path, &blocks, get(&blocks, &block, "body")),
            auth: self.auth(&path, &blocks, get(&blocks, &block, "auth")),
            settings,
            description: text(&blocks, "docs").into(),
            variables: Self::variables(&blocks),
            scripts: self.scripts(&path, &blocks),
        };
        Some((seq, ImportedItem::request(name, request)))
    }
}

/// A directory tree of sources keyed by path segments.
#[derive(Default)]
struct Dir<'a> {
    files: Vec<(&'a str, &'a str)>,
    dirs: BTreeMap<String, Dir<'a>>,
}

impl Importer {
    fn folder(&mut self, parent: &str, dir_name: &str, dir: &Dir<'_>) -> (f64, ImportedItem) {
        let folder_bru = dir
            .files
            .iter()
            .find(|(n, _)| *n == "folder.bru")
            .map(|(_, c)| parse(c));
        let empty = BTreeMap::new();
        let blocks = folder_bru.as_ref().unwrap_or(&empty);
        let name = match get(blocks, "meta", "name") {
            "" => dir_name.to_string(),
            n => n.to_string(),
        };
        let path = join_path(parent, &name);
        let seq = get(blocks, "meta", "seq").parse().unwrap_or(f64::MAX);
        let data = FolderData {
            headers: rows(dict(blocks, "headers")),
            auth: self.auth(&path, blocks, get(blocks, "auth", "mode")),
            description: text(blocks, "docs").into(),
            variables: Self::variables(blocks),
            scripts: self.scripts(&path, blocks),
            ..FolderData::default()
        };
        let children = self.children(&path, dir);
        (seq, ImportedItem::folder(name, data, children))
    }

    fn children(&mut self, path: &str, dir: &Dir<'_>) -> Vec<ImportedItem> {
        let mut items: Vec<(f64, String, ImportedItem)> = Vec::new();
        for (name, content) in &dir.files {
            if !name.ends_with(".bru") || matches!(*name, "folder.bru" | "collection.bru") {
                continue;
            }
            if let Some((seq, item)) = self.request(path, name, content) {
                items.push((seq, name.to_string(), item));
            }
        }
        for (name, sub) in &dir.dirs {
            if name == "environments" && path.is_empty() {
                continue;
            }
            let (seq, item) = self.folder(path, name, sub);
            items.push((seq, name.clone(), item));
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        items.into_iter().map(|(_, _, i)| i).collect()
    }
}

pub fn is_bruno(sources: &[Source]) -> bool {
    sources
        .iter()
        .any(|s| s.name.ends_with(".bru") || s.name.rsplit('/').next() == Some("bruno.json"))
        || sources.first().is_some_and(|s| {
            let t = s.content.trim_start();
            t.starts_with("meta {")
                && (t.contains("\nget {") || t.contains("\npost {") || t.contains("\nhttp {"))
        })
}

pub fn import(sources: &[Source]) -> CoreResult<ImportBundle> {
    // The collection root is the directory holding bruno.json (or the common prefix).
    let root = sources
        .iter()
        .find(|s| s.name.rsplit('/').next() == Some("bruno.json"))
        .map(|s| s.name.trim_end_matches("bruno.json").to_string())
        .unwrap_or_else(|| {
            let dirs: Vec<&str> = sources
                .iter()
                .map(|s| s.name.rsplit_once('/').map_or("", |(d, _)| d))
                .collect();
            let first = dirs.first().copied().unwrap_or("");
            let mut prefix = first.to_string();
            for d in &dirs {
                while !d.starts_with(&prefix) {
                    prefix = prefix
                        .rsplit_once('/')
                        .map_or(String::new(), |(p, _)| p.to_string());
                }
            }
            if prefix.is_empty() {
                prefix
            } else {
                format!("{prefix}/")
            }
        });
    let mut tree = Dir::default();
    for s in sources {
        let Some(rel) = s.name.strip_prefix(&root) else {
            continue;
        };
        let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        let Some(file) = parts.pop() else { continue };
        let mut dir = &mut tree;
        for p in parts {
            dir = dir.dirs.entry(p.to_string()).or_default();
        }
        dir.files.push((file, s.content.as_str()));
    }

    let mut importer = Importer {
        report: ImportReport {
            format: Format::Bruno,
            entries: Vec::new(),
        },
    };
    let config: Value = tree
        .files
        .iter()
        .find(|(n, _)| *n == "bruno.json")
        .and_then(|(_, c)| serde_json::from_str(c).ok())
        .unwrap_or(Value::Null);
    let name = config
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            root.trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|n| !n.is_empty())
                .unwrap_or("Bruno collection")
                .to_string()
        });

    let mut items = importer.children("", &tree);
    let collection_bru = tree
        .files
        .iter()
        .find(|(n, _)| *n == "collection.bru")
        .map(|(_, c)| parse(c));
    let mut variables = Map::new();
    if let Some(blocks) = &collection_bru {
        // Collection headers / auth apply to top-level items (Reqly collections have neither).
        let headers = rows(dict(blocks, "headers"));
        let auth = importer.auth(&name, blocks, get(blocks, "auth", "mode"));
        if !headers.is_empty() || auth != Auth::Inherit {
            for item in &mut items {
                let (h, a) = match item {
                    ImportedItem::Folder { data, .. } => (&mut data.headers, &mut data.auth),
                    ImportedItem::Request { data, .. } => (&mut data.headers, &mut data.auth),
                };
                let mut merged = headers.clone();
                merged.append(h);
                *h = merged;
                if *a == Auth::Inherit {
                    *a = auth.clone();
                }
            }
            importer.report.push(
                ReportKind::Converted,
                "collection_settings",
                &name,
                "collection headers and auth were applied to the top-level folders and requests",
            );
        }
        variables = Importer::variables(blocks);
        let scripts = importer.scripts(&name, blocks);
        if !scripts.is_empty() {
            importer.report.push_raw(
                ReportKind::Unsupported,
                "collection_scripts",
                &name,
                "collection-level scripts are not supported; they are listed here",
                serde_json::json!({ "preRequest": scripts.pre_request, "afterResponse": scripts.after_response }),
            );
        }
    }
    let mut environments = Vec::new();
    if let Some(envs) = tree.dirs.get("environments") {
        for (file, content) in &envs.files {
            let Some(env_name) = file.strip_suffix(".bru") else {
                continue;
            };
            let blocks = parse(content);
            let mut data: Map<String, Value> = dict(&blocks, "vars")
                .iter()
                .filter(|p| p.enabled)
                .map(|p| (p.key.clone(), Value::String(vars_to_reqly(&p.value))))
                .collect();
            let secrets: Vec<String> = match blocks.get("vars:secret") {
                Some(Block::List(names)) => names
                    .iter()
                    .map(|n| n.trim_start_matches('~').to_string())
                    .collect(),
                _ => Vec::new(),
            };
            for s in &secrets {
                data.entry(s.clone())
                    .or_insert(Value::String(String::new()));
            }
            if !secrets.is_empty() {
                importer.report.push(
                    ReportKind::Info,
                    "secrets_empty",
                    &format!("Environment {env_name}"),
                    "Bruno keeps secret values outside the collection; fill them in after import",
                );
            }
            environments.push(ImportedEnvironment {
                name: env_name.to_string(),
                color: None,
                data,
                secrets,
            });
        }
        environments.sort_by(|a, b| a.name.cmp(&b.name));
    }
    if sources.iter().any(|s| s.content.contains("process.env.")) {
        importer.report.push(ReportKind::Unsupported, "process_env", &name, "`{{process.env.*}}` variables (from .env files) are not supported; define them in an environment");
    }
    Ok(ImportBundle {
        format: Format::Bruno,
        collections: vec![ImportedCollection {
            name,
            variables,
            environments,
            items,
            ..ImportedCollection::default()
        }],
        report: importer.report,
        ..ImportBundle::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_and_text_blocks() {
        let blocks = parse(
            "meta {\n  name: A\n  seq: 2\n}\n\nheaders {\n  \"x: y\": 1\n  ~b: 2\n  c:\n}\n\nbody:json {\n  {\n    \"a\": 1\n  }\n}\n\nvars:secret [\n  token,\n  other\n]\n",
        );
        assert_eq!(get(&blocks, "meta", "name"), "A");
        assert_eq!(
            dict(&blocks, "headers"),
            [
                Pair {
                    key: "x: y".into(),
                    value: "1".into(),
                    enabled: true
                },
                Pair {
                    key: "b".into(),
                    value: "2".into(),
                    enabled: false
                },
                Pair {
                    key: "c".into(),
                    value: "".into(),
                    enabled: true
                },
            ]
        );
        assert_eq!(text(&blocks, "body:json"), "{\n  \"a\": 1\n}");
        assert_eq!(
            blocks["vars:secret"],
            Block::List(vec!["token".into(), "other".into()])
        );
    }

    #[test]
    fn multiline_values() {
        let blocks = parse("vars {\n  long: '''\n    line 1\n    line 2\n  '''\n  short: x\n}\n");
        assert_eq!(get(&blocks, "vars", "long"), "line 1\nline 2");
        assert_eq!(get(&blocks, "vars", "short"), "x");
    }
}
