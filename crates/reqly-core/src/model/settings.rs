use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProxyMode {
    /// No proxy, regardless of the environment.
    #[default]
    None,
    /// `HTTP(S)_PROXY` / `NO_PROXY` from the environment.
    System,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    /// e.g. `http://host:3128`, `socks5://host:1080`.
    pub url: String,
    pub username: String,
    pub password: String,
    /// Comma separated hosts that bypass the proxy.
    pub no_proxy: String,
}

/// Network settings. Every field is optional: `None` inherits from the
/// folder / collection / global level (resolved by [`RequestSettings::merge`]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct RequestSettings {
    pub timeout_ms: Option<u64>,
    pub connect_timeout_ms: Option<u64>,
    pub follow_redirects: Option<bool>,
    pub max_redirects: Option<u32>,
    pub verify_tls: Option<bool>,
    pub encode_url: Option<bool>,
    pub send_cookies: Option<bool>,
    pub store_cookies: Option<bool>,
    pub proxy: Option<ProxySettings>,
}

impl RequestSettings {
    /// Built-in defaults for the top of the inheritance chain.
    pub fn defaults() -> Self {
        Self {
            timeout_ms: Some(0),
            connect_timeout_ms: Some(30_000),
            follow_redirects: Some(true),
            max_redirects: Some(10),
            verify_tls: Some(true),
            encode_url: Some(true),
            send_cookies: Some(true),
            store_cookies: Some(true),
            proxy: Some(ProxySettings::default()),
        }
    }

    /// `self` overrides `base` field by field.
    pub fn merge(&self, base: &RequestSettings) -> RequestSettings {
        RequestSettings {
            timeout_ms: self.timeout_ms.or(base.timeout_ms),
            connect_timeout_ms: self.connect_timeout_ms.or(base.connect_timeout_ms),
            follow_redirects: self.follow_redirects.or(base.follow_redirects),
            max_redirects: self.max_redirects.or(base.max_redirects),
            verify_tls: self.verify_tls.or(base.verify_tls),
            encode_url: self.encode_url.or(base.encode_url),
            send_cookies: self.send_cookies.or(base.send_cookies),
            store_cookies: self.store_cookies.or(base.store_cookies),
            proxy: self.proxy.clone().or_else(|| base.proxy.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_specific_level() {
        let request = RequestSettings {
            timeout_ms: Some(5),
            ..Default::default()
        };
        let folder = RequestSettings {
            timeout_ms: Some(10),
            verify_tls: Some(false),
            ..Default::default()
        };
        let merged = request.merge(&folder.merge(&RequestSettings::defaults()));
        assert_eq!(merged.timeout_ms, Some(5));
        assert_eq!(merged.verify_tls, Some(false));
        assert_eq!(merged.follow_redirects, Some(true));
    }
}
