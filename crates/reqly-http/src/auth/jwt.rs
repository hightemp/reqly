//! JWT bearer tokens and Atlassian ASAP.

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Map, Value};

fn key(alg: Algorithm, secret: &str) -> Result<EncodingKey, String> {
    let e = |e: jsonwebtoken::errors::Error| e.to_string();
    Ok(match alg {
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            EncodingKey::from_secret(secret.as_bytes())
        }
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512 => EncodingKey::from_rsa_pem(secret.as_bytes()).map_err(e)?,
        Algorithm::ES256 | Algorithm::ES384 => {
            EncodingKey::from_ec_pem(secret.as_bytes()).map_err(e)?
        }
        Algorithm::EdDSA => EncodingKey::from_ed_pem(secret.as_bytes()).map_err(e)?,
        #[allow(unreachable_patterns)]
        _ => return Err("unsupported algorithm".into()),
    })
}

fn object(text: &str, what: &str) -> Result<Map<String, Value>, String> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("the {what} must be a JSON object")),
        Err(e) => Err(format!("the {what} is not valid JSON: {e}")),
    }
}

/// Signs `payload` (a JSON object) with `alg`; `extra_header` adds header fields.
pub fn encode(
    algorithm: &str,
    secret: &str,
    payload: &str,
    extra_header: &str,
) -> Result<String, String> {
    let alg: Algorithm = algorithm
        .trim()
        .parse()
        .map_err(|_| format!("unsupported algorithm {algorithm}"))?;
    let claims = object(payload, "payload")?;
    let extra = object(extra_header, "header")?;
    let mut header = Header::new(alg);
    if let Some(Value::String(kid)) = extra.get("kid") {
        header.kid = Some(kid.clone());
    }
    if let Some(Value::String(typ)) = extra.get("typ") {
        header.typ = Some(typ.clone());
    }
    if let Some(Value::String(cty)) = extra.get("cty") {
        header.cty = Some(cty.clone());
    }
    jsonwebtoken::encode(&header, &Value::Object(claims), &key(alg, secret)?)
        .map_err(|e| e.to_string())
}

pub struct Asap<'a> {
    pub issuer: &'a str,
    pub subject: &'a str,
    pub audience: &'a str,
    pub key_id: &'a str,
    pub private_key: &'a str,
    pub additional_claims: &'a str,
    pub expires_in_secs: u64,
}

/// An RS256 ASAP token (`iss`, `sub`, `aud`, `jti`, `iat`, `exp` + extra claims).
pub fn asap(p: &Asap, now: i64, jti: &str) -> Result<String, String> {
    if p.key_id.trim().is_empty() {
        return Err("ASAP needs a key id".into());
    }
    let mut claims = object(p.additional_claims, "additional claims")?;
    claims.insert("iss".into(), Value::String(p.issuer.into()));
    if !p.subject.is_empty() {
        claims.insert("sub".into(), Value::String(p.subject.into()));
    }
    let audiences: Vec<Value> = p
        .audience
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(|a| Value::String(a.into()))
        .collect();
    claims.insert(
        "aud".into(),
        if audiences.len() == 1 {
            audiences[0].clone()
        } else {
            Value::Array(audiences)
        },
    );
    claims.insert("jti".into(), Value::String(jti.into()));
    claims.insert("iat".into(), Value::from(now));
    let ttl = if p.expires_in_secs == 0 {
        60
    } else {
        p.expires_in_secs.min(3600)
    } as i64;
    claims.insert("exp".into(), Value::from(now + ttl));
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(p.key_id.into());
    jsonwebtoken::encode(
        &header,
        &Value::Object(claims),
        &key(Algorithm::RS256, p.private_key)?,
    )
    .map_err(|e| e.to_string())
}
