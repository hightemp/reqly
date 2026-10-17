//! AWS Signature Version 4 (header signing).

use url::Url;

use super::util::{decode, encode, hex, hmac_sha256, query_pairs, sha256_hex, utc_parts};

pub struct Credentials<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub session_token: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

/// Removes `.` and `..` segments and empty segments (RFC 3986 normalization).
fn normalize_path(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut p = format!("/{}", out.join("/"));
    if path.ends_with('/') && p != "/" {
        p.push('/');
    }
    p
}

fn canonical_uri(path: &str, service: &str) -> String {
    let path = if service == "s3" {
        path.to_string()
    } else {
        normalize_path(path)
    };
    path.split('/')
        .map(|seg| encode(&decode(seg)))
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_query(query: Option<&str>) -> String {
    let mut pairs: Vec<(String, String)> = query
        .map(query_pairs)
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (encode(&k), encode(&v)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Headers that must not be signed: the engine may change or add them.
fn skip(name: &str) -> bool {
    matches!(
        name,
        "authorization" | "user-agent" | "expect" | "connection" | "accept-encoding"
    )
}

/// Signs a request. Returns headers to add (`Authorization`, `X-Amz-Date`, …).
/// `headers` are the request headers without `Host`; `body` is `None` when
/// the payload is not known (multipart), which signs `UNSIGNED-PAYLOAD`.
pub fn sign(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    creds: &Credentials,
    now_secs: i64,
    sign_body_header: bool,
) -> Result<Vec<(String, String)>, String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    let host = match (parsed.host_str(), parsed.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => return Err("the URL has no host".into()),
    };
    let (date, time) = utc_parts(now_secs);
    let amz_date = format!("{date}T{time}Z");
    let payload_hash = match body {
        Some(b) => sha256_hex(b),
        None => "UNSIGNED-PAYLOAD".to_string(),
    };

    let mut added = vec![("X-Amz-Date".to_string(), amz_date.clone())];
    if sign_body_header {
        added.push(("X-Amz-Content-Sha256".to_string(), payload_hash.clone()));
    }
    if !creds.session_token.is_empty() {
        added.push((
            "X-Amz-Security-Token".to_string(),
            creds.session_token.to_string(),
        ));
    }

    // Canonical headers: lowercase names, trimmed values with inner runs of
    // spaces collapsed, repeated headers joined with commas.
    let mut map: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    map.entry("host".into()).or_default().push(host);
    for (k, v) in headers.iter().chain(added.iter()) {
        let name = k.to_ascii_lowercase();
        if skip(&name) || name == "host" {
            continue;
        }
        let value = v.split_whitespace().collect::<Vec<_>>().join(" ");
        map.entry(name).or_default().push(value);
    }
    let canonical_headers: String = map
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.join(",")))
        .collect();
    let signed_headers = map.keys().cloned().collect::<Vec<_>>().join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_uppercase(),
        canonical_uri(parsed.path(), creds.service),
        canonical_query(parsed.query()),
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let scope = format!("{date}/{}/{}/aws4_request", creds.region, creds.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac_sha256(
        format!("AWS4{}", creds.secret_key).as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, creds.region.as_bytes());
    let k_service = hmac_sha256(&k_region, creds.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    added.push((
        "Authorization".to_string(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            creds.access_key
        ),
    ));
    Ok(added)
}
