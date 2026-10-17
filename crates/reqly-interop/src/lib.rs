//! Import and export of foreign formats (stage 3).
//!
//! Every importer produces an [`ImportBundle`]: collections with folders and
//! requests in Reqly's own model, environments, cookies and an
//! [`ImportReport`] listing everything that was converted approximately or
//! could not be imported. Nothing is dropped silently.

pub mod bruno;
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
