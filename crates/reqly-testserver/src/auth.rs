//! Challenge-response auth endpoints: Digest (RFC 7616) and NTLMv2.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use md5::Md5;
use serde_json::json;
use sha2::{Digest, Sha256};

const REALM: &str = "reqly";
const NONCE: &str = "dcd98b7102dd2f0e8b11d0f600bfb0c093";

fn challenge(value: String) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, value)],
    )
        .into_response()
}

fn hex_digest(algorithm: &str, data: &[u8]) -> String {
    let bytes = if algorithm.eq_ignore_ascii_case("SHA-256") {
        Sha256::digest(data).to_vec()
    } else {
        Md5::digest(data).to_vec()
    };
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `Digest a="b", c=d` → map.
fn params(value: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = value.trim();
    while !rest.is_empty() {
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let key = key
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after = after.trim_start();
        let (val, next) = if let Some(quoted) = after.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            (&quoted[..end], quoted.get(end + 1..).unwrap_or(""))
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (after[..end].trim(), &after[end..])
        };
        out.insert(key, val.to_string());
        rest = next.trim_start_matches(',').trim();
    }
    out
}

/// `/digest/{qop}/{algorithm}/{user}/{pass}`: `qop` is `auth` or `auth-int`,
/// `algorithm` is `MD5` or `SHA-256`.
pub async fn digest(
    Path((qop, algorithm, user, pass)): Path<(String, String, String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ask = || {
        challenge(format!(
            "Digest realm=\"{REALM}\", qop=\"{qop}\", nonce=\"{NONCE}\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\", algorithm={algorithm}"
        ))
    };
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return ask();
    };
    let Some(rest) = value.strip_prefix("Digest ") else {
        return ask();
    };
    let p = params(rest);
    let get = |k: &str| p.get(k).cloned().unwrap_or_default();
    if get("username") != user || get("nonce") != NONCE || get("qop") != qop {
        return ask();
    }
    let ha1 = hex_digest(&algorithm, format!("{user}:{REALM}:{pass}").as_bytes());
    let ha2 = if qop == "auth-int" {
        hex_digest(
            &algorithm,
            format!("{method}:{}:{}", get("uri"), hex_digest(&algorithm, &body)).as_bytes(),
        )
    } else {
        hex_digest(&algorithm, format!("{method}:{}", get("uri")).as_bytes())
    };
    let expected = hex_digest(
        &algorithm,
        format!("{ha1}:{NONCE}:{}:{}:{qop}:{ha2}", get("nc"), get("cnonce")).as_bytes(),
    );
    if get("response") != expected || get("uri") != uri.to_string() {
        return ask();
    }
    axum::Json(json!({ "authenticated": true, "user": user, "qop": qop, "algorithm": algorithm }))
        .into_response()
}

const SERVER_CHALLENGE: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
}

fn hmac_md5(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Md5>::new_from_slice(key).expect("any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn type2() -> String {
    // Target info: MsvAvNbDomainName "REQLY", then MsvAvEOL.
    let domain = utf16("REQLY");
    let mut info = Vec::new();
    info.extend_from_slice(&2u16.to_le_bytes());
    info.extend_from_slice(&(domain.len() as u16).to_le_bytes());
    info.extend_from_slice(&domain);
    info.extend_from_slice(&[0, 0, 0, 0]);
    let flags: u32 = 0x0000_0001
        | 0x0000_0004
        | 0x0000_0200
        | 0x0000_8000
        | 0x0008_0000
        | 0x0080_0000
        | 0x2000_0000
        | 0x8000_0000;
    let mut msg = b"NTLMSSP\0".to_vec();
    msg.extend_from_slice(&2u32.to_le_bytes());
    msg.extend_from_slice(&[0, 0, 0, 0, 48, 0, 0, 0]); // empty target name at 48
    msg.extend_from_slice(&flags.to_le_bytes());
    msg.extend_from_slice(&SERVER_CHALLENGE);
    msg.extend_from_slice(&[0; 8]);
    msg.extend_from_slice(&(info.len() as u16).to_le_bytes());
    msg.extend_from_slice(&(info.len() as u16).to_le_bytes());
    msg.extend_from_slice(&48u32.to_le_bytes());
    msg.extend_from_slice(&info);
    format!("NTLM {}", STANDARD.encode(msg))
}

fn field(msg: &[u8], at: usize) -> Option<&[u8]> {
    let len = u16::from_le_bytes([*msg.get(at)?, *msg.get(at + 1)?]) as usize;
    let off = u32::from_le_bytes(msg.get(at + 4..at + 8)?.try_into().ok()?) as usize;
    msg.get(off..off + len)
}

fn from_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
}

/// `/ntlm/{user}/{pass}`: NTLM handshake that checks the NTLMv2 response.
pub async fn ntlm(Path((user, pass)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return challenge("NTLM".into());
    };
    let Some(msg) = value
        .strip_prefix("NTLM ")
        .and_then(|b| STANDARD.decode(b.trim()).ok())
    else {
        return challenge("NTLM".into());
    };
    if !msg.starts_with(b"NTLMSSP\0") || msg.len() < 12 {
        return challenge("NTLM".into());
    }
    match msg[8] {
        1 => challenge(type2()),
        3 => {
            let (Some(nt), Some(domain), Some(name)) =
                (field(&msg, 20), field(&msg, 28), field(&msg, 36))
            else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            let (domain, name) = (from_utf16(domain), from_utf16(name));
            if nt.len() <= 24 {
                return (StatusCode::FORBIDDEN, "NTLMv1 is not accepted").into_response();
            }
            let nt_hash = md4::Md4::digest(utf16(&pass));
            let key = hmac_md5(
                &nt_hash,
                &utf16(&format!("{}{domain}", name.to_uppercase())),
            );
            let mut data = SERVER_CHALLENGE.to_vec();
            data.extend_from_slice(&nt[16..]);
            let proof = hmac_md5(&key, &data);
            if name != user || proof != nt[..16] {
                return (StatusCode::FORBIDDEN, "bad NTLMv2 response").into_response();
            }
            axum::Json(json!({ "authenticated": true, "user": name, "domain": domain }))
                .into_response()
        }
        _ => challenge("NTLM".into()),
    }
}
