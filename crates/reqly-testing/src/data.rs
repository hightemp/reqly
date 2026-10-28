//! Data files of a run (CSV or JSON): one row of variables per iteration.

use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DataFormat {
    /// A header row with the variable names, then one row per iteration.
    Csv,
    /// An array of objects.
    Json,
}

impl DataFormat {
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

fn invalid(detail: impl ToString) -> CoreError {
    let detail = detail.to_string();
    CoreError::new("runner.data", format!("invalid data file: {detail}"))
        .with_param("detail", detail)
}

pub fn parse_data(format: DataFormat, text: &str) -> CoreResult<Vec<Map<String, Value>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match format {
        DataFormat::Csv => csv_rows(text),
        DataFormat::Json => match serde_json::from_str::<Value>(text).map_err(invalid)? {
            Value::Array(items) => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| match item {
                    Value::Object(map) => Ok(map),
                    _ => Err(invalid(format!("item {} is not an object", i + 1))),
                })
                .collect(),
            _ => Err(invalid("expected an array of objects")),
        },
    }
}

fn csv_rows(text: &str) -> CoreResult<Vec<Map<String, Value>>> {
    let mut reader = csv::ReaderBuilder::new().from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(invalid)?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    reader
        .records()
        .map(|record| {
            let record = record.map_err(invalid)?;
            Ok(headers
                .iter()
                .zip(record.iter())
                .map(|(name, value)| (name.clone(), Value::String(value.to_string())))
                .collect())
        })
        .collect()
}
