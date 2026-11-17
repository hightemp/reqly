//! `.http` / `.rest` files (JetBrains HTTP Client, VS Code REST Client) and
//! raw HTTP request text (`GET /path HTTP/1.1` + `Host:`).
//!
//! Requests are separated by `###` lines; `@name = value` lines define file
//! variables (imported as collection variables). `{{var}}` templates are kept
//! as they are, Reqly uses the same syntax.

use reqly_core::CoreResult;
use reqly_core::model::{
    Body, BodyKind, FileRef, FolderData, HttpRequest, HttpVersion, KeyValue, KeyValueKind,
};
use serde_json::{Map, Value};

use crate::detect::Format;
use crate::model::{ImportBundle, ImportedCollection, ImportedItem};
use crate::report::{ImportReport, ReportKind};
use crate::{Source, commented_out, curl::request_name};

const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT", "QUERY",
];

/// Headers that describe the transfer and are set again when sending.
const TRANSFER_HEADERS: &[&str] = &["content-length", "transfer-encoding", "connection"];

/// True for `.http` / `.rest` files and text that starts with a request line.
pub fn looks_like_http(name: &str, text: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".http") || lower.ends_with(".rest") {
        return true;
    }
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !is_comment(l) && variable(l).is_none())
        .is_some_and(|l| l.starts_with("###") || request_line(l).is_some())
}

fn is_comment(line: &str) -> bool {
    (line.starts_with('#') && !line.starts_with("###")) || line.starts_with("//")
}

/// `@name = value` (a file variable).
fn variable(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('@')?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| (name, value.trim()))
}

/// `METHOD target [HTTP/x]` or a bare URL (GET).
fn request_line(line: &str) -> Option<(String, String, Option<String>)> {
    let mut parts = line.split_whitespace();
    let first = parts.next()?;
    let (method, target) = if METHODS.contains(&first) {
        (first.to_string(), parts.next()?.to_string())
    } else if first.starts_with("http://") || first.starts_with("https://") {
        ("GET".to_string(), first.to_string())
    } else {
        return None;
    };
    let version = parts.next().map(str::to_string);
    if version
        .as_deref()
        .is_some_and(|v| !v.starts_with("HTTP/") || parts.next().is_some())
    {
        return None;
    }
    Some((method, target, version))
}

/// `# @name x` / `// @name x` / `# @name=x`.
fn name_tag(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix('#')
        .or_else(|| line.strip_prefix("//"))?
        .trim_start();
    let rest = rest.strip_prefix("@name")?;
    let rest = rest.trim_start().strip_prefix('=').unwrap_or(rest).trim();
    (!rest.is_empty()).then_some(rest)
}

fn stem(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = file.to_ascii_lowercase();
    for ext in [".http", ".rest"] {
        if lower.ends_with(ext) {
            return file[..file.len() - ext.len()].to_string();
        }
    }
    file.to_string()
}

pub fn import(sources: &[Source]) -> CoreResult<ImportBundle> {
    let mut report = ImportReport {
        format: Format::Http,
        entries: Vec::new(),
    };
    let mut variables = Map::new();
    let mut files: Vec<(String, Vec<ImportedItem>)> = Vec::new();
    for source in sources {
        let items = parse_file(&source.content, &mut variables, &mut report);
        if !items.is_empty() {
            files.push((stem(&source.name), items));
        }
    }
    if files.is_empty() {
        return Err(crate::invalid(Format::Http, "no HTTP requests found"));
    }
    let pasted = sources.len() == 1 && crate::is_pasted(&sources[0].name);
    let first = sources.first().map(|s| stem(&s.name)).unwrap_or_default();
    let collection_name = if sources.len() > 1 || pasted {
        "HTTP import".to_string()
    } else {
        first
    };
    // Pasted text or one request: the requests; one file: a folder named
    // after it; several files: a folder per file.
    let items = if files.len() == 1 && (pasted || files[0].1.len() == 1) {
        files.remove(0).1
    } else {
        files
            .into_iter()
            .map(|(name, items)| ImportedItem::folder(name, FolderData::default(), items))
            .collect()
    };
    Ok(ImportBundle {
        format: Format::Http,
        collections: vec![ImportedCollection {
            name: collection_name,
            variables,
            items,
            ..ImportedCollection::default()
        }],
        report,
        ..ImportBundle::default()
    })
}

/// The requests in `text` and what could not be converted exactly.
pub fn parse(text: &str) -> (Vec<HttpRequest>, Vec<String>) {
    let mut report = ImportReport::default();
    let requests = parse_file(text, &mut Map::new(), &mut report)
        .into_iter()
        .filter_map(|item| match item {
            ImportedItem::Request { data, .. } => Some(data),
            _ => None,
        })
        .collect();
    (
        requests,
        report.entries.into_iter().map(|e| e.message).collect(),
    )
}

