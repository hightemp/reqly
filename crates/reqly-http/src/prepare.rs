//! Turns a stored [`HttpRequest`] plus its ancestor folders into the exact
//! request to put on the wire. Pure and deterministic — no I/O.

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{AsciiSet, CONTROLS, NON_ALPHANUMERIC, utf8_percent_encode};
use reqly_core::model::{
    ApiKeyLocation, Auth, BodyKind, FileRef, FolderData, HttpRequest, KeyValue, KeyValueKind,
    RequestSettings,
};
use reqly_core::{CoreError, CoreResult};

/// Encodes everything except RFC 3986 unreserved characters.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Light encoding for already-structured parts (path, raw query): only
/// characters that are never valid in a URL.
const LENIENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'^')
    .add(b'\\');

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartPart {
    pub name: String,
    pub value: String,
    /// Local file to upload instead of `value`.
    pub file: Option<PathBuf>,
    /// Uploaded blob (server mode) to upload instead of `value`.
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PreparedBody {
    #[default]
    None,
    Bytes(Vec<u8>),
    File(PathBuf),
    Blob(String),
    Multipart(Vec<MultipartPart>),
}

/// Challenge-response schemes the engine negotiates itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineAuth {
    Digest { username: String, password: String },
    Ntlm { username: String, password: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: PreparedBody,
    /// Fully resolved: every field is `Some`.
    pub settings: RequestSettings,
    /// Auth that needs signing or I/O; applied by [`crate::auth::finalize`].
    pub auth: Option<Auth>,
    pub engine_auth: Option<EngineAuth>,
}

/// Prepares `request` inside `folders` (outermost first) with built-in defaults.
pub fn prepare(request: &HttpRequest, folders: &[&FolderData]) -> CoreResult<PreparedRequest> {
    prepare_with_base(request, folders, &RequestSettings::defaults())
}

/// Like [`prepare`], with `base` settings (global + collection) at the top of the chain.
pub fn prepare_with_base(
    request: &HttpRequest,
    folders: &[&FolderData],
    base: &RequestSettings,
) -> CoreResult<PreparedRequest> {
    let settings = folders
        .iter()
        .fold(base.merge(&RequestSettings::defaults()), |acc, f| {
            f.settings.merge(&acc)
        });
    let settings = request.settings.merge(&settings);
    let encode = settings.encode_url.unwrap_or(true);

    let method = match request.method.trim() {
        "" => "GET".to_string(),
        m => m.to_uppercase(),
    };

    let mut headers = merge_headers(
        folders
            .iter()
            .map(|f| f.headers.as_slice())
            .chain(std::iter::once(request.headers.as_slice())),
    );

    let mut query: Vec<(String, String)> = enabled(&request.params)
        .map(|kv| (kv.name.clone(), kv.value.clone()))
        .collect();

    let pending = apply_auth(effective_auth(request, folders), &mut headers, &mut query);

    // GraphQL: a JSON body, or query parameters for GET.
    let graphql = if request.body.kind == BodyKind::Graphql {
        Some(graphql_payload(&request.body.graphql)?)
    } else {
        None
    };
    if let Some(payload) = graphql.as_ref().filter(|_| method == "GET") {
        for (k, v) in payload.as_object().into_iter().flatten() {
            let v = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            query.push((k.clone(), v));
        }
    }

    let url = build_url(&request.url, &request.path_params, &query, encode)?;

    let body = match graphql {
        Some(_) if method == "GET" => PreparedBody::None,
        Some(payload) => {
            if !has_header(&headers, "Content-Type") {
                headers.push(("Content-Type".into(), "application/json".into()));
            }
            PreparedBody::Bytes(payload.to_string().into_bytes())
        }
        None => build_body(request, &mut headers),
    };

    Ok(PreparedRequest {
        method,
        url,
        headers,
        body,
        settings,
        auth: pending,
        engine_auth: None,
    })
}

/// `{"query", "variables"?, "operationName"?}` of a GraphQL operation.
pub fn graphql_payload(g: &reqly_core::model::GraphqlBody) -> CoreResult<serde_json::Value> {
    let mut payload = serde_json::json!({ "query": g.query });
    if !g.variables.trim().is_empty() {
        let variables: serde_json::Value = serde_json::from_str(&g.variables).map_err(|e| {
            CoreError::new(
                "graphql.invalid_variables",
                format!("variables are not valid JSON: {e}"),
            )
            .with_param("message", e.to_string())
        })?;
        if !variables.is_object() && !variables.is_null() {
            return Err(CoreError::new(
                "graphql.invalid_variables",
                "variables must be a JSON object",
            )
            .with_param("message", "not an object"));
        }
        payload["variables"] = variables;
    }
    if !g.operation_name.trim().is_empty() {
        payload["operationName"] = serde_json::json!(g.operation_name.trim());
    }
    Ok(payload)
}

fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter().filter(|kv| kv.enabled && !kv.name.is_empty())
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// Each level replaces same-named headers of outer levels (case-insensitive);
/// duplicates within one level are kept.
fn merge_headers<'a>(levels: impl Iterator<Item = &'a [KeyValue]>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for level in levels {
        let rows: Vec<&KeyValue> = enabled(level).collect();
        out.retain(|(k, _)| !rows.iter().any(|r| r.name.eq_ignore_ascii_case(k)));
        out.extend(rows.into_iter().map(|r| (r.name.clone(), r.value.clone())));
    }
    out
}

