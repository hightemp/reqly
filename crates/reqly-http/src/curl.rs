//! cURL command import ("paste as cURL") and export ("Copy as cURL").
//!
//! Parsing understands bash quoting (including `$'…'`), line continuations
//! and the `^`-escaped form Chrome produces for Windows cmd.

use percent_encoding::percent_decode_str;
use reqly_core::model::{
    Auth, Body, BodyKind, HttpRequest, KeyValue, KeyValueKind, ProxyMode, ProxySettings,
};
use reqly_core::{CoreError, CoreResult};

use crate::prepare::{PreparedBody, PreparedRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    /// `;`, `&&`, `||`, `|` or a newline: ends a command.
    Separator,
}

fn tokenize_err(msg: &str) -> CoreError {
    CoreError::new("curl.syntax", msg.to_string())
}

/// Undoes Windows cmd `^` escaping (Chrome "Copy as cURL (cmd)").
fn unescape_cmd(input: &str) -> String {
    let joined = input.replace("^\r\n", "").replace("^\n", "");
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Chrome's cmd flavour starts with `curl ^"…` or continues lines with ` ^`.
fn looks_like_cmd(input: &str) -> bool {
    let start = input.trim_start();
    start.starts_with("curl ^\"")
        || start.starts_with("curl.exe ^\"")
        || input.contains(" ^\n")
        || input.contains(" ^\r\n")
}

fn ansi_c(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> CoreResult<String> {
    let mut out = String::new();
    loop {
        match chars.next() {
            None => return Err(tokenize_err("unterminated $'…' string")),
            Some('\'') => return Ok(out),
            Some('\\') => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('"') => out.push('"'),
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    let byte =
                        u8::from_str_radix(&hex, 16).map_err(|_| tokenize_err("bad \\x escape"))?;
                    out.push(byte as char);
                }
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16)
                        .map_err(|_| tokenize_err("bad \\u escape"))?;
                    out.push(char::from_u32(code).ok_or_else(|| tokenize_err("bad \\u escape"))?);
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err(tokenize_err("unterminated $'…' string")),
            },
            Some(c) => out.push(c),
        }
    }
}

fn lex(input: &str) -> CoreResult<Vec<Token>> {
    let source = if looks_like_cmd(input) {
        unescape_cmd(input)
    } else {
        input.to_string()
    };
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = source.chars().peekable();

    let flush = |word: &mut String, in_word: &mut bool, tokens: &mut Vec<Token>| {
        if *in_word {
            tokens.push(Token::Word(std::mem::take(word)));
            *in_word = false;
        }
    };

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' => flush(&mut word, &mut in_word, &mut tokens),
            '\n' | ';' => {
                flush(&mut word, &mut in_word, &mut tokens);
                tokens.push(Token::Separator);
            }
            '&' | '|' => {
                flush(&mut word, &mut in_word, &mut tokens);
                if chars.peek() == Some(&c) {
                    chars.next();
                }
                tokens.push(Token::Separator);
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some(other) => {
                    word.push(other);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => word.push(ch),
                        None => return Err(tokenize_err("unterminated single quote")),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                word.push_str(&ansi_c(&mut chars)?);
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.peek() {
                            Some(&n @ ('"' | '\\' | '$' | '`')) => {
                                chars.next();
                                word.push(n);
                            }
                            Some('\n') => {
                                chars.next();
                            }
                            _ => word.push('\\'),
                        },
                        Some(ch) => word.push(ch),
                        None => return Err(tokenize_err("unterminated double quote")),
                    }
                }
            }
            other => {
                word.push(other);
                in_word = true;
            }
        }
    }
    flush(&mut word, &mut in_word, &mut tokens);
    Ok(tokens)
}