/// Requests of one file; its variables are added to `variables`.
fn parse_file(
    text: &str,
    variables: &mut Map<String, Value>,
    report: &mut ImportReport,
) -> Vec<ImportedItem> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut blocks: Vec<(String, Vec<&str>)> = vec![(String::new(), Vec::new())];
    for line in text.lines() {
        if let Some(title) = line.trim_start().strip_prefix("###") {
            blocks.push((title.trim().to_string(), Vec::new()));
        } else {
            blocks.last_mut().expect("non-empty").1.push(line);
        }
    }
    blocks
        .into_iter()
        .filter_map(|(title, lines)| parse_block(&title, &lines, variables, report))
        .collect()
}

#[derive(PartialEq)]
enum State {
    Preamble,
    Headers,
    Body,
}

fn parse_block(
    title: &str,
    lines: &[&str],
    variables: &mut Map<String, Value>,
    report: &mut ImportReport,
) -> Option<ImportedItem> {
    let mut state = State::Preamble;
    let mut name: Option<String> = None;
    let mut method = String::new();
    let mut target = String::new();
    let mut version: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    let mut pre_script = String::new();
    let mut lines = lines.iter().copied().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        match state {
            State::Preamble => {
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(tag) = name_tag(trimmed) {
                    name = Some(tag.to_string());
                } else if is_comment(trimmed) {
                } else if let Some((k, v)) = variable(trimmed) {
                    variables.insert(k.to_string(), Value::String(convert_templates(v)));
                } else if let Some(script) = trimmed.strip_prefix('<').map(str::trim) {
                    // `< {% … %}`: a pre-request script.
                    pre_script = handler(script, &mut lines);
                } else if let Some((m, t, v)) = request_line(trimmed) {
                    (method, target, version) = (m, t, v);
                    // Query continuation lines: `  ?a=1` / `  &b=2`.
                    while let Some(next) = lines.peek() {
                        let n = next.trim();
                        if next.starts_with(char::is_whitespace)
                            && (n.starts_with('?') || n.starts_with('&'))
                        {
                            target.push_str(n);
                            lines.next();
                        } else {
                            break;
                        }
                    }
                    state = State::Headers;
                } else {
                    report.push(
                        ReportKind::Info,
                        "http_skipped",
                        title,
                        format!("skipped a line that is not a request: {trimmed}"),
                    );
                }
            }
            State::Headers => {
                if trimmed.is_empty() {
                    state = State::Body;
                } else if is_comment(trimmed) {
                } else if let Some((k, v)) = trimmed.split_once(':') {
                    headers.push(KeyValue::new(k.trim(), convert_templates(v.trim())));
                } else {
                    // No blank line before the body.
                    state = State::Body;
                    body.push(line);
                }
            }
            State::Body => body.push(line),
        }
    }
    if method.is_empty() {
        return None;
    }

    let mut request = HttpRequest {
        method,
        ..HttpRequest::default()
    };
    if version.as_deref() == Some("HTTP/2") || version.as_deref() == Some("HTTP/2.0") {
        request.settings.http_version = Some(HttpVersion::Http2);
    }

    // Raw HTTP: an origin-form target and a Host header.
    let host = headers
        .iter()
        .position(|h| h.name.eq_ignore_ascii_case("host"));
    let target = convert_templates(&target);
    let target = match host {
        Some(i) if target.starts_with('/') => {
            let host = headers.remove(i).value;
            let scheme = if host.ends_with(":443") {
                "https"
            } else {
                "http"
            };
            format!("{scheme}://{host}{target}")
        }
        _ => target,
    };
    headers.retain(|h| !TRANSFER_HEADERS.contains(&h.name.to_ascii_lowercase().as_str()));
    let (url, query) = match target.split_once('?') {
        Some((u, q)) => (u.to_string(), Some(q)),
        None => (target.clone(), None),
    };
    request.url = url;
    if let Some(q) = query {
        let q = q.split('#').next().unwrap_or_default();
        request.params = url::form_urlencoded::parse(q.as_bytes())
            .map(|(k, v)| KeyValue::new(k, v))
            .collect();
    }

    let item_name = name
        .or_else(|| (!title.is_empty()).then(|| title.to_string()))
        .unwrap_or_else(|| request_name(&request));

    let (body, after_script) = split_handlers(&body);
    request.body = build_body(&body, &mut headers, &item_name, report);
    request.headers = headers;
    if !pre_script.is_empty() || !after_script.is_empty() {
        for (script, code) in [
            (&pre_script, "http_pre_request_handler"),
            (&after_script, "http_response_handler"),
        ] {
            if !script.is_empty() {
                report.push(
                    ReportKind::Converted,
                    code,
                    &item_name,
                    "the script uses the HTTP Client API; it is imported commented out",
                );
            }
        }
        request.scripts.pre_request = commented_out(&pre_script, "HTTP Client");
        request.scripts.after_response = commented_out(&after_script, "HTTP Client");
    }
    Some(ImportedItem::request(item_name, request))
}

