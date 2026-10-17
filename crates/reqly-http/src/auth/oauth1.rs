//! OAuth 1.0 request signing (RFC 5849).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use reqly_core::model::OAuth1Method;
use url::Url;

use super::util::{encode, query_pairs};

pub struct Params<'a> {
    pub consumer_key: &'a str,
    pub consumer_secret: &'a str,
    pub token: &'a str,
    pub token_secret: &'a str,
    pub method: OAuth1Method,
    pub private_key: &'a str,
    pub callback: &'a str,
    pub verifier: &'a str,
    pub nonce: &'a str,
    pub timestamp: &'a str,
    pub version: bool,
    pub body_hash: Option<String>,
}

fn method_name(m: OAuth1Method) -> &'static str {
    match m {
        OAuth1Method::HmacSha1 => "HMAC-SHA1",
        OAuth1Method::HmacSha256 => "HMAC-SHA256",
        OAuth1Method::HmacSha512 => "HMAC-SHA512",
        OAuth1Method::RsaSha1 => "RSA-SHA1",
        OAuth1Method::RsaSha256 => "RSA-SHA256",
        OAuth1Method::Plaintext => "PLAINTEXT",
    }
}

/// The `oauth_*` protocol parameters (without the signature).
pub fn protocol_params(p: &Params) -> Vec<(String, String)> {
    let mut out = vec![
        ("oauth_consumer_key".to_string(), p.consumer_key.to_string()),
        ("oauth_nonce".to_string(), p.nonce.to_string()),
        (
            "oauth_signature_method".to_string(),
            method_name(p.method).to_string(),
        ),
        ("oauth_timestamp".to_string(), p.timestamp.to_string()),
    ];
    if !p.token.is_empty() {
        out.push(("oauth_token".into(), p.token.into()));
    }
    if p.version {
        out.push(("oauth_version".into(), "1.0".into()));
    }
    if !p.callback.is_empty() {
        out.push(("oauth_callback".into(), p.callback.into()));
    }
    if !p.verifier.is_empty() {
        out.push(("oauth_verifier".into(), p.verifier.into()));
    }
    if let Some(hash) = &p.body_hash {
        out.push(("oauth_body_hash".into(), hash.clone()));
    }
    out
}

/// Signature base string (RFC 5849 §3.4.1). `form` holds decoded form-body pairs.
pub fn base_string(
    method: &str,
    url: &str,
    protocol: &[(String, String)],
    form: &[(String, String)],
) -> Result<String, String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let port = match (parsed.port(), scheme.as_str()) {
        (Some(80), "http") | (Some(443), "https") | (None, _) => String::new(),
        (Some(p), _) => format!(":{p}"),
    };
    let base_uri = format!("{scheme}://{host}{port}{}", parsed.path());
    let mut pairs: Vec<(String, String)> = parsed
        .query()
        .map(query_pairs)
        .unwrap_or_default()
        .into_iter()
        .chain(form.iter().cloned())
        .chain(protocol.iter().cloned())
        .filter(|(k, _)| k != "oauth_signature")
        .map(|(k, v)| (encode(&k), encode(&v)))
        .collect();
    pairs.sort();
    let normalized = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!(
        "{}&{}&{}",
        method.to_uppercase(),
        encode(&base_uri),
        encode(&normalized)
    ))
}

pub fn signature(p: &Params, base: &str) -> Result<String, String> {
    let key = format!("{}&{}", encode(p.consumer_secret), encode(p.token_secret));
    macro_rules! hmac {
        ($h:ty) => {{
            let mut mac = Hmac::<$h>::new_from_slice(key.as_bytes()).map_err(|e| e.to_string())?;
            mac.update(base.as_bytes());
            STANDARD.encode(mac.finalize().into_bytes())
        }};
    }
    Ok(match p.method {
        OAuth1Method::HmacSha1 => hmac!(sha1::Sha1),
        OAuth1Method::HmacSha256 => hmac!(sha2::Sha256),
        OAuth1Method::HmacSha512 => hmac!(sha2::Sha512),
        OAuth1Method::Plaintext => key,
        OAuth1Method::RsaSha1 | OAuth1Method::RsaSha256 => {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            use rsa::pkcs8::DecodePrivateKey;
            use rsa::signature::{SignatureEncoding, Signer};
            let key = rsa::RsaPrivateKey::from_pkcs8_pem(p.private_key)
                .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(p.private_key))
                .map_err(|e| format!("invalid RSA private key: {e}"))?;
            let sig = if p.method == OAuth1Method::RsaSha1 {
                rsa::pkcs1v15::SigningKey::<sha1::Sha1>::new(key)
                    .sign(base.as_bytes())
                    .to_bytes()
            } else {
                rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(key)
                    .sign(base.as_bytes())
                    .to_bytes()
            };
            STANDARD.encode(sig)
        }
    })
}

/// `Authorization: OAuth …` header value.
pub fn header(realm: &str, protocol: &[(String, String)], signature: &str) -> String {
    let mut parts = Vec::new();
    if !realm.is_empty() {
        parts.push(format!("realm=\"{}\"", realm.replace('"', "")));
    }
    for (k, v) in protocol {
        parts.push(format!("{}=\"{}\"", encode(k), encode(v)));
    }
    parts.push(format!("oauth_signature=\"{}\"", encode(signature)));
    format!("OAuth {}", parts.join(", "))
}
