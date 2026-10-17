use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use curl::easy::{Easy2, Handler, HttpVersion, InfoType, List, WriteError};

use crate::{Req, Res, Timings};

struct Collector {
    body: Vec<u8>,
    headers: Vec<String>,
    timeline: Vec<String>,
    cancel: Arc<AtomicBool>,
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.body.extend_from_slice(data);
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        self.headers
            .push(String::from_utf8_lossy(data).trim_end().to_string());
        true
    }

    fn debug(&mut self, kind: InfoType, data: &[u8]) {
        let prefix = match kind {
            InfoType::Text => "* ",
            InfoType::HeaderIn => "< ",
            InfoType::HeaderOut => "> ",
            InfoType::DataOut => "| ",
            _ => return,
        };
        for line in String::from_utf8_lossy(data).lines() {
            self.timeline.push(format!("{prefix}{line}"));
        }
    }

    fn progress(&mut self, _: f64, _: f64, _: f64, _: f64) -> bool {
        !self.cancel.load(Ordering::Relaxed)
    }
}

pub fn send(req: &Req, cancel: Arc<AtomicBool>) -> Result<Res, String> {
    let mut easy = Easy2::new(Collector {
        body: Vec::new(),
        headers: Vec::new(),
        timeline: Vec::new(),
        cancel,
    });
    let e = |err: curl::Error| err.to_string();
    easy.url(&req.url).map_err(e)?;
    easy.custom_request(&req.method).map_err(e)?;
    easy.verbose(true).map_err(e)?;
    easy.progress(true).map_err(e)?;
    easy.follow_location(req.follow_redirects).map_err(e)?;
    easy.noproxy("*").map_err(e)?;
    easy.http_version(if req.http2 {
        HttpVersion::V2TLS
    } else {
        HttpVersion::V11
    })
    .map_err(e)?;
    if let Some(t) = req.timeout {
        easy.timeout(t).map_err(e)?;
    }
    if !req.body.is_empty() {
        easy.post_fields_copy(&req.body).map_err(e)?;
    }
    let mut list = List::new();
    for (k, v) in &req.headers {
        list.append(&format!("{k}: {v}")).map_err(e)?;
    }
    easy.http_headers(list).map_err(e)?;
    if let Some(ca) = &req.ca_pem {
        easy.ssl_cainfo_blob(ca.as_bytes()).map_err(e)?;
    }
    if let (Some(cert), Some(key)) = (&req.client_cert_pem, &req.client_key_pem) {
        easy.ssl_cert_blob(cert.as_bytes()).map_err(e)?;
        easy.ssl_cert_type("PEM").map_err(e)?;
        easy.ssl_key_blob(key.as_bytes()).map_err(e)?;
        easy.ssl_key_type("PEM").map_err(e)?;
    }

    easy.perform().map_err(e)?;

    let status = easy.response_code().map_err(e)? as u16;
    let dns = easy.namelookup_time().map_err(e)?;
    let connect = easy.connect_time().map_err(e)?;
    let tls = easy.appconnect_time().map_err(e)?;
    let ttfb = easy.starttransfer_time().map_err(e)?;
    let total = easy.total_time().map_err(e)?;
    let redirects = easy.redirect_count().map_err(e)?;
    let collector = easy.get_ref();

    // Only the headers of the final response (after redirects).
    let last_status = collector
        .headers
        .iter()
        .rposition(|h| h.starts_with("HTTP/"))
        .unwrap_or(0);
    let status_line = collector
        .headers
        .get(last_status)
        .cloned()
        .unwrap_or_default();
    let headers = collector.headers[last_status + 1..]
        .iter()
        .filter_map(|h| h.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect();

    let some = |d: Duration| (!d.is_zero()).then_some(d);
    Ok(Res {
        status,
        http_version: status_line
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_string(),
        headers,
        body: collector.body.clone(),
        timings: Timings {
            dns: some(dns),
            connect: some(connect.saturating_sub(dns)),
            tls: some(tls.saturating_sub(connect)),
            ttfb: some(ttfb),
            total,
        },
        timeline: collector.timeline.clone(),
        redirects,
    })
}
