//! Hawk HTTP authentication (client header).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use reqly_core::model::HawkAlgorithm;
use sha2::Digest;
use url::Url;

pub struct Params<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub algorithm: HawkAlgorithm,
    pub ts: i64,
    pub nonce: &'a str,
    pub ext: &'a str,
    pub app: &'a str,
    pub dlg: &'a str,
}

fn digest(alg: HawkAlgorithm, data: &[u8]) -> String {
    match alg {
        HawkAlgorithm::Sha256 => STANDARD.encode(sha2::Sha256::digest(data)),
        HawkAlgorithm::Sha1 => STANDARD.encode(sha1::Sha1::digest(data)),
    }
}

/// Payload hash: `hawk.1.payload\n<content type>\n<payload>\n`.
pub fn payload_hash(alg: HawkAlgorithm, content_type: &str, payload: &[u8]) -> String {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut data = format!("hawk.1.payload\n{ct}\n").into_bytes();
    data.extend_from_slice(payload);
    data.push(b'\n');
    digest(alg, &data)
}

pub fn normalized(
    p: &Params,
    method: &str,
    url: &str,
    hash: Option<&str>,
) -> Result<String, String> {
    let u = Url::parse(url).map_err(|e| e.to_string())?;
    let resource = match u.query() {
        Some(q) => format!("{}?{q}", u.path()),
        None => u.path().to_string(),
    };
    let mut s = format!(
        "hawk.1.header\n{}\n{}\n{}\n{resource}\n{}\n{}\n{}\n{}\n",
        p.ts,
        p.nonce,
        method.to_uppercase(),
        u.host_str().unwrap_or("").to_ascii_lowercase(),
        u.port_or_known_default().unwrap_or(80),
        hash.unwrap_or(""),
        p.ext.replace('\\', "\\\\").replace('\n', "\\n"),
    );
    if !p.app.is_empty() {
        s.push_str(&format!("{}\n{}\n", p.app, p.dlg));
    }
    Ok(s)
}

pub fn header(p: &Params, method: &str, url: &str, hash: Option<&str>) -> Result<String, String> {
    let normalized = normalized(p, method, url, hash)?;
    let mac = match p.algorithm {
        HawkAlgorithm::Sha256 => {
            let mut m = Hmac::<sha2::Sha256>::new_from_slice(p.key.as_bytes())
                .map_err(|e| e.to_string())?;
            m.update(normalized.as_bytes());
            STANDARD.encode(m.finalize().into_bytes())
        }
        HawkAlgorithm::Sha1 => {
            let mut m =
                Hmac::<sha1::Sha1>::new_from_slice(p.key.as_bytes()).map_err(|e| e.to_string())?;
            m.update(normalized.as_bytes());
            STANDARD.encode(m.finalize().into_bytes())
        }
    };
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!(
        "Hawk id=\"{}\", ts=\"{}\", nonce=\"{}\"",
        q(p.id),
        p.ts,
        q(p.nonce)
    );
    if let Some(h) = hash {
        out.push_str(&format!(", hash=\"{h}\""));
    }
    if !p.ext.is_empty() {
        out.push_str(&format!(", ext=\"{}\"", q(p.ext)));
    }
    out.push_str(&format!(", mac=\"{mac}\""));
    if !p.app.is_empty() {
        out.push_str(&format!(", app=\"{}\"", q(p.app)));
        if !p.dlg.is_empty() {
            out.push_str(&format!(", dlg=\"{}\"", q(p.dlg)));
        }
    }
    Ok(out)
}
