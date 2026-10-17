//! HTTP Digest (RFC 7616): MD5, SHA-256 and their `-sess` variants,
//! `qop=auth` and `auth-int`.

use md5::Md5;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub algorithm: String,
    /// Offered qop values (`auth`, `auth-int`); empty = RFC 2069 style.
    pub qop: Vec<String>,
}

fn params(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
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
            let mut end = 0;
            let bytes = quoted.as_bytes();
            while end < bytes.len()
                && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\'))
            {
                end += 1;
            }
            (
                quoted[..end].replace("\\\"", "\""),
                quoted.get(end + 1..).unwrap_or(""),
            )
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (after[..end].trim().to_string(), &after[end..])
        };
        out.push((key, val));
        rest = next.trim_start_matches(',').trim();
    }
    out
}

/// Picks the strongest Digest challenge among `WWW-Authenticate` values.
pub fn parse(values: &[String]) -> Option<Challenge> {
    let mut best: Option<Challenge> = None;
    for v in values {
        let Some(rest) = v
            .trim()
            .strip_prefix("Digest ")
            .or_else(|| v.trim().strip_prefix("digest "))
        else {
            continue;
        };
        let mut c = Challenge {
            algorithm: "MD5".into(),
            ..Challenge::default()
        };
        for (k, val) in params(rest) {
            match k.as_str() {
                "realm" => c.realm = val,
                "nonce" => c.nonce = val,
                "opaque" => c.opaque = Some(val),
                "algorithm" => c.algorithm = val,
                "qop" => {
                    c.qop = val
                        .split(',')
                        .map(|q| q.trim().to_ascii_lowercase())
                        .collect()
                }
                _ => {}
            }
        }
        let strong = c.algorithm.to_ascii_uppercase().starts_with("SHA-256");
        if best.is_none() || strong {
            best = Some(c);
        }
    }
    best
}

fn h(algorithm: &str, data: &str) -> String {
    let bytes = if algorithm.to_ascii_uppercase().starts_with("SHA-256") {
        Sha256::digest(data.as_bytes()).to_vec()
    } else {
        Md5::digest(data.as_bytes()).to_vec()
    };
    super::util::hex(&bytes)
}

fn h_bytes(algorithm: &str, data: &[u8]) -> String {
    let bytes = if algorithm.to_ascii_uppercase().starts_with("SHA-256") {
        Sha256::digest(data).to_vec()
    } else {
        Md5::digest(data).to_vec()
    };
    super::util::hex(&bytes)
}

/// The `Authorization` value. `body` is needed for `auth-int`.
#[allow(clippy::too_many_arguments)]
pub fn authorization(
    c: &Challenge,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    body: Option<&[u8]>,
    cnonce: &str,
    nc: u32,
) -> Result<String, String> {
    let alg = &c.algorithm;
    let qop = if c.qop.iter().any(|q| q == "auth") {
        Some("auth")
    } else if c.qop.iter().any(|q| q == "auth-int") {
        Some("auth-int")
    } else if c.qop.is_empty() {
        None
    } else {
        return Err(format!("unsupported qop {}", c.qop.join(",")));
    };
    let mut ha1 = h(alg, &format!("{username}:{}:{password}", c.realm));
    if alg.to_ascii_lowercase().ends_with("-sess") {
        ha1 = h(alg, &format!("{ha1}:{}:{cnonce}", c.nonce));
    }
    let ha2 = match qop {
        Some("auth-int") => {
            let body = body.ok_or("auth-int needs a known body")?;
            h(
                alg,
                &format!("{}:{uri}:{}", method.to_uppercase(), h_bytes(alg, body)),
            )
        }
        _ => h(alg, &format!("{}:{uri}", method.to_uppercase())),
    };
    let nc = format!("{nc:08x}");
    let response = match qop {
        Some(q) => h(alg, &format!("{ha1}:{}:{nc}:{cnonce}:{q}:{ha2}", c.nonce)),
        None => h(alg, &format!("{ha1}:{}:{ha2}", c.nonce)),
    };
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", algorithm={alg}, response=\"{response}\"",
        q(username),
        q(&c.realm),
        q(&c.nonce),
        q(uri)
    );
    if let Some(qop) = qop {
        out.push_str(&format!(", qop={qop}, nc={nc}, cnonce=\"{}\"", q(cnonce)));
    }
    if let Some(opaque) = &c.opaque {
        out.push_str(&format!(", opaque=\"{}\"", q(opaque)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7616 §3.9.1 example (MD5 and SHA-256).
    #[test]
    fn rfc7616_examples() {
        let mut c = Challenge {
            realm: "http-auth@example.org".into(),
            nonce: "7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v".into(),
            opaque: Some("FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS".into()),
            algorithm: "MD5".into(),
            qop: vec!["auth".into(), "auth-int".into()],
        };
        let cnonce = "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ";
        let md5 = authorization(
            &c,
            "Mufasa",
            "Circle of Life",
            "GET",
            "/dir/index.html",
            None,
            cnonce,
            1,
        )
        .unwrap();
        assert!(
            md5.contains("response=\"8ca523f5e9506fed4657c9700eebdbec\""),
            "{md5}"
        );
        c.algorithm = "SHA-256".into();
        let sha = authorization(
            &c,
            "Mufasa",
            "Circle of Life",
            "GET",
            "/dir/index.html",
            None,
            cnonce,
            1,
        )
        .unwrap();
        assert!(
            sha.contains(
                "response=\"753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1\""
            ),
            "{sha}"
        );
    }

    #[test]
    fn parses_and_prefers_sha256() {
        let c = parse(&[
            "Digest realm=\"r\", qop=\"auth,auth-int\", nonce=\"n\", algorithm=MD5".into(),
            "Digest realm=\"r\", qop=\"auth\", nonce=\"n2\", algorithm=SHA-256, opaque=\"o\""
                .into(),
            "Basic realm=\"x\"".into(),
        ])
        .unwrap();
        assert_eq!(c.algorithm, "SHA-256");
        assert_eq!(c.nonce, "n2");
        assert_eq!(c.opaque.as_deref(), Some("o"));
    }
}
