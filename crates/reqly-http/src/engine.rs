//! libcurl execution of a [`PreparedRequest`] (ADR-002).
//!
//! Each request runs on tokio's blocking pool with its own `Easy2` handle.
//! The handler collects headers, the body (spilling to a temp file when
//! large) and a timeline from curl's debug callback; a progress callback
//! checks the [`CancelToken`].

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use curl::easy::{Easy2, Form, Handler, HttpVersion, InfoType, List, WriteError};
use reqly_core::model::{ProxyMode, ProxySettings, RequestSettings};
use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use ts_rs::TS;

use crate::ca;
use crate::prepare::{PreparedBody, PreparedRequest};

/// Request body bytes kept in the timeline (the rest is elided).
const TIMELINE_BODY_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub enum ClientIdentity {
    Pem { cert: String, key: String },
    Pkcs12 { der: Vec<u8>, passphrase: String },
}

#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Extra trusted CAs (PEM), added to the OS and Mozilla roots.
    pub ca_pems: Vec<String>,
    pub client_identity: Option<ClientIdentity>,
    /// Cookie jar contents in Netscape format, one cookie per line.
    pub cookies: Vec<String>,
    /// Response bodies larger than this go to a temp file.
    pub max_body_in_memory: usize,
    /// `host:port:address` pins (curl `--resolve`), used by the network policy.
    pub resolve: Vec<String>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            ca_pems: Vec::new(),
            client_identity: None,
            cookies: Vec::new(),
            max_body_in_memory: 4 * 1024 * 1024,
            resolve: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TimelineKind {
    /// Connection / TLS information from curl.
    Info,
    HeaderOut,
    DataOut,
    HeaderIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimelineEntry {
    pub kind: TimelineKind,
    pub text: String,
}

/// Phase durations in milliseconds; `None` when the phase did not happen
/// (e.g. reused connection, plain HTTP).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Timings {
    pub dns_ms: Option<f64>,
    pub connect_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    /// Time to first byte, from the start.
    pub ttfb_ms: Option<f64>,
    pub total_ms: f64,
}

#[derive(Debug)]
pub enum ResponseBody {
    Memory(Vec<u8>),
    File(NamedTempFile),
}

impl ResponseBody {
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }

    pub fn to_vec(&self) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Memory(b) => Ok(b.clone()),
            Self::File(f) => std::fs::read(f.path()),
        }
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    /// Headers of the final response (after redirects).
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
    /// Decoded body size in bytes.
    pub body_size: u64,
    pub headers_size: u64,
    pub timings: Timings,
    pub timeline: Vec<TimelineEntry>,
    pub redirects: u32,
    pub final_url: String,
    pub remote_addr: Option<String>,
    /// Cookie jar after the exchange (Netscape format), when storing is enabled.
    pub cookies: Vec<String>,
}

/// Progress callback: `(downloaded, total if known)`.
pub type ProgressFn = Box<dyn FnMut(u64, Option<u64>) + Send>;

struct Collector {
    memory: Vec<u8>,
    file: Option<NamedTempFile>,
    spill_at: usize,
    body_size: u64,
    /// Header lines of the response currently being received.
    headers: Vec<String>,
    timeline: Vec<TimelineEntry>,
    data_out_logged: usize,
    cancel: CancelToken,
    progress: Option<ProgressFn>,
    io_error: Option<std::io::Error>,
}

