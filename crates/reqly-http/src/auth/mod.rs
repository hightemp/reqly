//! Authentication that needs signing, I/O or the engine: applied to a
//! prepared request by [`finalize`] after [`crate::prepare`].

pub mod digest;
pub mod edgegrid;
pub mod hawk;
pub mod jwt;
pub mod netrc;
pub mod ntlm;
pub mod oauth1;
pub mod sigv4;
mod util;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqly_core::model::{Auth, JwtLocation, OAuth1Location, OAuth2Config};
use reqly_core::{CoreError, CoreResult};
use sha1::Digest;

use crate::prepare::{EngineAuth, PreparedBody, PreparedRequest};

/// Time, randomness and environment for signing (fixed in tests).
#[derive(Debug, Clone)]
pub struct SignContext {
    /// Unix seconds.
    pub now: i64,
    /// A fresh random value for nonces and token ids.
    pub nonce: String,
    /// Reading local files (netrc) is allowed (desktop).
    pub allow_local_files: bool,
    /// The OAuth 2.0 access token resolved by the caller.
    pub oauth2_token: Option<String>,
}

impl SignContext {
    pub fn system(allow_local_files: bool) -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("OS randomness");
        Self {
            now: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            nonce: util::hex(&bytes),
            allow_local_files,
            oauth2_token: None,
        }
    }
}

fn auth_error(kind: &str, message: impl Into<String>) -> CoreError {
    CoreError::new("auth.failed", message.into()).with_param("type", kind)
}

fn set_header(req: &mut PreparedRequest, name: &str, value: String) {
    req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    req.headers.push((name.to_string(), value));
}

fn header<'a>(req: &'a PreparedRequest, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Body bytes for hashing; `None` when they are not known up front (multipart).
fn body_bytes(req: &PreparedRequest) -> CoreResult<Option<Vec<u8>>> {
    Ok(match &req.body {
        PreparedBody::None => Some(Vec::new()),
        PreparedBody::Bytes(b) => Some(b.clone()),
        PreparedBody::File(path) => Some(std::fs::read(path).map_err(|e| {
            CoreError::new("http.body_file", e.to_string())
                .with_param("path", path.display().to_string())
        })?),
        PreparedBody::Blob(_) | PreparedBody::Multipart(_) => None,
    })
}

fn is_form(req: &PreparedRequest) -> bool {
    header(req, "content-type").is_some_and(|c| {
        c.to_ascii_lowercase()
            .starts_with("application/x-www-form-urlencoded")
    })
}

fn add_query(url: &str, pairs: &[(String, String)]) -> String {
    let joined = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", util::encode(k), util::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    if joined.is_empty() {
        return url.to_string();
    }
    let (base, fragment) = match url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (url, None),
    };
    let sep = if base.contains('?') { "&" } else { "?" };
    let mut out = format!("{base}{sep}{joined}");
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

/// `Authorization: <prefix> <token>` for OAuth 2.0.
pub fn bearer(config: &OAuth2Config, token: &str) -> String {
    let prefix = if config.token_prefix.trim().is_empty() {
        "Bearer"
    } else {
        config.token_prefix.trim()
    };
    format!("{prefix} {token}")
}