/// A handler after `<` / `>`: an inline `{% … %}` script (possibly over
/// several lines, read from `lines`) or a script file path.
fn handler<'a>(first: &str, lines: &mut impl Iterator<Item = &'a str>) -> String {
    let Some(rest) = first.strip_prefix("{%") else {
        return format!("// script file: {first}");
    };
    if let Some(script) = rest.strip_suffix("%}") {
        return script.trim().to_string();
    }
    let mut script = vec![rest];
    for line in lines {
        if let Some(last) = line.trim_end().strip_suffix("%}") {
            script.push(last);
            break;
        }
        script.push(line);
    }
    dedent(&script)
}

fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or(l.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// The body without response handlers (`> {% … %}`, `> file.js`) and
/// response references (`<> file`), and the handler script.
fn split_handlers<'a>(body: &[&'a str]) -> (Vec<&'a str>, String) {
    let mut out = Vec::new();
    let mut script = String::new();
    let mut lines = body.iter().copied();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.starts_with("<>") {
            continue;
        }
        if let Some(rest) = t.strip_prefix('>')
            && !t.starts_with(">>")
        {
            script = handler(rest.trim(), &mut lines);
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    (out, script)
}

fn header_value<'a>(headers: &'a [KeyValue], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

fn remove_content_type(headers: &mut Vec<KeyValue>) {
    headers.retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
}

fn file_ref(line: &str) -> Option<&str> {
    let path = line.trim().strip_prefix('<')?.trim();
    (!path.is_empty() && !path.starts_with('{')).then_some(path)
}

fn build_body(
    lines: &[&str],
    headers: &mut Vec<KeyValue>,
    path: &str,
    report: &mut ImportReport,
) -> Body {
    let text = convert_templates(&lines.join("\n"));
    if text.trim().is_empty() {
        return Body::default();
    }
    let original_type = header_value(headers, "content-type")
        .unwrap_or_default()
        .to_string();
    let content_type = original_type.to_ascii_lowercase();
    let report_file = |report: &mut ImportReport, file: &str| {
        report.push(
            ReportKind::Converted,
            "http_file_ref",
            path,
            format!("{file} is relative to the .http file; check the path"),
        );
    };

    if content_type.starts_with("multipart/form-data")
        && let Some(boundary) = boundary(&original_type)
    {
        let fields = multipart(&text, &boundary);
        for f in fields.iter().filter(|f| f.kind == KeyValueKind::File) {
            report_file(report, &f.value);
        }
        remove_content_type(headers);
        return Body {
            kind: BodyKind::Multipart,
            fields,
            ..Body::default()
        };
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if let [line] = lines.as_slice()
        && let Some(file) = file_ref(line)
    {
        report_file(report, file);
        return Body {
            kind: BodyKind::Binary,
            file: Some(FileRef::Path { path: file.into() }),
            ..Body::default()
        };
    }
    let kind = if content_type.contains("json") {
        BodyKind::Json
    } else if content_type.contains("xml") {
        BodyKind::Xml
    } else if content_type.contains("yaml") {
        BodyKind::Yaml
    } else if content_type.starts_with("text/html") {
        BodyKind::Html
    } else if content_type.starts_with("application/x-www-form-urlencoded") {
        let fields = url::form_urlencoded::parse(text.trim().replace('\n', "").as_bytes())
            .map(|(k, v)| KeyValue::new(k, v))
            .collect();
        // The body kind sets this header again when sending.
        remove_content_type(headers);
        return Body {
            kind: BodyKind::FormUrlencoded,
            fields,
            ..Body::default()
        };
    } else {
        BodyKind::Text
    };
    Body {
        kind,
        text,
        ..Body::default()
    }
}

fn boundary(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|p| {
        p.trim()
            .split_once('=')
            .filter(|(k, _)| k.eq_ignore_ascii_case("boundary"))
            .map(|(_, b)| b.trim_matches('"').to_string())
    })
}

/// Parts of a multipart body: `name` → value, or a file for `< path` values.
fn multipart(text: &str, boundary: &str) -> Vec<KeyValue> {
    let delimiter = format!("--{boundary}");
    let mut fields = Vec::new();
    for part in text.split(&delimiter).skip(1) {
        if part.starts_with("--") {
            break;
        }
        let part = part.trim_start_matches(['\r', '\n']);
        let (head, value) = part.split_once("\n\n").unwrap_or((part, ""));
        let Some(disposition) = head
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-disposition:"))
        else {
            continue;
        };
        let param = |key: &str| {
            disposition.split(';').find_map(|p| {
                p.trim()
                    .strip_prefix(key)
                    .and_then(|v| v.strip_prefix('='))
                    .map(|v| v.trim_matches('"').to_string())
            })
        };
        let Some(name) = param("name") else { continue };
        let value = value.trim_end_matches(['\r', '\n']);
        let mut field = KeyValue::new(name, value);
        if let Some(file) = file_ref(value) {
            field.value = file.to_string();
            field.kind = KeyValueKind::File;
        }
        fields.push(field);
    }
    fields
}

/// HTTP Client dynamic variables that have another name in Reqly.
fn convert_templates(text: &str) -> String {
    text.replace("{{$uuid}}", "{{$guid}}")
        .replace("{{$random.uuid}}", "{{$guid}}")
}
