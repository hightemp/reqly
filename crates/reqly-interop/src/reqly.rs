//! Reqly's own exchange format (`docs/format/reqly.schema.json`).
//!
//! ```json
//! { "format": "reqly", "formatVersion": "1.0.0", "collections": [...], ... }
//! ```
//! Collections, items and environments use the same JSON shapes as the
//! application model, so the format follows the model and old files are
//! upgraded by [`migrate`].

use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::detect::{Format, parse_structured};
use crate::model::{
    ExportData, ExportedResponse, ImportBundle, ImportedCollection, ImportedCookie,
    ImportedEnvironment,
};
use crate::report::ImportReport;

pub const FORMAT: &str = "reqly";
/// Current format version (semver). Bump the major for breaking changes and
/// add a step to [`migrate`].
pub const FORMAT_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReqlyDocument {
    pub format: String,
    pub format_version: String,
    /// Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<i64>,
    /// Application that wrote the file, e.g. `reqly 0.3.0`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Name of the exported workspace.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workspace: String,
    #[serde(default)]
    pub collections: Vec<ImportedCollection>,
    /// Global environments of the workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<ImportedEnvironment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cookies: Vec<ImportedCookie>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<ExportedResponse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Syntax {
    #[default]
    Json,
    Yaml,
}

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    pub syntax: Syntax,
    /// Unix milliseconds to record as the export time.
    pub exported_at: Option<i64>,
    pub source: String,
}

pub fn document(data: &ExportData, options: &ExportOptions) -> ReqlyDocument {
    ReqlyDocument {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION.into(),
        exported_at: options.exported_at,
        source: options.source.clone(),
        workspace: data.workspace.clone(),
        collections: data.collections.clone(),
        environments: data.environments.clone(),
        cookies: data.cookies.clone(),
        history: data.history.clone(),
    }
}

pub fn export(data: &ExportData, options: &ExportOptions) -> CoreResult<String> {
    let err = |e: String| CoreError::new("export.failed", e);
    let mut value =
        serde_json::to_value(document(data, options)).map_err(|e| err(e.to_string()))?;
    compact(&mut value);
    match options.syntax {
        Syntax::Json => serde_json::to_string_pretty(&value).map_err(|e| err(e.to_string())),
        Syntax::Yaml => serde_yaml_ng::to_string(&value).map_err(|e| err(e.to_string())),
    }
}

/// Drops values that import restores by default: nulls, empty arrays and
/// objects, and per-row defaults of key/value rows (`id`, `enabled: true`,
/// `kind: text`, empty description).
fn compact(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(compact),
        Value::Object(map) => {
            let is_row = map.contains_key("enabled")
                && map.contains_key("name")
                && map.contains_key("value");
            if is_row {
                map.shift_remove("id");
                if map.get("enabled") == Some(&Value::Bool(true)) {
                    map.shift_remove("enabled");
                }
                if map.get("kind").and_then(Value::as_str) == Some("text") {
                    map.shift_remove("kind");
                }
            }
            map.values_mut().for_each(compact);
            map.retain(|key, v| match v {
                Value::Null => false,
                Value::Array(a) => !a.is_empty(),
                Value::Object(o) => !o.is_empty(),
                Value::String(s) => {
                    !(s.is_empty()
                        && (is_row
                            || matches!(
                                key.as_str(),
                                "description"
                                    | "text"
                                    | "preRequest"
                                    | "afterResponse"
                                    | "query"
                                    | "variables"
                                    | "operationName"
                            )))
                }
                Value::Bool(false) => key != "eventStream",
                _ => true,
            });
        }
        _ => {}
    }
}

fn major(version: &str) -> Option<u64> {
    version.split('.').next()?.parse().ok()
}

/// Upgrades a document of any supported version to [`FORMAT_VERSION`].
pub fn migrate(mut value: Value) -> CoreResult<Value> {
    let version = value
        .get("formatVersion")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0")
        .to_string();
    let current = major(FORMAT_VERSION).unwrap_or(1);
    match major(&version) {
        Some(m) if m > current => {
            return Err(CoreError::new(
                "import.newer_format",
                format!("this file was written by a newer Reqly (format {version})"),
            )
            .with_param("version", version));
        }
        Some(_) => {}
        None => {
            return Err(crate::invalid(
                Format::Reqly,
                format!("invalid format version {version}"),
            ));
        }
    }
    // Future steps: `if major(&version) == Some(1) { value = v1_to_v2(value) }`.
    if let Some(obj) = value.as_object_mut() {
        obj.insert("formatVersion".into(), Value::String(FORMAT_VERSION.into()));
    }
    Ok(value)
}

pub fn parse(text: &str) -> CoreResult<ReqlyDocument> {
    let value =
        parse_structured(text).ok_or_else(|| crate::invalid(Format::Reqly, "not JSON or YAML"))?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(crate::invalid(Format::Reqly, "not a Reqly export"));
    }
    serde_json::from_value(migrate(value)?)
        .map_err(|e| crate::invalid(Format::Reqly, e.to_string()))
}

pub fn import(text: &str) -> CoreResult<ImportBundle> {
    let doc = parse(text)?;
    Ok(ImportBundle {
        format: Format::Reqly,
        collections: doc.collections,
        environments: doc.environments,
        cookies: doc.cookies,
        history: doc.history,
        report: ImportReport {
            format: Format::Reqly,
            entries: Vec::new(),
        },
    })
}
