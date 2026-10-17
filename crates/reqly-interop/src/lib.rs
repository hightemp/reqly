//! Import and export of foreign formats (stage 3).
//!
//! Every importer produces an [`ImportBundle`]: collections with folders and
//! requests in Reqly's own model, environments, cookies and an
//! [`ImportReport`] listing everything that was converted approximately or
//! could not be imported. Nothing is dropped silently.

pub mod detect;
pub mod insomnia;
pub mod insomnia_export;
pub mod model;
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

/// Formats Reqly can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExportFormat {
    ReqlyJson,
    ReqlyYaml,
    Insomnia4,
    Insomnia5,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::ReqlyJson => "reqly.json",
            Self::ReqlyYaml => "reqly.yaml",
            Self::Insomnia4 => "insomnia.json",
            Self::Insomnia5 => "insomnia.yaml",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::ReqlyJson | Self::Insomnia4 => "application/json",
            Self::ReqlyYaml | Self::Insomnia5 => "application/yaml",
        }
    }
}

/// Writes `data` in `format`. `exported_at` is Unix milliseconds.
pub fn export(
    data: &ExportData,
    format: ExportFormat,
    exported_at: i64,
    source: &str,
) -> CoreResult<String> {
    let options = |syntax| reqly::ExportOptions {
        syntax,
        exported_at: Some(exported_at),
        source: source.to_string(),
    };
    match format {
        ExportFormat::ReqlyJson => reqly::export(data, &options(reqly::Syntax::Json)),
        ExportFormat::ReqlyYaml => reqly::export(data, &options(reqly::Syntax::Yaml)),
        ExportFormat::Insomnia4 => insomnia_export::export_v4(data, exported_at),
        ExportFormat::Insomnia5 => insomnia_export::export_v5(data),
    }
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
    };
    bundle.format = format;
    bundle.report.format = format;
    Ok(bundle)
}