impl Collector {
    fn push_lines(&mut self, kind: TimelineKind, data: &[u8]) {
        for line in String::from_utf8_lossy(data).lines() {
            let line = line.trim_end();
            if !line.is_empty() || kind == TimelineKind::DataOut {
                self.timeline.push(TimelineEntry {
                    kind,
                    text: line.to_string(),
                });
            }
        }
    }
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.body_size += data.len() as u64;
        let result = if let Some(file) = self.file.as_mut() {
            file.write_all(data)
        } else if self.memory.len() + data.len() > self.spill_at {
            NamedTempFile::new().and_then(|mut file| {
                file.write_all(&self.memory)?;
                file.write_all(data)?;
                self.memory = Vec::new();
                self.file = Some(file);
                Ok(())
            })
        } else {
            self.memory.extend_from_slice(data);
            Ok(())
        };
        match result {
            Ok(()) => Ok(data.len()),
            Err(e) => {
                self.io_error = Some(e);
                Ok(0) // makes curl abort with a write error
            }
        }
    }

    fn header(&mut self, data: &[u8]) -> bool {
        let line = String::from_utf8_lossy(data).trim_end().to_string();
        if line.starts_with("HTTP/") {
            // A new response (redirect hop or after 100-continue) starts.
            self.headers.clear();
        }
        if !line.is_empty() {
            self.headers.push(line);
        }
        true
    }

    fn debug(&mut self, kind: InfoType, data: &[u8]) {
        match kind {
            InfoType::Text => self.push_lines(TimelineKind::Info, data),
            InfoType::HeaderOut => self.push_lines(TimelineKind::HeaderOut, data),
            InfoType::HeaderIn => self.push_lines(TimelineKind::HeaderIn, data),
            InfoType::DataOut => {
                let room = TIMELINE_BODY_LIMIT.saturating_sub(self.data_out_logged);
                if room > 0 {
                    let take = &data[..data.len().min(room)];
                    self.data_out_logged += take.len();
                    self.push_lines(TimelineKind::DataOut, take);
                    if self.data_out_logged >= TIMELINE_BODY_LIMIT {
                        self.timeline.push(TimelineEntry {
                            kind: TimelineKind::DataOut,
                            text: "[request body truncated]".into(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn progress(&mut self, dltotal: f64, dlnow: f64, _ultotal: f64, _ulnow: f64) -> bool {
        if let Some(cb) = self.progress.as_mut() {
            cb(dlnow as u64, (dltotal > 0.0).then_some(dltotal as u64));
        }
        !self.cancel.is_cancelled()
    }
}

fn map_curl_error(e: &curl::Error, collector: &Collector) -> CoreError {
    let code = if collector.cancel.is_cancelled() || e.is_aborted_by_callback() {
        "http.cancelled"
    } else if e.is_operation_timedout() {
        "http.timeout"
    } else if e.is_couldnt_resolve_host() {
        "http.dns"
    } else if e.is_couldnt_resolve_proxy() {
        "http.proxy"
    } else if e.is_couldnt_connect() {
        "http.connect"
    } else if e.is_too_many_redirects() {
        "http.too_many_redirects"
    } else if e.is_url_malformed() || e.is_unsupported_protocol() {
        "http.invalid_url"
    } else if e.is_peer_failed_verification()
        || e.is_ssl_connect_error()
        || e.is_ssl_certproblem()
        || e.is_ssl_cacert()
        || e.is_ssl_cacert_badfile()
        || e.is_ssl_cipher()
        || e.is_ssl_engine_notfound()
        || e.is_ssl_crl_badfile()
        || e.is_ssl_issuer_error()
    {
        "http.tls"
    } else if e.is_write_error() && collector.io_error.is_some() {
        "http.io"
    } else {
        "http.failed"
    };
    let mut message = e.description().to_string();
    if let Some(extra) = e.extra_description() {
        message = format!("{message}: {extra}");
    }
    if let Some(io) = &collector.io_error {
        message = format!("{message} ({io})");
    }
    CoreError::new(code, message.clone()).with_param("detail", message)
}

/// Runs `request` on the blocking pool.
pub async fn execute(
    request: PreparedRequest,
    options: ExecOptions,
    cancel: CancelToken,
    progress: Option<ProgressFn>,
) -> CoreResult<HttpResponse> {
    tokio::task::spawn_blocking(move || execute_blocking(&request, &options, cancel, progress))
        .await
        .map_err(|e| CoreError::internal(e.to_string()))?
}

fn configure_proxy(
    easy: &mut Easy2<Collector>,
    proxy: &ProxySettings,
    url: &str,
) -> Result<(), curl::Error> {
    match proxy.mode {
        ProxyMode::None => {
            // An empty proxy disables curl's own *_proxy environment lookup.
            easy.proxy("")?;
            easy.noproxy("*")?;
        }
        ProxyMode::System => {
            let env = |names: &[&str]| {
                names
                    .iter()
                    .find_map(|n| std::env::var(n).ok())
                    .unwrap_or_default()
            };
            let proxy_url = if url.starts_with("https://") {
                env(&["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"])
            } else {
                env(&["http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"])
            };
            easy.proxy(&proxy_url)?;
            easy.noproxy(&env(&["no_proxy", "NO_PROXY"]))?;
        }
        ProxyMode::Manual => {
            easy.proxy(&proxy.url)?;
            if !proxy.username.is_empty() {
                easy.proxy_username(&proxy.username)?;
                easy.proxy_password(&proxy.password)?;
            }
            easy.noproxy(&proxy.no_proxy)?;
        }
    }
    Ok(())
}

fn configure(
    easy: &mut Easy2<Collector>,
    request: &PreparedRequest,
    options: &ExecOptions,
) -> CoreResult<()> {
    let settings: &RequestSettings = &request.settings;
    let e = |err: curl::Error| CoreError::new("http.config", err.to_string());

    easy.url(&request.url).map_err(e)?;
    easy.verbose(true).map_err(e)?;
    easy.progress(true).map_err(e)?;
    easy.http_version(HttpVersion::V2TLS).map_err(e)?;
    // Advertise and transparently decode every supported encoding.
    easy.accept_encoding("").map_err(e)?;

    let timeout = settings.timeout_ms.unwrap_or(0);
    if timeout > 0 {
        easy.timeout(Duration::from_millis(timeout)).map_err(e)?;
    }
    if let Some(ms) = settings.connect_timeout_ms.filter(|ms| *ms > 0) {
        easy.connect_timeout(Duration::from_millis(ms)).map_err(e)?;
    }
    easy.follow_location(settings.follow_redirects.unwrap_or(true))
        .map_err(e)?;
    easy.max_redirections(settings.max_redirects.unwrap_or(10))
        .map_err(e)?;

    let verify = settings.verify_tls.unwrap_or(true);
    easy.ssl_verify_peer(verify).map_err(e)?;
    easy.ssl_verify_host(verify).map_err(e)?;
    easy.ssl_cainfo_blob(ca::bundle(&options.ca_pems).as_bytes())
        .map_err(e)?;
    match &options.client_identity {
        Some(ClientIdentity::Pem { cert, key }) => {
            easy.ssl_cert_blob(cert.as_bytes()).map_err(e)?;
            easy.ssl_cert_type("PEM").map_err(e)?;
            easy.ssl_key_blob(key.as_bytes()).map_err(e)?;
            easy.ssl_key_type("PEM").map_err(e)?;
        }
        Some(ClientIdentity::Pkcs12 { der, passphrase }) => {
            easy.ssl_cert_blob(der).map_err(e)?;
            easy.ssl_cert_type("P12").map_err(e)?;
            easy.key_password(passphrase).map_err(e)?;
        }
        None => {}
    }

    if !options.resolve.is_empty() {
        let mut list = List::new();
        for entry in &options.resolve {
            list.append(entry).map_err(e)?;
        }
        easy.resolve(list).map_err(e)?;
    }

    configure_proxy(
        easy,
        &settings.proxy.clone().unwrap_or_default(),
        &request.url,
    )
    .map_err(e)?;

    // Enable the in-memory cookie engine and seed it from the jar.
    easy.cookie_file("").map_err(e)?;
    if settings.send_cookies.unwrap_or(true) {
        for line in &options.cookies {
            easy.cookie_list(line).map_err(e)?;
        }
    }

    let mut headers = List::new();
    let has = |name: &str| {
        request
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    };
    for (k, v) in &request.headers {
        // `Name;` sends a header with an empty value; `Name:` would remove it.
        let line = if v.is_empty() {
            format!("{k};")
        } else {
            format!("{k}: {v}")
        };
        headers.append(&line).map_err(e)?;
    }
    if !has("User-Agent") {
        headers
            .append(&format!("User-Agent: reqly/{}", env!("CARGO_PKG_VERSION")))
            .map_err(e)?;
    }
    if !has("Expect") {
        headers.append("Expect:").map_err(e)?;
    }
    easy.http_headers(headers).map_err(e)?;

    let method = request.method.as_str();
    match &request.body {
        PreparedBody::None => match method {
            "GET" => easy.get(true).map_err(e)?,
            "HEAD" => easy.nobody(true).map_err(e)?,
            _ => easy.custom_request(method).map_err(e)?,
        },
        PreparedBody::Bytes(bytes) => {
            easy.post_fields_copy(bytes).map_err(e)?;
            if method != "POST" {
                easy.custom_request(method).map_err(e)?;
            }
        }
        PreparedBody::File(path) => {
            let bytes = std::fs::read(path).map_err(|err| {
                CoreError::new("http.body_file", err.to_string())
                    .with_param("path", path.to_string_lossy().into_owned())
            })?;
            easy.post_fields_copy(&bytes).map_err(e)?;
            if method != "POST" {
                easy.custom_request(method).map_err(e)?;
            }
        }
        PreparedBody::Blob(hash) => {
            return Err(CoreError::internal(format!("unresolved blob body {hash}")));
        }
        PreparedBody::Multipart(parts) => {
            let mut form = Form::new();
            for part in parts {
                let mut p = form.part(&part.name);
                if let Some(path) = &part.file {
                    if !path.exists() {
                        return Err(CoreError::new("http.body_file", "file not found")
                            .with_param("path", path.to_string_lossy().into_owned()));
                    }
                    p.file(path);
                } else if part.blob.is_some() {
                    return Err(CoreError::internal("unresolved blob in multipart body"));
                } else {
                    p.contents(part.value.as_bytes());
                }
                p.add()
                    .map_err(|err| CoreError::new("http.config", err.to_string()))?;
            }
            easy.httppost(form).map_err(e)?;
            if method != "POST" {
                easy.custom_request(method).map_err(e)?;
            }
        }
    }
    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn status_text(status_line: &str, status: u16) -> String {
    // "HTTP/1.1 404 Not Found" → "Not Found"; HTTP/2 has no reason phrase.
    let reason = status_line.splitn(3, ' ').nth(2).unwrap_or("").trim();
    if !reason.is_empty() {
        return reason.to_string();
    }
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("")
        .to_string()
}

pub fn execute_blocking(
    request: &PreparedRequest,
    options: &ExecOptions,
    cancel: CancelToken,
    progress: Option<ProgressFn>,
) -> CoreResult<HttpResponse> {
    let mut easy = Easy2::new(Collector {
        memory: Vec::new(),
        file: None,
        spill_at: options.max_body_in_memory,
        body_size: 0,
        headers: Vec::new(),
        timeline: Vec::new(),
        data_out_logged: 0,
        cancel,
        progress,
        io_error: None,
    });
    configure(&mut easy, request, options)?;

    if let Err(err) = easy.perform() {
        let collector = easy.get_ref();
        // Keep what happened so far: the UI shows it next to the error.
        let timeline = serde_json::to_value(&collector.timeline).unwrap_or_default();
        return Err(map_curl_error(&err, collector).with_param("timeline", timeline));
    }

    let info = |r: Result<Duration, curl::Error>| r.map(ms).unwrap_or(0.0);
    let dns = info(easy.namelookup_time());
    let connect = info(easy.connect_time());
    let tls = info(easy.appconnect_time());
    let ttfb = info(easy.starttransfer_time());
    let total = info(easy.total_time());
    let positive = |v: f64| (v > 0.0).then_some(v);

    let status = easy.response_code().unwrap_or(0) as u16;
    let redirects = easy.redirect_count().unwrap_or(0);
    let final_url = easy
        .effective_url()
        .ok()
        .flatten()
        .unwrap_or(&request.url)
        .to_string();
    let remote_addr = match (easy.primary_ip(), easy.primary_port()) {
        (Ok(Some(ip)), Ok(port)) => Some(format!("{ip}:{port}")),
        _ => None,
    };
    let headers_size = easy.header_size().unwrap_or(0);
    let store_cookies = request.settings.store_cookies.unwrap_or(true);
    let cookies = if store_cookies {
        easy.cookies()
            .map(|list| {
                list.iter()
                    .map(|c| String::from_utf8_lossy(c).into_owned())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let collector = easy.get_mut();
    let status_line = collector.headers.first().cloned().unwrap_or_default();
    let http_version = status_line
        .split(' ')
        .next()
        .unwrap_or_default()
        .to_string();
    let headers = collector
        .headers
        .iter()
        .skip(1)
        .filter_map(|h| h.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let body = match collector.file.take() {
        Some(mut file) => {
            file.flush()
                .map_err(|e| CoreError::new("http.io", e.to_string()))?;
            ResponseBody::File(file)
        }
        None => ResponseBody::Memory(std::mem::take(&mut collector.memory)),
    };

    Ok(HttpResponse {
        status,
        status_text: status_text(&status_line, status),
        http_version,
        headers,
        body,
        body_size: collector.body_size,
        headers_size,
        timings: Timings {
            dns_ms: positive(dns),
            connect_ms: positive(connect - dns),
            tls_ms: positive(tls - connect).filter(|_| tls > 0.0),
            ttfb_ms: positive(ttfb),
            total_ms: total,
        },
        timeline: std::mem::take(&mut collector.timeline),
        redirects,
        final_url,
        remote_addr,
        cookies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_from_line_or_code() {
        assert_eq!(status_text("HTTP/1.1 404 Not Found", 404), "Not Found");
        assert_eq!(status_text("HTTP/2 201", 201), "Created");
        assert_eq!(status_text("HTTP/1.1 299", 299), "");
    }

    #[test]
    fn cancel_token_is_shared() {
        let a = CancelToken::new();
        let b = a.clone();
        b.cancel();
        assert!(a.is_cancelled());
    }
}
