//! Import and export of foreign formats (stage 3).
//!
//! Every importer produces an [`ImportBundle`]: collections with folders and
//! requests in Reqly's own model, environments, cookies and an
//! [`ImportReport`] listing everything that was converted approximately or
//! could not be imported. Nothing is dropped silently.

mod auth_map;
pub mod bruno;
pub mod codegen;
pub mod curl;
pub mod detect;
pub mod har;
pub mod insomnia;
pub mod insomnia_export;
pub mod model;
pub mod openapi;
pub mod postman;
pub mod report;
pub mod reqly;
pub mod text;

pub use detect::{Format, detect};
pub use model::*;
pub use report::{ImportReport, ReportEntry, ReportKind};

use reqly_core::{CoreError, CoreResult};

/// One input file (a path is only used for directory formats and messages).
#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub content: String,
}

impl Source {
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
        }
    }
}

/// Text files of a zip archive (e.g. a zipped Bruno collection), as sources.
pub fn unzip(bytes: &[u8]) -> CoreResult<Vec<Source>> {
    use std::io::Read;
    let err = |e: String| CoreError::new("import.zip", e);
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| err(e.to_string()))?;
        if file.is_dir() || file.size() > 16 * 1024 * 1024 {
            continue;
        }
        let name = file.name().map_err(|e| err(e.to_string()))?.into_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| err(e.to_string()))?;
        if let Ok(text) = String::from_utf8(bytes) {
            out.push(Source::new(name, text));
        }
    }
    Ok(out)
}

/// Formats Reqly can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExportFormat {
    ReqlyJson,
    ReqlyYaml,
    Insomnia4,
    Insomnia5,
    Postman,
    Har,
    Curl,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::ReqlyJson => "reqly.json",
            Self::ReqlyYaml => "reqly.yaml",
            Self::Insomnia4 => "insomnia.json",
            Self::Insomnia5 => "insomnia.yaml",
            Self::Postman => "postman_collection.json",
            Self::Har => "har",
            Self::Curl => "sh",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::ReqlyJson | Self::Insomnia4 | Self::Postman | Self::Har => "application/json",
            Self::Curl => "text/x-shellscript",
            Self::ReqlyYaml | Self::Insomnia5 => "application/yaml",
        }
    }
}

/// One file of an export.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportFile {
    pub name: String,
    pub mime: String,
    pub content: String,
}

/// A file-name-safe version of `name` (lowercase, dashes).
pub fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut out = String::new();
    for c in cleaned.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "export".into()
    } else {
        trimmed.to_lowercase()
    }
}

/// Writes `data` in `format`; `stem` names the files. `exported_at` is Unix milliseconds.
pub fn export(
    data: &ExportData,
    format: ExportFormat,
    exported_at: i64,
    source: &str,
    stem: &str,
) -> CoreResult<Vec<ExportFile>> {
    let options = |syntax| reqly::ExportOptions {
        syntax,
        exported_at: Some(exported_at),
        source: source.to_string(),
    };
    let one = |content: String| {
        vec![ExportFile {
            name: format!("{}.{}", file_stem(stem), format.extension()),
            mime: format.mime().into(),
            content,
        }]
    };
    Ok(match format {
        ExportFormat::ReqlyJson => one(reqly::export(data, &options(reqly::Syntax::Json))?),
        ExportFormat::ReqlyYaml => one(reqly::export(data, &options(reqly::Syntax::Yaml))?),
        ExportFormat::Insomnia4 => one(insomnia_export::export_v4(data, exported_at)?),
        ExportFormat::Insomnia5 => one(insomnia_export::export_v5(data)?),
        ExportFormat::Har => one(har::export(
            data,
            &insomnia_export::rfc3339(exported_at.div_euclid(1000)),
        )?),
        ExportFormat::Curl => one(curl::export(data)),
        ExportFormat::Postman => postman::export(data)?
            .into_iter()
            .map(|(suffix, content)| ExportFile {
                name: format!("{}.{suffix}", file_stem(stem)),
                mime: "application/json".into(),
                content,
            })
            .collect(),
    })
}

pub(crate) fn invalid(format: Format, message: impl Into<String>) -> CoreError {
    CoreError::new("import.invalid", message.into()).with_param("format", format.id())
}

/// Detects the format of `sources` (unless given) and imports them.
pub fn import(sources: &[Source], format: Option<Format>) -> CoreResult<ImportBundle> {
    let first = sources
        .first()
        .ok_or_else(|| CoreError::new("import.empty", "nothing to import"))?;
    let format = match format {
        Some(f) => f,
        None => detect(sources).ok_or_else(|| {
            CoreError::new("import.unknown_format", "the format was not recognized")
                .with_param("name", first.name.clone())
        })?,
    };
    let mut bundle = match format {
        Format::Reqly => reqly::import(&first.content)?,
        Format::Insomnia => insomnia::import(&first.content)?,
        Format::Curl => curl::import(&first.name, &first.content)?,
        Format::Har => har::import(&first.content)?,
        Format::OpenApi => openapi::import(&first.content)?,
        Format::Bruno => bruno::import(sources)?,
        Format::Postman => {
            // Several Postman files (collection + environments) can be imported together.
            let mut bundle = ImportBundle {
                format,
                ..ImportBundle::default()
            };
            for source in sources {
                let part = postman::import(&source.content)?;
                bundle.collections.extend(part.collections);
                bundle.environments.extend(part.environments);
                bundle.report.entries.extend(part.report.entries);
            }
            bundle
        }
    };
    bundle.format = format;
    bundle.report.format = format;
    Ok(bundle)
}