fn effective_auth<'a>(request: &'a HttpRequest, folders: &[&'a FolderData]) -> &'a Auth {
    if !matches!(request.auth, Auth::Inherit) {
        return &request.auth;
    }
    folders
        .iter()
        .rev()
        .map(|f| &f.auth)
        .find(|a| !matches!(a, Auth::Inherit))
        .unwrap_or(&Auth::None)
}

/// Applies static schemes; returns the ones that need [`crate::auth::finalize`].
fn apply_auth(
    auth: &Auth,
    headers: &mut Vec<(String, String)>,
    query: &mut Vec<(String, String)>,
) -> Option<Auth> {
    let mut set_header = |name: &str, value: String| {
        if !has_header(headers, name) {
            headers.push((name.to_string(), value));
        }
    };
    match auth {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => {
            set_header(
                "Authorization",
                format!(
                    "Basic {}",
                    STANDARD.encode(format!("{username}:{password}"))
                ),
            );
        }
        Auth::Bearer { token, prefix } => {
            let prefix = if prefix.trim().is_empty() {
                "Bearer"
            } else {
                prefix.trim()
            };
            set_header("Authorization", format!("{prefix} {token}"));
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } if !key.is_empty() => match location {
            ApiKeyLocation::Header => set_header(key, value.clone()),
            ApiKeyLocation::Query => query.push((key.clone(), value.clone())),
        },
        Auth::ApiKey { .. } => {}
        other => return Some(other.clone()),
    }
    None
}

/// Percent-encodes with `set`, keeping valid `%XX` escapes untouched so
/// pre-encoded input is not double-encoded.
fn encode_keep_escapes(input: &str, set: &'static AsciiSet) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
            && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        {
            out.extend(utf8_percent_encode(&input[start..i], set));
            out.push_str(&input[i..i + 3]);
            i += 3;
            start = i;
        } else {
            i += 1;
        }
    }
    out.extend(utf8_percent_encode(&input[start..], set));
    out
}

fn build_url(
    raw: &str,
    path_params: &[KeyValue],
    query: &[(String, String)],
    encode: bool,
) -> CoreResult<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CoreError::new("http.invalid_url", "URL is empty"));
    }
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };

    let (before_fragment, fragment) = match with_scheme.split_once('#') {
        Some((a, f)) => (a.to_string(), Some(f.to_string())),
        None => (with_scheme, None),
    };
    let (base, existing_query) = match before_fragment.split_once('?') {
        Some((b, q)) => (b.to_string(), Some(q.to_string())),
        None => (before_fragment, None),
    };

    // scheme://authority/path
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let path_start = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    let (origin, path) = base.split_at(path_start);

    let path = path
        .split('/')
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':')
                && let Some(param) = enabled(path_params).find(|p| p.name == name)
            {
                return if encode {
                    encode_keep_escapes(&param.value, COMPONENT)
                } else {
                    param.value.clone()
                };
            }
            if encode {
                encode_keep_escapes(segment, LENIENT)
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/");

    let mut parts: Vec<String> = Vec::new();
    if let Some(q) = existing_query.filter(|q| !q.is_empty()) {
        parts.push(if encode {
            encode_keep_escapes(&q, LENIENT)
        } else {
            q
        });
    }
    for (k, v) in query {
        parts.push(if encode {
            format!(
                "{}={}",
                encode_keep_escapes(k, COMPONENT),
                encode_keep_escapes(v, COMPONENT)
            )
        } else {
            format!("{k}={v}")
        });
    }

    let mut url = format!("{origin}{path}");
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    if let Some(f) = fragment {
        url.push('#');
        url.push_str(&f);
    }
    Ok(url)
}

/// `application/x-www-form-urlencoded` serialization (space as `+`).
fn form_encode(fields: &[KeyValue]) -> String {
    let enc = |s: &str| {
        utf8_percent_encode(s, COMPONENT)
            .to_string()
            .replace("%20", "+")
    };
    enabled(fields)
        .map(|kv| format!("{}={}", enc(&kv.name), enc(&kv.value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn build_body(request: &HttpRequest, headers: &mut Vec<(String, String)>) -> PreparedBody {
    let body = &request.body;
    if let Some(ct) = body.kind.default_content_type()
        && !has_header(headers, "Content-Type")
    {
        headers.push(("Content-Type".into(), ct.into()));
    }
    match body.kind {
        BodyKind::None => PreparedBody::None,
        kind if kind.is_text() => PreparedBody::Bytes(body.text.clone().into_bytes()),
        BodyKind::FormUrlencoded => PreparedBody::Bytes(form_encode(&body.fields).into_bytes()),
        BodyKind::Multipart => PreparedBody::Multipart(
            enabled(&body.fields)
                .map(|kv| {
                    let is_file = kv.kind == KeyValueKind::File;
                    let blob = kv
                        .value
                        .strip_prefix("blob:")
                        .filter(|_| is_file)
                        .map(str::to_string);
                    MultipartPart {
                        name: kv.name.clone(),
                        value: kv.value.clone(),
                        file: (is_file && blob.is_none()).then(|| PathBuf::from(&kv.value)),
                        blob,
                    }
                })
                .collect(),
        ),
        BodyKind::Binary => match &body.file {
            Some(FileRef::Path { path }) => PreparedBody::File(PathBuf::from(path)),
            Some(FileRef::Blob { hash, .. }) => PreparedBody::Blob(hash.clone()),
            None => PreparedBody::None,
        },
        _ => PreparedBody::None,
    }
}
