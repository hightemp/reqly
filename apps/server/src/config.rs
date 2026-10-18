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
    pub network: NetworkConfig,
    pub auth: crate::auth::AuthConfig,
}

/// Outgoing request policy (SSRF protection). Metadata and link-local
/// ranges are always denied.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NetworkConfig {
    pub block_private: bool,
    pub block_loopback: bool,
    /// Extra denied ranges, e.g. `["203.0.113.0/24"]`.
    pub deny: Vec<String>,
    /// Hostnames that bypass every check.
    pub allow_hosts: Vec<String>,
}

impl NetworkConfig {
    pub fn policy(&self) -> anyhow::Result<reqly_http::policy::NetworkPolicy> {
        let mut policy = reqly_http::policy::NetworkPolicy::server_default();
        policy.block_private = self.block_private;
        policy.block_loopback = self.block_loopback;
        policy.allow_hosts = self.allow_hosts.clone();
        for cidr in &self.deny {
            policy
                .deny
                .push(reqly_http::policy::Cidr::parse(cidr).map_err(|e| anyhow::anyhow!("{e}"))?);
        }
        Ok(policy)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8484".into(),
            data_dir: PathBuf::from(".reqly-data"),
            log: "info".into(),
            network: NetworkConfig::default(),
            auth: crate::auth::AuthConfig::default(),
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
        let flag = |v: String| matches!(v.as_str(), "1" | "true" | "yes");
        if let Some(v) = env("REQLY_BLOCK_PRIVATE") {
            cfg.network.block_private = flag(v);
        }
        if let Some(v) = env("REQLY_BLOCK_LOOPBACK") {
            cfg.network.block_loopback = flag(v);
        }
        if let Some(v) = env("REQLY_AUTH_MODE") {
            cfg.auth.mode = match v.to_lowercase().as_str() {
                "token" => crate::auth::AuthMode::Token,
                "none" => crate::auth::AuthMode::None,
                "multi" => crate::auth::AuthMode::Multi,
                other => {
                    anyhow::bail!("REQLY_AUTH_MODE must be none, token or multi, got {other}")
                }
            };
        }
        if let Some(v) = env("REQLY_REGISTRATION") {
            cfg.auth.registration = match v.to_lowercase().as_str() {
                "open" => crate::auth::Registration::Open,
                "invite" => crate::auth::Registration::Invite,
                "admin" => crate::auth::Registration::Admin,
                other => {
                    anyhow::bail!("REQLY_REGISTRATION must be open, invite or admin, got {other}")
                }
            };
        }
        if let Some(v) = env("REQLY_ADMIN_USERNAME") {
            cfg.auth.admin_username = Some(v);
        }
        if let Some(v) = env("REQLY_ADMIN_PASSWORD") {
            cfg.auth.admin_password = Some(v);
        }
        if let Some(v) = env("REQLY_AUTH_TOKEN") {
            cfg.auth.token = Some(v);
        }
        if let Some(v) = env("REQLY_SECURE_COOKIE") {
            cfg.auth.secure_cookie = flag(v);
        }
        if cfg.auth.mode == crate::auth::AuthMode::Token
            && cfg.auth.token.as_deref().is_none_or(|t| t.len() < 12)
        {
            anyhow::bail!("token auth needs REQLY_AUTH_TOKEN (at least 12 characters)");
        }
        if let Some(v) = env("REQLY_ALLOW_HOSTS") {
            cfg.network.allow_hosts = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        Ok(cfg)
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("reqly.db")
    }
}
