//! HAR 1.2: import requests from a HAR log, export requests as a HAR log.

use reqly_core::CoreResult;
use reqly_core::model::{Body, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind};
use serde_json::{Value, json};

use crate::detect::{Format, parse_structured};
use crate::model::{ExportData, ImportBundle, ImportedCollection, ImportedItem};
use crate::report::{ImportReport, ReportKind};

pub fn is_har(value: &Value) -> bool {
    value
        .get("log")
        .is_some_and(|l| l.get("entries").is_some_and(Value::is_array))
        || is_entry(value)
        || is_request(value)
}

/// A single HAR request object (`{ method, url, httpVersion, … }`).
fn is_request(value: &Value) -> bool {
    value.get("method").is_some_and(Value::is_string)
        && value.get("url").is_some_and(Value::is_string)
        && (value.get("httpVersion").is_some()
            || value.get("postData").is_some()
            || value.get("queryString").is_some())
}

/// A single HAR entry (`{ request, response, … }`).
fn is_entry(value: &Value) -> bool {
    value.get("request").is_some_and(is_request)
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

/// Headers that browsers / HTTP stacks add and must not be replayed.
fn skip_header(name: &str) -> bool {
    name.starts_with(':')
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "content-length" | "host" | "connection" | "accept-encoding"
        )
}

fn body(post: Option<&Value>) -> Body {
    let Some(post) = post.filter(|p| p.as_object().is_some_and(|o| !o.is_empty())) else {
        return Body::default();
    };
    let mime = s(post, "mimeType")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let params = arr(post, "params");
    if mime == "application/x-www-form-urlencoded"
        || (!params.is_empty() && !mime.starts_with("multipart"))
    {
        return Body {
            kind: BodyKind::FormUrlencoded,
            fields: params
                .iter()
                .map(|p| KeyValue::new(s(p, "name"), s(p, "value")))
                .collect(),
            ..Body::default()
        };
    }
    if mime.starts_with("multipart/form-data") && !params.is_empty() {
        return Body {
            kind: BodyKind::Multipart,
            fields: params
                .iter()
                .map(|p| {
                    if s(p, "fileName").is_empty() {
                        KeyValue::new(s(p, "name"), s(p, "value"))
                    } else {
                        let mut row = KeyValue::new(s(p, "name"), s(p, "fileName"));
                        row.kind = KeyValueKind::File;
                        row
                    }
                })
                .collect(),
            ..Body::default()
        };
    }
    let text = s(post, "text");
    if text.is_empty() {
        return Body::default();
    }
    let kind = if mime.contains("json") {
        BodyKind::Json
    } else if mime.contains("xml") {
        BodyKind::Xml
    } else if mime.contains("yaml") {
        BodyKind::Yaml
    } else if mime.contains("html") {
        BodyKind::Html
    } else {
        BodyKind::Text
    };
    Body {
        kind,
        text: text.into(),
        ..Body::default()
    }
}