/// Applies the pending auth of `req`.
pub fn finalize(req: &mut PreparedRequest, ctx: &SignContext) -> CoreResult<()> {
    let Some(auth) = req.auth.take() else {
        return Ok(());
    };
    match auth {
        Auth::Inherit
        | Auth::None
        | Auth::Basic { .. }
        | Auth::Bearer { .. }
        | Auth::ApiKey { .. } => {}
        Auth::Digest { username, password } => {
            req.engine_auth = Some(EngineAuth::Digest { username, password });
        }
        Auth::Ntlm {
            username,
            password,
            domain,
        } => {
            let username = if domain.trim().is_empty() {
                username
            } else {
                format!("{}\\{username}", domain.trim())
            };
            req.engine_auth = Some(EngineAuth::Ntlm { username, password });
        }
        Auth::OAuth2(config) => {
            let token = ctx.oauth2_token.clone().ok_or_else(|| {
                CoreError::new("auth.oauth2_no_token", "get an OAuth 2.0 token first")
            })?;
            set_header(req, "Authorization", bearer(&config, &token));
        }
        Auth::OAuth1 {
            consumer_key,
            consumer_secret,
            token,
            token_secret,
            signature_method,
            private_key,
            realm,
            callback,
            verifier,
            nonce,
            timestamp,
            location,
            include_body_hash,
        } => {
            let form_body = is_form(req);
            let body_hash = if include_body_hash && !form_body {
                body_bytes(req)?.map(|b| STANDARD.encode(sha1::Sha1::digest(b)))
            } else {
                None
            };
            let timestamp = if timestamp.trim().is_empty() {
                ctx.now.to_string()
            } else {
                timestamp
            };
            let nonce = if nonce.trim().is_empty() {
                ctx.nonce.clone()
            } else {
                nonce
            };
            let params = oauth1::Params {
                consumer_key: &consumer_key,
                consumer_secret: &consumer_secret,
                token: &token,
                token_secret: &token_secret,
                method: signature_method,
                private_key: &private_key,
                callback: &callback,
                verifier: &verifier,
                nonce: &nonce,
                timestamp: &timestamp,
                version: true,
                body_hash,
            };
            let protocol = oauth1::protocol_params(&params);
            let form: Vec<(String, String)> = match (&req.body, form_body) {
                (PreparedBody::Bytes(b), true) => util::query_pairs(&String::from_utf8_lossy(b)),
                _ => Vec::new(),
            };
            let base = oauth1::base_string(&req.method, &req.url, &protocol, &form)
                .map_err(|e| auth_error("oauth1", e))?;
            let signature =
                oauth1::signature(&params, &base).map_err(|e| auth_error("oauth1", e))?;
            match location {
                OAuth1Location::Header => set_header(
                    req,
                    "Authorization",
                    oauth1::header(&realm, &protocol, &signature),
                ),
                OAuth1Location::Query => {
                    let mut pairs = protocol;
                    pairs.push(("oauth_signature".into(), signature));
                    req.url = add_query(&req.url, &pairs);
                }
                OAuth1Location::Body => {
                    let PreparedBody::Bytes(b) = &mut req.body else {
                        return Err(auth_error(
                            "oauth1",
                            "parameters in the body need a form body",
                        ));
                    };
                    if !form_body {
                        return Err(auth_error(
                            "oauth1",
                            "parameters in the body need a form body",
                        ));
                    }
                    let mut pairs = protocol;
                    pairs.push(("oauth_signature".into(), signature));
                    let extra = pairs
                        .iter()
                        .map(|(k, v)| format!("{}={}", util::encode(k), util::encode(v)))
                        .collect::<Vec<_>>()
                        .join("&");
                    if !b.is_empty() {
                        b.push(b'&');
                    }
                    b.extend_from_slice(extra.as_bytes());
                }
            }
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            session_token,
            region,
            service,
        } => {
            let body = body_bytes(req)?;
            let creds = sigv4::Credentials {
                access_key: &access_key,
                secret_key: &secret_key,
                session_token: &session_token,
                region: &region,
                service: &service,
            };
            let added = sigv4::sign(
                &req.method,
                &req.url,
                &req.headers,
                body.as_deref(),
                &creds,
                ctx.now,
                service == "s3",
            )
            .map_err(|e| auth_error("awsSigV4", e))?;
            for (k, v) in added {
                set_header(req, &k, v);
            }
        }
        Auth::Hawk {
            id,
            key,
            algorithm,
            ext,
            app,
            dlg,
            include_payload_hash,
        } => {
            let hash = if include_payload_hash {
                let ct = header(req, "content-type").unwrap_or("").to_string();
                body_bytes(req)?.map(|b| hawk::payload_hash(algorithm, &ct, &b))
            } else {
                None
            };
            let nonce: String = ctx.nonce.chars().take(12).collect();
            let params = hawk::Params {
                id: &id,
                key: &key,
                algorithm,
                ts: ctx.now,
                nonce: &nonce,
                ext: &ext,
                app: &app,
                dlg: &dlg,
            };
            let value = hawk::header(&params, &req.method, &req.url, hash.as_deref())
                .map_err(|e| auth_error("hawk", e))?;
            set_header(req, "Authorization", value);
        }
        Auth::Asap {
            issuer,
            subject,
            audience,
            key_id,
            private_key,
            additional_claims,
            expires_in_secs,
        } => {
            let token = jwt::asap(
                &jwt::Asap {
                    issuer: &issuer,
                    subject: &subject,
                    audience: &audience,
                    key_id: &key_id,
                    private_key: &private_key,
                    additional_claims: &additional_claims,
                    expires_in_secs,
                },
                ctx.now,
                &ctx.nonce,
            )
            .map_err(|e| auth_error("asap", e))?;
            set_header(req, "Authorization", format!("Bearer {token}"));
        }
        Auth::Jwt {
            algorithm,
            secret,
            payload,
            header: extra,
            prefix,
            location,
            query_param,
        } => {
            let token = jwt::encode(&algorithm, &secret, &payload, &extra)
                .map_err(|e| auth_error("jwt", e))?;
            match location {
                JwtLocation::Header => {
                    let prefix = if prefix.trim().is_empty() {
                        "Bearer".to_string()
                    } else {
                        prefix.trim().to_string()
                    };
                    set_header(req, "Authorization", format!("{prefix} {token}"));
                }
                JwtLocation::Query => {
                    let name = if query_param.trim().is_empty() {
                        "token"
                    } else {
                        query_param.trim()
                    };
                    req.url = add_query(&req.url, &[(name.to_string(), token)]);
                }
            }
        }
        Auth::Netrc { file } => {
            if !ctx.allow_local_files {
                return Err(CoreError::new(
                    "http.not_allowed",
                    "netrc files are not available on the server",
                )
                .with_param("setting", "netrc"));
            }
            let path = if file.trim().is_empty() {
                netrc::default_path().ok_or_else(|| auth_error("netrc", "no home directory"))?
            } else {
                std::path::PathBuf::from(file.trim())
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|e| auth_error("netrc", format!("{}: {e}", path.display())))?;
            let host = url::Url::parse(&req.url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default();
            if let Some((login, password)) = netrc::lookup(&text, &host) {
                set_header(
                    req,
                    "Authorization",
                    format!("Basic {}", STANDARD.encode(format!("{login}:{password}"))),
                );
            }
        }
        Auth::EdgeGrid {
            access_token,
            client_token,
            client_secret,
            headers_to_sign,
            max_body,
        } => {
            let body = body_bytes(req)?.unwrap_or_default();
            let ts = edgegrid::timestamp(ctx.now);
            let params = edgegrid::Params {
                client_token: &client_token,
                access_token: &access_token,
                client_secret: &client_secret,
                headers_to_sign: headers_to_sign
                    .split(',')
                    .map(|h| h.trim().to_string())
                    .filter(|h| !h.is_empty())
                    .collect(),
                max_body: if max_body == 0 {
                    131_072
                } else {
                    max_body as usize
                },
                timestamp: &ts,
                nonce: &ctx.nonce,
            };
            let value = edgegrid::header(&params, &req.method, &req.url, &req.headers, &body)
                .map_err(|e| auth_error("edgeGrid", e))?;
            set_header(req, "Authorization", value);
        }
    }
    Ok(())
}
