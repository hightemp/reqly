//! Akamai EdgeGrid (EG1-HMAC-SHA256).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::Digest;
use url::Url;

use super::util::hmac_sha256;

pub struct Params<'a> {
    pub client_token: &'a str,
    pub access_token: &'a str,
    pub client_secret: &'a str,
    pub headers_to_sign: Vec<String>,
    pub max_body: usize,
    /// `YYYYMMDDTHH:MM:SS+0000`.
    pub timestamp: &'a str,
    pub nonce: &'a str,
}

/// `YYYYMMDDTHH:MM:SS+0000` for Unix seconds.
pub fn timestamp(secs: i64) -> String {
    let (date, time) = super::util::utc_parts(secs);
    format!(
        "{date}T{}:{}:{}+0000",
        &time[0..2],
        &time[2..4],
        &time[4..6]
    )
}

pub fn signing_key(secret: &str, timestamp: &str) -> String {
    STANDARD.encode(hmac_sha256(secret.as_bytes(), timestamp.as_bytes()))
}

pub fn content_hash(method: &str, body: &[u8], max_body: usize) -> String {
    if method.eq_ignore_ascii_case("POST") && !body.is_empty() {
        let end = body.len().min(max_body);
        STANDARD.encode(sha2::Sha256::digest(&body[..end]))
    } else {
        String::new()
    }
}

fn canonical_headers(p: &Params, headers: &[(String, String)]) -> String {
    p.headers_to_sign
        .iter()
        .filter_map(|name| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| {
                    format!(
                        "{}:{}",
                        name.to_ascii_lowercase(),
                        v.split_whitespace().collect::<Vec<_>>().join(" ")
                    )
                })
        })
        .collect::<Vec<_>>()
        .join("\t")
}

/// The data that is signed (without the trailing unsigned header).
pub fn data_to_sign(
    p: &Params,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<String, String> {
    let u = Url::parse(url).map_err(|e| e.to_string())?;
    let host = match u.port() {
        Some(port) => format!("{}:{port}", u.host_str().unwrap_or("")),
        None => u.host_str().unwrap_or("").to_string(),
    };
    let path = match u.query() {
        Some(q) => format!("{}?{q}", u.path()),
        None => u.path().to_string(),
    };
    Ok(format!(
        "{}\t{}\t{host}\t{path}\t{}\t{}\t",
        method.to_uppercase(),
        u.scheme(),
        canonical_headers(p, headers),
        content_hash(method, body, p.max_body)
    ))
}

pub fn header(
    p: &Params,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<String, String> {
    let unsigned = format!(
        "EG1-HMAC-SHA256 client_token={};access_token={};timestamp={};nonce={};",
        p.client_token, p.access_token, p.timestamp, p.nonce
    );
    let data = data_to_sign(p, method, url, headers, body)? + &unsigned;
    let key = signing_key(p.client_secret, p.timestamp);
    let signature = STANDARD.encode(hmac_sha256(key.as_bytes(), data.as_bytes()));
    Ok(format!("{unsigned}signature={signature}"))
}