pub fn import(text: &str) -> CoreResult<ImportBundle> {
    let value = parse_structured(text).ok_or_else(|| crate::invalid(Format::Har, "not JSON"))?;
    if !is_har(&value) {
        return Err(crate::invalid(Format::Har, "not a HAR file"));
    }
    // A single entry or request becomes a one-entry log.
    let value = if is_request(&value) {
        json!({ "log": { "entries": [{ "request": value }] } })
    } else if is_entry(&value) {
        json!({ "log": { "entries": [value] } })
    } else {
        value
    };
    let log = &value["log"];
    let mut report = ImportReport {
        format: Format::Har,
        entries: Vec::new(),
    };
    let mut items = Vec::new();
    for (i, entry) in arr(log, "entries").iter().enumerate() {
        let req = entry.get("request").cloned().unwrap_or(Value::Null);
        let url = s(&req, "url");
        if url.is_empty() {
            report.push(
                ReportKind::Info,
                "har_no_url",
                &format!("#{}", i + 1),
                "an entry without a URL was skipped",
            );
            continue;
        }
        let base = url.split('?').next().unwrap_or(url).to_string();
        let query = arr(&req, "queryString");
        let params = if query.is_empty() {
            url.split_once('?')
                .map(|(_, q)| {
                    q.split('&')
                        .filter(|p| !p.is_empty())
                        .map(|p| {
                            let (k, v) = p.split_once('=').unwrap_or((p, ""));
                            KeyValue::new(k, v)
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            query
                .iter()
                .map(|q| {
                    let mut row = KeyValue::new(s(q, "name"), s(q, "value"));
                    row.description = s(q, "comment").into();
                    row
                })
                .collect()
        };
        let headers = arr(&req, "headers")
            .iter()
            .filter(|h| !skip_header(s(h, "name")))
            .map(|h| KeyValue::new(s(h, "name"), s(h, "value")))
            .collect();
        let request = HttpRequest {
            method: s(&req, "method").to_uppercase(),
            url: base,
            params,
            headers,
            body: body(req.get("postData")),
            description: s(&req, "comment").into(),
            ..HttpRequest::default()
        };
        let name = crate::curl::request_name(&request);
        items.push(ImportedItem::request(name, request));
    }
    if !items.is_empty()
        && arr(log, "entries")
            .iter()
            .any(|e| e.get("response").is_some())
    {
        report.push(
            ReportKind::Info,
            "har_responses",
            "",
            "recorded responses were not imported",
        );
    }
    let creator = log.get("creator").map(|c| s(c, "name")).unwrap_or("");
    let name = if creator.is_empty() {
        "HAR import".to_string()
    } else {
        format!("HAR import ({creator})")
    };
    Ok(ImportBundle {
        format: Format::Har,
        collections: vec![ImportedCollection {
            name,
            items,
            ..ImportedCollection::default()
        }],
        report,
        ..ImportBundle::default()
    })
}

fn entry(r: &HttpRequest, folders: &[&FolderData], started: &str) -> Value {
    let prepared = reqly_http::prepare::prepare(r, folders).ok();
    // The unrendered URL with enabled query parameters (templates stay as they are).
    let query_text: Vec<String> = r
        .params
        .iter()
        .filter(|p| p.enabled && !p.name.is_empty())
        .map(|p| {
            if p.value.is_empty() {
                p.name.clone()
            } else {
                format!("{}={}", p.name, p.value)
            }
        })
        .collect();
    let url = if query_text.is_empty() {
        r.url.clone()
    } else {
        format!("{}?{}", r.url, query_text.join("&"))
    };
    let headers: Vec<Value> = prepared
        .as_ref()
        .map(|p| {
            p.headers
                .iter()
                .map(|(k, v)| json!({ "name": k, "value": v }))
                .collect()
        })
        .unwrap_or_default();
    let query: Vec<Value> = r
        .params
        .iter()
        .filter(|p| p.enabled)
        .map(|p| json!({ "name": p.name, "value": p.value }))
        .collect();
    let mut request = json!({
        "method": if r.method.is_empty() { "GET" } else { &r.method },
        "url": url,
        "httpVersion": "HTTP/1.1",
        "cookies": [],
        "headers": headers,
        "queryString": query,
        "headersSize": -1,
        "bodySize": -1,
    });
    let mime = crate::text::content_type(&r.headers)
        .map(str::to_string)
        .or_else(|| r.body.kind.default_content_type().map(str::to_string))
        .unwrap_or_default();
    match r.body.kind {
        BodyKind::None => {}
        BodyKind::FormUrlencoded | BodyKind::Multipart => {
            request["postData"] = json!({
                "mimeType": mime,
                "params": r.body.fields.iter().filter(|f| f.enabled).map(|f| {
                    if f.kind == KeyValueKind::File {
                        json!({ "name": f.name, "fileName": f.value })
                    } else {
                        json!({ "name": f.name, "value": f.value })
                    }
                }).collect::<Vec<_>>(),
            });
        }
        BodyKind::Binary => {
            let file = match &r.body.file {
                Some(FileRef::Path { path }) => path.clone(),
                Some(FileRef::Blob { name, .. }) => name.clone(),
                None => String::new(),
            };
            request["postData"] =
                json!({ "mimeType": "application/octet-stream", "text": "", "comment": file });
        }
        _ => request["postData"] = json!({ "mimeType": mime, "text": r.body.text }),
    }
    json!({
        "startedDateTime": started,
        "time": 0,
        "request": request,
        "response": {
            "status": 0, "statusText": "", "httpVersion": "HTTP/1.1", "cookies": [], "headers": [],
            "content": { "size": 0, "mimeType": "" }, "redirectURL": "", "headersSize": -1, "bodySize": -1
        },
        "cache": {},
        "timings": { "send": 0, "wait": 0, "receive": 0 },
    })
}

fn walk(
    items: &[ImportedItem],
    folders: &mut Vec<FolderData>,
    started: &str,
    out: &mut Vec<Value>,
) {
    for item in items {
        match item {
            ImportedItem::Folder { data, children, .. } => {
                folders.push(data.clone());
                walk(children, folders, started, out);
                folders.pop();
            }
            ImportedItem::Request { data, .. } => {
                let chain: Vec<&FolderData> = folders.iter().collect();
                out.push(entry(data, &chain, started));
            }
            // HAR has no WebSocket or gRPC entries.
            _ => {}
        }
    }
}

/// A HAR log with one entry per request (no responses).
pub fn export(data: &ExportData, started: &str) -> CoreResult<String> {
    let mut entries = Vec::new();
    for c in &data.collections {
        walk(&c.items, &mut Vec::new(), started, &mut entries);
    }
    let doc = json!({
        "log": {
            "version": "1.2",
            "creator": { "name": "Reqly", "version": env!("CARGO_PKG_VERSION") },
            "pages": [],
            "entries": entries,
        }
    });
    serde_json::to_string_pretty(&doc)
        .map_err(|e| reqly_core::CoreError::new("export.failed", e.to_string()))
}