/// A GraphQL body; `variables` are pretty-printed JSON (empty when none).
pub(crate) fn graphql_body(
    query: &str,
    variables: &serde_json::Value,
    operation_name: &str,
) -> reqly_core::model::Body {
    let variables = match variables {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Object(o) if o.is_empty() => String::new(),
        serde_json::Value::String(s) => s.clone(),
        v => serde_json::to_string_pretty(v).unwrap_or_default(),
    };
    reqly_core::model::Body {
        kind: reqly_core::model::BodyKind::Graphql,
        graphql: reqly_core::model::GraphqlBody {
            query: query.to_string(),
            variables,
            operation_name: operation_name.to_string(),
        },
        ..Default::default()
    }
}

/// `{"query", "variables", "operationName"}` of a GraphQL body.
pub(crate) fn graphql_json(body: &reqly_core::model::Body) -> serde_json::Value {
    let g = &body.graphql;
    let variables: serde_json::Value = if g.variables.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&g.variables)
            .unwrap_or_else(|_| serde_json::Value::String(g.variables.clone()))
    };
    let mut v = serde_json::json!({ "query": g.query, "variables": variables });
    if !g.operation_name.is_empty() {
        v["operationName"] = serde_json::json!(g.operation_name);
    }
    v
}

/// Script API calls Reqly does not implement, found by a plain text scan.
pub fn unsupported_script_calls(script: &str) -> Vec<&'static str> {
    const UNSUPPORTED: &[(&str, &str)] = &[
        ("pm.vault", "pm.vault"),
        ("pm.visualizer", "pm.visualizer"),
        ("pm.cookies.jar", "pm.cookies.jar"),
        ("pm.require(", "pm.require"),
        ("xml2Json(", "xml2Json"),
        ("insomnia.parentFolders", "insomnia.parentFolders"),
        ("'cheerio'", "require('cheerio')"),
        ("\"cheerio\"", "require('cheerio')"),
        ("'xml2js'", "require('xml2js')"),
        ("\"xml2js\"", "require('xml2js')"),
        ("'tv4'", "require('tv4')"),
        ("\"tv4\"", "require('tv4')"),
        ("'ajv'", "require('ajv')"),
        ("\"ajv\"", "require('ajv')"),
        ("'csv-parse", "require('csv-parse')"),
        ("\"csv-parse", "require('csv-parse')"),
    ];
    let mut out: Vec<&'static str> = UNSUPPORTED
        .iter()
        .filter(|(needle, _)| script.contains(needle))
        .map(|(_, name)| *name)
        .collect();
    out.dedup();
    out
}

/// Report entries for imported scripts that run in Reqly.
pub(crate) fn report_scripts(
    report: &mut report::ImportReport,
    path: &str,
    scripts: &reqly_core::model::Scripts,
) {
    let mut calls = unsupported_script_calls(&scripts.pre_request);
    calls.extend(unsupported_script_calls(&scripts.after_response));
    calls.sort_unstable();
    calls.dedup();
    if !calls.is_empty() {
        report.push(
            report::ReportKind::Unsupported,
            "script_unsupported_calls",
            path,
            format!(
                "scripts use APIs Reqly does not provide: {}",
                calls.join(", ")
            ),
        );
    }
}

/// Scripts written for another tool's API, kept commented out so they do
/// not fail every send.
pub(crate) fn commented_out(script: &str, tool: &str) -> String {
    if script.trim().is_empty() {
        return String::new();
    }
    let mut out =
        format!("// {tool} script (uses its own API); adapt it to rq / pm and uncomment.\n");
    for line in script.lines() {
        out.push_str("// ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod script_tests {
    use super::{commented_out, unsupported_script_calls};

    #[test]
    fn finds_unsupported_calls() {
        let found = unsupported_script_calls(
            "pm.visualizer.set(t); const $ = require('cheerio'); pm.environment.set('a', 1)",
        );
        assert_eq!(found, ["pm.visualizer", "require('cheerio')"]);
        assert!(unsupported_script_calls("pm.test('x', () => pm.expect(1).to.eql(1))").is_empty());
    }

    #[test]
    fn foreign_scripts_are_commented_out() {
        assert_eq!(commented_out("", "Bruno"), "");
        let out = commented_out("bru.setVar('a', 1);\nres.getBody();", "Bruno");
        assert!(out.lines().all(|l| l.starts_with("//")), "{out}");
        assert!(out.contains("// bru.setVar('a', 1);"));
    }
}
