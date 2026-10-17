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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HttpVersion {
    /// HTTP/2 over TLS when the server offers it, HTTP/1.1 otherwise.
    #[default]
    Auto,
    #[serde(rename = "http1.1")]
    Http1_1,
    /// HTTP/2, with an upgrade from HTTP/1.1 on plain connections.
    Http2,
    /// HTTP/2 without negotiation (h2c prior knowledge).
    Http2PriorKnowledge,
    Http3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TlsVersion {
    #[default]
    Default,
    #[serde(rename = "tls1.0")]
    Tls1_0,
    #[serde(rename = "tls1.1")]
    Tls1_1,
    #[serde(rename = "tls1.2")]
    Tls1_2,
    #[serde(rename = "tls1.3")]
    Tls1_3,
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
    pub http_version: Option<HttpVersion>,
    /// DNS overrides, curl `--resolve` syntax: `host:port:address[,address]`.
    pub resolve: Option<Vec<String>>,
    /// Connect elsewhere while keeping the URL's host for `Host` and SNI,
    /// curl `--connect-to` syntax: `host:port:connect-host:connect-port`.
    pub connect_to: Option<Vec<String>>,
    pub tls_min: Option<TlsVersion>,
    pub tls_max: Option<TlsVersion>,
    /// Advertise `Accept-Encoding` and decode compressed responses.
    pub compression: Option<bool>,
    /// TCP keep-alive; off also asks the server to close the connection.
    pub keep_alive: Option<bool>,
    /// Abort the download above this many bytes (0 = no limit).
    pub max_response_bytes: Option<u64>,
    /// Wait this long before sending (0 = send immediately).
    pub delay_ms: Option<u64>,
    /// Connect through a Unix domain socket (Linux, macOS; desktop only).
    pub unix_socket: Option<String>,
    /// Outgoing network interface or local address (desktop only).
    pub interface: Option<String>,
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
            http_version: Some(HttpVersion::Auto),
            resolve: Some(Vec::new()),
            connect_to: Some(Vec::new()),
            tls_min: Some(TlsVersion::Default),
            tls_max: Some(TlsVersion::Default),
            compression: Some(true),
            keep_alive: Some(true),
            max_response_bytes: Some(0),
            delay_ms: Some(0),
            unix_socket: Some(String::new()),
            interface: Some(String::new()),
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
            http_version: self.http_version.or(base.http_version),
            resolve: self.resolve.clone().or_else(|| base.resolve.clone()),
            connect_to: self.connect_to.clone().or_else(|| base.connect_to.clone()),
            tls_min: self.tls_min.or(base.tls_min),
            tls_max: self.tls_max.or(base.tls_max),
            compression: self.compression.or(base.compression),
            keep_alive: self.keep_alive.or(base.keep_alive),
            max_response_bytes: self.max_response_bytes.or(base.max_response_bytes),
            delay_ms: self.delay_ms.or(base.delay_ms),
            unix_socket: self
                .unix_socket
                .clone()
                .or_else(|| base.unix_socket.clone()),
            interface: self.interface.clone().or_else(|| base.interface.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names() {
        assert_eq!(
            serde_json::to_string(&HttpVersion::Http1_1).unwrap(),
            "\"http1.1\""
        );
        assert_eq!(
            serde_json::to_string(&HttpVersion::Http2PriorKnowledge).unwrap(),
            "\"http2PriorKnowledge\""
        );
        assert_eq!(
            serde_json::to_string(&TlsVersion::Tls1_2).unwrap(),
            "\"tls1.2\""
        );
    }

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
