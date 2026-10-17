//! Spike for ADR-002. Two minimal engines with the same API so the same
//! tests measure what each can provide.

use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct Req {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub ca_pem: Option<String>,
    pub client_cert_pem: Option<String>,
    pub client_key_pem: Option<String>,
    pub http2: bool,
    pub timeout: Option<Duration>,
    pub follow_redirects: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Timings {
    pub dns: Option<Duration>,
    pub connect: Option<Duration>,
    pub tls: Option<Duration>,
    pub ttfb: Option<Duration>,
    pub total: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct Res {
    pub status: u16,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timings: Timings,
    /// Raw lines as sent/received (`> ` outgoing, `< ` incoming, `* ` info).
    pub timeline: Vec<String>,
    pub redirects: u32,
}

#[cfg(feature = "engine-curl")]
pub mod curl_engine;
#[cfg(feature = "engine-reqwest")]
pub mod reqwest_engine;
