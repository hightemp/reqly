//! Server configuration: defaults < `reqly.toml` < `REQLY_*` environment variables.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Socket address to listen on.
    pub bind: String,
    /// Directory for the database and blobs.
    pub data_dir: PathBuf,
    /// `tracing` filter, e.g. `info,reqly_server=debug`.
    pub log: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8484".into(),
            data_dir: PathBuf::from(".reqly-data"),
            log: "info".into(),
        }
    }
}

impl Config {
    pub fn load(file: Option<&Path>, env: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let mut cfg = match file {
            Some(path) => {
                let text = std::fs::read_to_string(path)?;
                toml::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?
            }
            None => Self::default(),
        };
        if let Some(v) = env("REQLY_BIND") {
            cfg.bind = v;
        }
        if let Some(v) = env("REQLY_DATA_DIR") {
            cfg.data_dir = v.into();
        }
        if let Some(v) = env("REQLY_LOG") {
            cfg.log = v;
        }
        Ok(cfg)
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("reqly.db")
    }
}