/// Shell words of `input`, ignoring command separators.
pub fn tokenize(input: &str) -> CoreResult<Vec<String>> {
    Ok(lex(input)?
        .into_iter()
        .filter_map(|t| match t {
            Token::Word(w) => Some(w),
            Token::Separator => None,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCurl {
    pub request: HttpRequest,
    /// Options that were ignored or only partially understood.
    pub warnings: Vec<String>,
}

/// Options that take a value, by long name; short aliases map to these.
const VALUE_OPTIONS: &[&str] = &[
    "request",
    "header",
    "data",
    "data-raw",
    "data-binary",
    "data-ascii",
    "data-urlencode",
    "form",
    "form-string",
    "user",
    "cookie",
    "user-agent",
    "referer",
    "proxy",
    "proxy-user",
    "url",
    "max-time",
    "connect-timeout",
    "cert",
    "key",
    "cacert",
    "output",
    "max-redirs",
    "resolve",
    "connect-to",
    "oauth2-bearer",
    "write-out",
    "retry",
    "cookie-jar",
    "upload-file",
];

const IGNORED_FLAGS: &[&str] = &[
    "compressed",
    "silent",
    "show-error",
    "verbose",
    "include",
    "location",
    "globoff",
    "http1.1",
    "http2",
    "http2-prior-knowledge",
    "fail",
    "no-buffer",
    "progress-bar",
    "raw",
    "tr-encoding",
];

fn short_to_long(c: char) -> Option<&'static str> {
    Some(match c {
        'X' => "request",
        'H' => "header",
        'd' => "data",
        'F' => "form",
        'u' => "user",
        'b' => "cookie",
        'A' => "user-agent",
        'e' => "referer",
        'x' => "proxy",
        'U' => "proxy-user",
        'm' => "max-time",
        'o' => "output",
        'E' => "cert",
        'T' => "upload-file",
        'w' => "write-out",
        'c' => "cookie-jar",
        'k' => "insecure",
        'L' => "location",
        'G' => "get",
        'I' => "head",
        's' => "silent",
        'S' => "show-error",
        'v' => "verbose",
        'i' => "include",
        'g' => "globoff",
        'f' => "fail",
        'N' => "no-buffer",
        '#' => "progress-bar",
        _ => return None,
    })
}

fn decode(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

fn split_pairs(s: &str) -> Option<Vec<(String, String)>> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').map(|(k, v)| (decode(k), decode(v))))
        .collect()
}

/// Parses the first `curl` command in `input`.
pub fn parse_curl(input: &str) -> CoreResult<ParsedCurl> {
    let tokens = lex(input)?;
    let words: Vec<String> = tokens
        .into_iter()
        .take_while(|t| *t != Token::Separator)
        .filter_map(|t| match t {
            Token::Word(w) => Some(w),
            Token::Separator => None,
        })
        .collect();
    parse_words(&words)
}

/// Parses every `curl` command in `input` (newline, `;` or `&&` separated).
pub fn parse_curl_many(input: &str) -> CoreResult<Vec<ParsedCurl>> {
    let mut commands: Vec<Vec<String>> = vec![Vec::new()];
    for token in lex(input)? {
        match token {
            Token::Word(w) => commands.last_mut().expect("non-empty").push(w),
            Token::Separator => commands.push(Vec::new()),
        }
    }
    commands
        .into_iter()
        .filter(|c| !c.is_empty())
        .map(|c| parse_words(&c))
        .collect()
}

fn parse_words(words: &[String]) -> CoreResult<ParsedCurl> {
    // Skip leading words before "curl" (e.g. `$ curl …`).
    let start = words
        .iter()
        .position(|w| w == "curl" || w.ends_with("/curl") || w == "curl.exe")
        .ok_or_else(|| CoreError::new("curl.not_curl", "not a curl command"))?;
    let args = &words[start + 1..];

    let mut warnings = Vec::new();
    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut data_is_binary = false;
    let mut form: Vec<KeyValue> = Vec::new();
    let mut get = false;
    let mut head = false;
    let mut request = HttpRequest::default();
    let mut proxy: Option<ProxySettings> = None;

    // Expand `-sSL`, `-XPOST`, `--opt=value` into (option, value) pairs.
    let mut i = 0;
    let mut options: Vec<(String, Option<String>)> = Vec::new();
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                continue;
            }
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) if VALUE_OPTIONS.contains(&n) => (n.to_string(), Some(v.to_string())),
                _ => (long.to_string(), None),
            };
            if VALUE_OPTIONS.contains(&name.as_str()) {
                let value = inline.or_else(|| {
                    let v = args.get(i).cloned();
                    i += 1;
                    v
                });
                options.push((name, value));
            } else {
                options.push((name, None));
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            let chars: Vec<char> = arg[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                let c = chars[j];
                j += 1;
                let Some(name) = short_to_long(c) else {
                    warnings.push(format!("-{c}"));
                    continue;
                };
                if VALUE_OPTIONS.contains(&name) {
                    let rest: String = chars[j..].iter().collect();
                    let value = if rest.is_empty() {
                        let v = args.get(i).cloned();
                        i += 1;
                        v
                    } else {
                        Some(rest)
                    };
                    options.push((name.to_string(), value));
                    break;
                }
                options.push((name.to_string(), None));
            }
        } else if url.is_none() {
            url = Some(arg.clone());
        } else {
            warnings.push(format!("extra argument {arg}"));
        }
    }

    for (name, value) in options {
        let value = value.unwrap_or_default();
        match name.as_str() {
            "request" => method = Some(value.to_uppercase()),
            "header" => {
                if let Some((k, v)) = value.split_once(':') {
                    headers.push(KeyValue::new(k.trim(), v.trim()));
                } else if let Some(k) = value.strip_suffix(';') {
                    headers.push(KeyValue::new(k.trim(), ""));
                }
            }
            "data" | "data-ascii" | "data-raw" => data.push(value),
            "data-binary" => {
                data_is_binary = true;
                data.push(value);
            }
            "data-urlencode" => {
                let encoded = match value.split_once('=') {
                    Some((k, v)) => format!(
                        "{k}={}",
                        percent_encoding::utf8_percent_encode(
                            v,
                            percent_encoding::NON_ALPHANUMERIC
                        )
                    ),
                    None => percent_encoding::utf8_percent_encode(
                        &value,
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string(),
                };
                data.push(encoded);
            }
            "form" | "form-string" => {
                let (k, v) = value.split_once('=').unwrap_or((&value, ""));
                let mut row = KeyValue::new(k, v);
                if name == "form"
                    && let Some(path) = v.strip_prefix('@')
                {
                    // `@file;type=…;filename=…` — keep the path only.
                    row.value = path.split(';').next().unwrap_or(path).to_string();
                    row.kind = KeyValueKind::File;
                }
                form.push(row);
            }
            "user" => {
                let (u, p) = value.split_once(':').unwrap_or((&value, ""));
                request.auth = Auth::Basic {
                    username: u.into(),
                    password: p.into(),
                };
            }
            "oauth2-bearer" => {
                request.auth = Auth::Bearer {
                    token: value,
                    prefix: String::new(),
                }
            }
            "cookie" => headers.push(KeyValue::new("Cookie", value)),
            "user-agent" => headers.push(KeyValue::new("User-Agent", value)),
            "referer" => headers.push(KeyValue::new("Referer", value)),
            "url" => url = Some(value),
            "insecure" => request.settings.verify_tls = Some(false),
            "get" => get = true,
            "head" => head = true,
            "proxy" => {
                proxy
                    .get_or_insert_with(|| ProxySettings {
                        mode: ProxyMode::Manual,
                        ..Default::default()
                    })
                    .url = value;
            }
            "proxy-user" => {
                let (u, p) = value.split_once(':').unwrap_or((&value, ""));
                let entry = proxy.get_or_insert_with(|| ProxySettings {
                    mode: ProxyMode::Manual,
                    ..Default::default()
                });
                entry.username = u.into();
                entry.password = p.into();
            }
            "max-time" => {
                request.settings.timeout_ms = value.parse::<f64>().ok().map(|s| (s * 1000.0) as u64)
            }
            "connect-timeout" => {
                request.settings.connect_timeout_ms =
                    value.parse::<f64>().ok().map(|s| (s * 1000.0) as u64);
            }
            "max-redirs" => request.settings.max_redirects = value.parse().ok(),
            n if IGNORED_FLAGS.contains(&n) => {}
            other => warnings.push(format!("--{other}")),
        }
    }

    let raw_url = url.ok_or_else(|| CoreError::new("curl.no_url", "curl command has no URL"))?;
    request.settings.proxy = proxy;

    // Query string → params (decoded), so the URL bar and table stay in sync.
    let (base, query) = match raw_url.split_once('?') {
        Some((b, q)) => (b.to_string(), Some(q.to_string())),
        None => (raw_url.clone(), None),
    };
    request.url = base;
    if let Some(q) = query {
        let q = q.split('#').next().unwrap_or_default();
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            request.params.push(KeyValue::new(decode(k), decode(v)));
        }
    }

    let content_type = headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase());

    if get && !data.is_empty() {
        for (k, v) in split_pairs(&data.join("&")).unwrap_or_default() {
            request.params.push(KeyValue::new(k, v));
        }
        data.clear();
    }

    if !form.is_empty() {
        request.body = Body {
            kind: BodyKind::Multipart,
            fields: form,
            ..Default::default()
        };
    } else if !data.is_empty() {
        let text = data.join("&");
        let is_form_ct = content_type
            .as_deref()
            .is_none_or(|ct| ct.starts_with("application/x-www-form-urlencoded"));
        request.body = match content_type.as_deref() {
            Some(ct) if ct.contains("json") => Body {
                kind: BodyKind::Json,
                text,
                ..Default::default()
            },
            Some(ct) if ct.contains("xml") => Body {
                kind: BodyKind::Xml,
                text,
                ..Default::default()
            },
            Some(ct) if ct.contains("yaml") => Body {
                kind: BodyKind::Yaml,
                text,
                ..Default::default()
            },
            Some(ct) if ct.starts_with("text/html") => Body {
                kind: BodyKind::Html,
                text,
                ..Default::default()
            },
            _ if is_form_ct && !data_is_binary => match split_pairs(&text) {
                Some(pairs) => Body {
                    kind: BodyKind::FormUrlencoded,
                    fields: pairs
                        .into_iter()
                        .map(|(k, v)| KeyValue::new(k, v))
                        .collect(),
                    ..Default::default()
                },
                None => Body {
                    kind: BodyKind::Text,
                    text,
                    ..Default::default()
                },
            },
            _ => Body {
                kind: BodyKind::Text,
                text,
                ..Default::default()
            },
        };
        // The body kind sets this header again when sending.
        if matches!(request.body.kind, BodyKind::FormUrlencoded) {
            headers.retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
        }
    }

    request.method = match (method, head) {
        (Some(m), _) => m,
        (None, true) => "HEAD".into(),
        (None, false) if request.body.kind != BodyKind::None => "POST".into(),
        _ => "GET".into(),
    };
    request.headers = headers;
    Ok(ParsedCurl { request, warnings })
}

