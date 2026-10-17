//! cURL commands: import several commands, export a set of commands.

use reqly_core::CoreResult;
use reqly_core::model::{FolderData, HttpRequest};

use crate::detect::Format;
use crate::model::{ExportData, ImportBundle, ImportedCollection, ImportedItem};
use crate::report::{ImportReport, ReportKind};

/// True when the text is made of curl commands (comments and blank lines allowed).
pub fn looks_like_curl(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .is_some_and(|l| {
            let l = l.trim_start_matches('$').trim_start();
            l.starts_with("curl ") || l.starts_with("curl.exe ") || l == "curl"
        })
}

/// A readable name: `METHOD /path` or the URL.
pub fn request_name(r: &HttpRequest) -> String {
    let url = r.url.as_str();
    let path = url
        .split_once("://")
        .map(|(_, rest)| rest.find('/').map_or("/", |i| &rest[i..]))
        .unwrap_or(url);
    let path = path.split('?').next().unwrap_or(path);
    let path = if path.is_empty() { "/" } else { path };
    format!(
        "{} {path}",
        if r.method.is_empty() {
            "GET"
        } else {
            &r.method
        }
    )
}

pub fn import(name: &str, text: &str) -> CoreResult<ImportBundle> {
    let mut report = ImportReport {
        format: Format::Curl,
        entries: Vec::new(),
    };
    let mut items = Vec::new();
    for (i, parsed) in reqly_http::curl::parse_curl_each(text)?
        .into_iter()
        .enumerate()
    {
        match parsed {
            Ok(p) => {
                let item_name = request_name(&p.request);
                for w in &p.warnings {
                    report.push(
                        ReportKind::Converted,
                        "curl_option",
                        &item_name,
                        format!("ignored option: {w}"),
                    );
                }
                if p.request.url.is_empty() {
                    report.push(
                        ReportKind::Info,
                        "curl_no_url",
                        &item_name,
                        "the command has no URL",
                    );
                }
                items.push(ImportedItem::request(item_name, p.request));
            }
            Err(e) => report.push(
                ReportKind::Info,
                "curl_skipped",
                &format!("#{}", i + 1),
                format!("skipped a line that is not a curl command ({})", e.message),
            ),
        }
    }
    if items.is_empty() {
        return Err(crate::invalid(Format::Curl, "no curl commands found"));
    }
    // One command: a single request; several: a folder named after the source.
    let collection_name = if name.is_empty() {
        "cURL import".to_string()
    } else {
        name.to_string()
    };
    let items = if items.len() > 1 {
        vec![ImportedItem::folder(
            collection_name.clone(),
            FolderData::default(),
            items,
        )]
    } else {
        items
    };
    Ok(ImportBundle {
        format: Format::Curl,
        collections: vec![ImportedCollection {
            name: collection_name,
            items,
            ..ImportedCollection::default()
        }],
        report,
        ..ImportBundle::default()
    })
}

fn walk(
    items: &[ImportedItem],
    folders: &mut Vec<FolderData>,
    path: &mut Vec<String>,
    out: &mut Vec<String>,
) {
    for item in items {
        match item {
            ImportedItem::Folder {
                name,
                data,
                children,
                ..
            } => {
                folders.push(data.clone());
                path.push(name.clone());
                walk(children, folders, path, out);
                path.pop();
                folders.pop();
            }
            ImportedItem::Request { name, data, .. } => {
                let chain: Vec<&FolderData> = folders.iter().collect();
                let mut title = path.clone();
                title.push(name.clone());
                match reqly_http::prepare::prepare(data, &chain) {
                    Ok(prepared) => out.push(format!(
                        "# {}\n{}",
                        title.join(" / "),
                        reqly_http::curl::to_curl(&prepared)
                    )),
                    Err(e) => out.push(format!("# {}: {}", title.join(" / "), e.message)),
                }
            }
            _ => {}
        }
    }
}

/// All requests as curl commands (templates are left unrendered).
pub fn export(data: &ExportData) -> String {
    let mut out = Vec::new();
    for c in &data.collections {
        walk(&c.items, &mut Vec::new(), &mut Vec::new(), &mut out);
    }
    out.join("\n\n") + "\n"
}