/// Single-quotes for bash: `it's` → `'it'\''s'`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Renders a prepared request as a multi-line bash cURL command.
pub fn to_curl(r: &PreparedRequest) -> String {
    let mut parts = vec![
        format!("curl --request {}", r.method),
        format!("--url {}", quote(&r.url)),
    ];
    for (k, v) in &r.headers {
        parts.push(format!("--header {}", quote(&format!("{k}: {v}"))));
    }
    match &r.body {
        PreparedBody::None => {}
        PreparedBody::Bytes(b) => {
            parts.push(format!("--data-raw {}", quote(&String::from_utf8_lossy(b))))
        }
        PreparedBody::File(p) => parts.push(format!(
            "--data-binary {}",
            quote(&format!("@{}", p.display()))
        )),
        PreparedBody::Blob(h) => {
            parts.push(format!("--data-binary {}", quote(&format!("@blob-{h}"))))
        }
        PreparedBody::Multipart(fields) => {
            for f in fields {
                let value = match (&f.file, &f.blob) {
                    (Some(p), _) => format!("@{}", p.display()),
                    (None, Some(h)) => format!("@blob-{h}"),
                    _ => f.value.clone(),
                };
                parts.push(format!("--form {}", quote(&format!("{}={value}", f.name))));
            }
        }
    }
    if r.settings.verify_tls == Some(false) {
        parts.push("--insecure".into());
    }
    parts.join(" \\\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn quoted_words_tokenize_back(words in proptest::collection::vec("\\PC{0,12}", 1..5)) {
            let line = words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ");
            prop_assert_eq!(tokenize(&line).unwrap(), words);
        }
    }
}
