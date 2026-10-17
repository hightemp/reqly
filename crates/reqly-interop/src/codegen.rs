//! Code generation: a prepared request → a snippet in many languages.

use reqly_http::prepare::{PreparedBody, PreparedRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request model for generators: everything resolved and flattened.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: CodeBody,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodeBody {
    None,
    Text(String),
    /// `application/x-www-form-urlencoded` pairs.
    Form(Vec<(String, String)>),
    Multipart(Vec<Part>),
    /// A file sent as the raw body.
    File(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    pub value: Option<String>,
    pub file: Option<String>,
}

fn form_decode(s: &str) -> String {
    percent_decode(&s.replace('+', " "))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl CodeRequest {
    pub fn from_prepared(p: &PreparedRequest) -> Self {
        let content_type = p
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.to_ascii_lowercase())
            .unwrap_or_default();
        let body = match &p.body {
            PreparedBody::None => CodeBody::None,
            PreparedBody::Bytes(bytes) => {
                let text = String::from_utf8_lossy(bytes).into_owned();
                if content_type.starts_with("application/x-www-form-urlencoded") {
                    CodeBody::Form(
                        text.split('&')
                            .filter(|p| !p.is_empty())
                            .map(|p| {
                                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                                (form_decode(k), form_decode(v))
                            })
                            .collect(),
                    )
                } else {
                    CodeBody::Text(text)
                }
            }
            PreparedBody::File(path) => CodeBody::File(path.display().to_string()),
            PreparedBody::Blob(_) => CodeBody::File("uploaded-file".into()),
            PreparedBody::Multipart(parts) => CodeBody::Multipart(
                parts
                    .iter()
                    .map(|part| Part {
                        name: part.name.clone(),
                        value: (part.file.is_none() && part.blob.is_none())
                            .then(|| part.value.clone()),
                        file: part
                            .file
                            .as_ref()
                            .map(|f| f.display().to_string())
                            .or_else(|| part.blob.as_ref().map(|_| part.value.clone())),
                    })
                    .collect(),
            ),
        };
        // Multipart boundaries are set by every HTTP library itself.
        let headers = p
            .headers
            .iter()
            .filter(|(k, v)| {
                !(k.eq_ignore_ascii_case("content-type")
                    && v.to_ascii_lowercase().starts_with("multipart/"))
            })
            .cloned()
            .collect();
        Self {
            method: p.method.clone(),
            url: p.url.clone(),
            headers,
            body,
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn body_text(&self) -> Option<String> {
        match &self.body {
            CodeBody::Text(t) => Some(t.clone()),
            CodeBody::Form(pairs) => Some(
                pairs
                    .iter()
                    .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
                    .collect::<Vec<_>>()
                    .join("&"),
            ),
            _ => None,
        }
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ----- targets ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Target {
    CurlBash,
    CurlCmd,
    CurlPowershell,
    Wget,
    Httpie,
    JsFetch,
    JsAxios,
    JsXhr,
    NodeHttp,
    NodeUndici,
    PythonRequests,
    PythonHttpx,
    PythonHttpClient,
    GoNetHttp,
    RustReqwest,
    JavaOkHttp,
    JavaHttpClient,
    KotlinOkHttp,
    CsharpHttpClient,
    CsharpRestSharp,
    PhpCurl,
    PhpGuzzle,
    RubyNetHttp,
    SwiftUrlSession,
    DartHttp,
    PowershellRestMethod,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TargetInfo {
    pub id: Target,
    /// Language group, e.g. `Shell`, `JavaScript`.
    pub language: String,
    pub label: String,
    /// CodeMirror highlighting mode: `shell`, `javascript`, `python`, `text`…
    pub syntax: String,
}

pub const TARGETS: &[(Target, &str, &str, &str)] = &[
    (Target::CurlBash, "Shell", "cURL (bash)", "shell"),
    (Target::CurlCmd, "Shell", "cURL (Windows cmd)", "shell"),
    (
        Target::CurlPowershell,
        "Shell",
        "cURL (PowerShell)",
        "shell",
    ),
    (Target::Wget, "Shell", "Wget", "shell"),
    (Target::Httpie, "Shell", "HTTPie", "shell"),
    (
        Target::PowershellRestMethod,
        "Shell",
        "PowerShell Invoke-RestMethod",
        "shell",
    ),
    (Target::JsFetch, "JavaScript", "fetch", "javascript"),
    (Target::JsAxios, "JavaScript", "Axios", "javascript"),
    (Target::JsXhr, "JavaScript", "XMLHttpRequest", "javascript"),
    (Target::NodeHttp, "JavaScript", "Node.js http", "javascript"),
    (
        Target::NodeUndici,
        "JavaScript",
        "Node.js undici",
        "javascript",
    ),
    (Target::PythonRequests, "Python", "Requests", "python"),
    (Target::PythonHttpx, "Python", "HTTPX", "python"),
    (Target::PythonHttpClient, "Python", "http.client", "python"),
    (Target::GoNetHttp, "Go", "net/http", "go"),
    (Target::RustReqwest, "Rust", "reqwest", "rust"),
    (Target::JavaOkHttp, "Java", "OkHttp", "java"),
    (Target::JavaHttpClient, "Java", "HttpClient", "java"),
    (Target::KotlinOkHttp, "Kotlin", "OkHttp", "kotlin"),
    (Target::CsharpHttpClient, "C#", "HttpClient", "csharp"),
    (Target::CsharpRestSharp, "C#", "RestSharp", "csharp"),
    (Target::PhpCurl, "PHP", "cURL", "php"),
    (Target::PhpGuzzle, "PHP", "Guzzle", "php"),
    (Target::RubyNetHttp, "Ruby", "Net::HTTP", "ruby"),
    (Target::SwiftUrlSession, "Swift", "URLSession", "swift"),
    (Target::DartHttp, "Dart", "http", "dart"),
];

pub fn targets() -> Vec<TargetInfo> {
    TARGETS
        .iter()
        .map(|(id, language, label, syntax)| TargetInfo {
            id: *id,
            language: (*language).into(),
            label: (*label).into(),
            syntax: (*syntax).into(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Spaces per indentation level.
    pub indent: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { indent: 2 }
    }
}

pub fn generate(target: Target, r: &CodeRequest, o: Options) -> String {
    let i = " ".repeat(o.indent.clamp(1, 8));
    let mut out = match target {
        Target::CurlBash => curl(r, Shell::Bash),
        Target::CurlCmd => curl(r, Shell::Cmd),
        Target::CurlPowershell => curl(r, Shell::Pwsh),
        Target::Wget => wget(r),
        Target::Httpie => httpie(r),
        Target::PowershellRestMethod => powershell(r),
        Target::JsFetch => js_fetch(r, &i),
        Target::JsAxios => js_axios(r, &i),
        Target::JsXhr => js_xhr(r),
        Target::NodeHttp => node_http(r, &i),
        Target::NodeUndici => node_undici(r, &i),
        Target::PythonRequests => py_requests(r, &i, "requests"),
        Target::PythonHttpx => py_requests(r, &i, "httpx"),
        Target::PythonHttpClient => py_http_client(r, &i),
        Target::GoNetHttp => go(r, &i),
        Target::RustReqwest => rust(r, &i),
        Target::JavaOkHttp => okhttp(r, &i, false),
        Target::KotlinOkHttp => okhttp(r, &i, true),
        Target::JavaHttpClient => java_http_client(r, &i),
        Target::CsharpHttpClient => csharp_http_client(r, &i),
        Target::CsharpRestSharp => csharp_restsharp(r),
        Target::PhpCurl => php_curl(r, &i),
        Target::PhpGuzzle => php_guzzle(r, &i),
        Target::RubyNetHttp => ruby(r),
        Target::SwiftUrlSession => swift(r, &i),
        Target::DartHttp => dart(r, &i),
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Boundary used by snippets that build multipart bodies by hand.
const BOUNDARY: &str = "----ReqlyFormBoundary7MA4YWxk";

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

// ----- string literals -----------------------------------------------------------------------

/// Double-quoted literal with C-style escapes (JS, Python, Go, Java, C#, Rust, Swift…).
fn dq(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Double-quoted literal for languages that interpolate `$` (Kotlin, Dart).
fn dq_dollar(s: &str) -> String {
    dq(s).replace('$', "\\$")
}

/// Single-quoted literal with `\'` escapes (JS, Python, PHP, Ruby, Dart raw…).
fn sq(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// PHP and Ruby single quotes only escape `\` and `'`.
fn sq_literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[derive(Clone, Copy, PartialEq)]
enum Shell {
    Bash,
    Cmd,
    Pwsh,
}

fn shell_quote(s: &str, shell: Shell) -> String {
    match shell {
        Shell::Bash => {
            if !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
            {
                s.to_string()
            } else {
                format!("'{}'", s.replace('\'', "'\\''"))
            }
        }
        // cmd: double quotes, inner quotes doubled-escaped with backslash, % doubled.
        Shell::Cmd => format!("\"{}\"", s.replace('"', "\\\"").replace('%', "%%")),
        Shell::Pwsh => format!("'{}'", s.replace('\'', "''")),
    }
}

fn is_json(r: &CodeRequest) -> bool {
    r.header("content-type")
        .is_some_and(|c| c.to_ascii_lowercase().contains("json"))
}

// ----- shell ----------------------------------------------------------------------------------

fn curl(r: &CodeRequest, shell: Shell) -> String {
    let cont = match shell {
        Shell::Bash => " \\\n",
        Shell::Cmd => " ^\n",
        Shell::Pwsh => " `\n",
    };
    let q = |s: &str| shell_quote(s, shell);
    let bin = if shell == Shell::Pwsh {
        "curl.exe"
    } else {
        "curl"
    };
    let mut parts = vec![
        format!("{bin} --request {}", r.method),
        format!("  --url {}", q(&r.url)),
    ];
    for (k, v) in &r.headers {
        parts.push(format!("  --header {}", q(&format!("{k}: {v}"))));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::Text(t) => parts.push(format!("  --data {}", q(t))),
        CodeBody::Form(pairs) => {
            for (k, v) in pairs {
                parts.push(format!("  --data-urlencode {}", q(&format!("{k}={v}"))));
            }
        }
        CodeBody::Multipart(ps) => {
            for p in ps {
                let value = match (&p.file, &p.value) {
                    (Some(f), _) => format!("{}=@{f}", p.name),
                    (None, Some(v)) => format!("{}={v}", p.name),
                    _ => format!("{}=", p.name),
                };
                parts.push(format!("  --form {}", q(&value)));
            }
        }
        CodeBody::File(f) => parts.push(format!("  --data-binary {}", q(&format!("@{f}")))),
    }
    parts.join(cont)
}

fn wget(r: &CodeRequest) -> String {
    let q = |s: &str| shell_quote(s, Shell::Bash);
    let mut prefix = String::new();
    let mut parts = vec![
        "wget --quiet".to_string(),
        format!("  --method {}", r.method),
    ];
    for (k, v) in &r.headers {
        parts.push(format!("  --header {}", q(&format!("{k}: {v}"))));
    }
    match &r.body {
        CodeBody::File(f) => parts.push(format!("  --body-file {}", q(f))),
        CodeBody::Multipart(_) => {
            prefix.push_str("# wget cannot send multipart bodies; use curl --form instead\n")
        }
        _ => {
            if let Some(t) = r.body_text() {
                parts.push(format!("  --body-data {}", q(&t)));
            }
        }
    }
    parts.push("  --output-document -".into());
    parts.push(format!("  {}", q(&r.url)));
    prefix + &parts.join(" \\\n")
}

fn httpie(r: &CodeRequest) -> String {
    let q = |s: &str| shell_quote(s, Shell::Bash);
    let mut parts = Vec::new();
    let mut head = String::new();
    match &r.body {
        CodeBody::Text(t) => head.push_str(&format!("printf %s {} | ", q(t))),
        CodeBody::File(f) => head.push_str(&format!("http {} {} < {}", r.method, q(&r.url), q(f))),
        _ => {}
    }
    if let CodeBody::File(_) = &r.body {
        let mut s = head;
        for (k, v) in &r.headers {
            s.push_str(&format!(" \\\n  {}", q(&format!("{k}:{v}"))));
        }
        return s;
    }
    let flag = match &r.body {
        CodeBody::Form(_) => " --form",
        CodeBody::Multipart(_) => " --multipart",
        _ => "",
    };
    parts.push(format!("{head}http{flag} {} {}", r.method, q(&r.url)));
    for (k, v) in &r.headers {
        parts.push(format!("  {}", q(&format!("{k}:{v}"))));
    }
    match &r.body {
        CodeBody::Form(pairs) => {
            for (k, v) in pairs {
                parts.push(format!("  {}", q(&format!("{k}={v}"))));
            }
        }
        CodeBody::Multipart(ps) => {
            for p in ps {
                parts.push(format!(
                    "  {}",
                    q(&match (&p.file, &p.value) {
                        (Some(f), _) => format!("{}@{f}", p.name),
                        (_, v) => format!("{}={}", p.name, v.clone().unwrap_or_default()),
                    })
                ));
            }
        }
        _ => {}
    }
    parts.join(" \\\n")
}

fn powershell(r: &CodeRequest) -> String {
    let q = |s: &str| shell_quote(s, Shell::Pwsh);
    let mut out = String::from("$headers = @{}\n");
    let mut content_type = None;
    for (k, v) in &r.headers {
        if k.eq_ignore_ascii_case("content-type") {
            content_type = Some(v.clone());
        } else {
            out.push_str(&format!("$headers.Add({}, {})\n", q(k), q(v)));
        }
    }
    let mut args = format!(
        "$response = Invoke-RestMethod -Uri {} -Method {} -Headers $headers",
        q(&r.url),
        r.method
    );
    if let Some(ct) = &content_type {
        args.push_str(&format!(" -ContentType {}", q(ct)));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => args.push_str(&format!(" -InFile {}", q(f))),
        CodeBody::Multipart(ps) => {
            out.push_str("$form = @{\n");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => {
                        out.push_str(&format!("  {} = Get-Item -Path {}\n", q(&p.name), q(f)))
                    }
                    (_, v) => out.push_str(&format!(
                        "  {} = {}\n",
                        q(&p.name),
                        q(&v.clone().unwrap_or_default())
                    )),
                }
            }
            out.push_str("}\n");
            args.push_str(" -Form $form");
        }
        _ => {
            if let Some(t) = r.body_text() {
                out.push_str(&format!("$body = {}\n", q(&t)));
                args.push_str(" -Body $body");
            }
        }
    }
    out.push_str(&args);
    out
}

// ----- JavaScript -------------------------------------------------------------------------

fn js_headers(r: &CodeRequest, i: &str, depth: usize, skip_content_type: bool) -> String {
    let pad = i.repeat(depth);
    let rows: Vec<String> = r
        .headers
        .iter()
        .filter(|(k, _)| !(skip_content_type && k.eq_ignore_ascii_case("content-type")))
        .map(|(k, v)| format!("{pad}{i}{}: {}", sq(k), sq(v)))
        .collect();
    if rows.is_empty() {
        "{}".into()
    } else {
        format!("{{\n{}\n{pad}}}", rows.join(",\n"))
    }
}

/// JS statements building the body; returns (setup code, expression).
fn js_body(r: &CodeRequest, i: &str, node: bool) -> (String, Option<String>) {
    match &r.body {
        CodeBody::None => (String::new(), None),
        CodeBody::Text(t) if is_json(r) && serde_json::from_str::<serde_json::Value>(t).is_ok() => {
            let pretty = serde_json::to_string_pretty(
                &serde_json::from_str::<serde_json::Value>(t).unwrap_or_default(),
            )
            .unwrap_or_default()
            .replace("  ", i);
            (String::new(), Some(format!("JSON.stringify({pretty})")))
        }
        CodeBody::Text(t) => (String::new(), Some(sq(t))),
        CodeBody::Form(pairs) => {
            let mut s = String::from("const body = new URLSearchParams();\n");
            for (k, v) in pairs {
                s.push_str(&format!("body.append({}, {});\n", sq(k), sq(v)));
            }
            (s, Some("body".into()))
        }
        CodeBody::Multipart(ps) => {
            let mut s = String::from("const body = new FormData();\n");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) if node => s.push_str(&format!(
                        "body.append({}, new Blob([await fs.readFile({})]), {});\n",
                        sq(&p.name),
                        sq(f),
                        sq(f.rsplit('/').next().unwrap_or(f))
                    )),
                    (Some(f), _) => s.push_str(&format!(
                        "body.append({}, fileInput.files[0]); // {}\n",
                        sq(&p.name),
                        f
                    )),
                    (_, v) => s.push_str(&format!(
                        "body.append({}, {});\n",
                        sq(&p.name),
                        sq(&v.clone().unwrap_or_default())
                    )),
                }
            }
            (s, Some("body".into()))
        }
        CodeBody::File(f) if node => (String::new(), Some(format!("await fs.readFile({})", sq(f)))),
        CodeBody::File(f) => (String::new(), Some(format!("fileInput.files[0] /* {f} */"))),
    }
}

fn needs_fs(r: &CodeRequest) -> bool {
    match &r.body {
        CodeBody::File(_) => true,
        CodeBody::Multipart(ps) => ps.iter().any(|p| p.file.is_some()),
        _ => false,
    }
}

fn js_fetch(r: &CodeRequest, i: &str) -> String {
    let multipart = matches!(r.body, CodeBody::Multipart(_));
    let (setup, body) = js_body(r, i, false);
    let mut out = setup;
    out.push_str(&format!(
        "const response = await fetch({}, {{\n",
        sq(&r.url)
    ));
    out.push_str(&format!("{i}method: {},\n", sq(&r.method)));
    out.push_str(&format!("{i}headers: {}", js_headers(r, i, 1, multipart)));
    if let Some(b) = body {
        out.push_str(&format!(",\n{i}body: {b}"));
    }
    out.push_str("\n});\n\nconsole.log(await response.text());");
    out
}

fn js_axios(r: &CodeRequest, i: &str) -> String {
    let (setup, body) = js_body(r, i, false);
    let body = body.map(|b| {
        b.strip_prefix("JSON.stringify(")
            .and_then(|b| b.strip_suffix(')'))
            .map(str::to_string)
            .unwrap_or(b)
    });
    let mut out = String::from("import axios from 'axios';\n\n");
    out.push_str(&setup);
    out.push_str("const { data } = await axios.request({\n");
    out.push_str(&format!("{i}method: {},\n", sq(&r.method)));
    out.push_str(&format!("{i}url: {},\n", sq(&r.url)));
    out.push_str(&format!(
        "{i}headers: {}",
        js_headers(r, i, 1, matches!(r.body, CodeBody::Multipart(_)))
    ));
    if let Some(b) = body {
        out.push_str(&format!(",\n{i}data: {b}"));
    }
    out.push_str("\n});\n\nconsole.log(data);");
    out
}

fn js_xhr(r: &CodeRequest) -> String {
    let (setup, body) = js_body(r, "  ", false);
    let mut out = setup;
    out.push_str("const xhr = new XMLHttpRequest();\n");
    out.push_str("xhr.addEventListener('load', () => console.log(xhr.responseText));\n");
    out.push_str(&format!("xhr.open({}, {});\n", sq(&r.method), sq(&r.url)));
    for (k, v) in &r.headers {
        out.push_str(&format!("xhr.setRequestHeader({}, {});\n", sq(k), sq(v)));
    }
    out.push_str(&format!(
        "xhr.send({});",
        body.unwrap_or_else(|| "null".into())
    ));
    out
}

fn node_http(r: &CodeRequest, i: &str) -> String {
    let https = r.url.starts_with("https:");
    let module = if https { "https" } else { "http" };
    let mut out = format!("import {module} from 'node:{module}';\n");
    if needs_fs(r) {
        out.push_str("import fs from 'node:fs/promises';\n");
    }
    out.push('\n');
    let (setup, body) = match &r.body {
        CodeBody::Multipart(ps) => {
            let mut s = format!("const boundary = {};\nconst parts = [];\n", sq(BOUNDARY));
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => {
                        s.push_str(&format!(
                            "parts.push(Buffer.from(`--${{boundary}}\\r\\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\\r\\n\\r\\n`));\n",
                            p.name.replace('"', "\\\"").replace('`', "\\`"),
                            file_name(f).replace('"', "\\\"").replace('`', "\\`")
                        ));
                        s.push_str(&format!("parts.push(await fs.readFile({}), Buffer.from('\\r\\n'));\n", sq(f)));
                    }
                    (_, v) => s.push_str(&format!(
                        "parts.push(Buffer.from(`--${{boundary}}\\r\\nContent-Disposition: form-data; name=\"{}\"\\r\\n\\r\\n`), Buffer.from({}), Buffer.from('\\r\\n'));\n",
                        p.name.replace('"', "\\\"").replace('`', "\\`"),
                        sq(&v.clone().unwrap_or_default())
                    )),
                }
            }
            s.push_str("parts.push(Buffer.from(`--${boundary}--\\r\\n`));\nconst body = Buffer.concat(parts);\n");
            (s, Some("body".to_string()))
        }
        _ => js_body(r, i, true),
    };
    let multipart = matches!(r.body, CodeBody::Multipart(_));
    out.push_str(&setup);
    out.push_str(&format!(
        "const req = {module}.request({}, {{\n",
        sq(&r.url)
    ));
    out.push_str(&format!("{i}method: {},\n", sq(&r.method)));
    let mut headers = js_headers(r, i, 1, multipart);
    if multipart {
        let line = format!("{i}{i}'Content-Type': `multipart/form-data; boundary=${{boundary}}`");
        headers = if headers == "{}" {
            format!("{{\n{line}\n{i}}}")
        } else {
            headers.replacen("{\n", &format!("{{\n{line},\n"), 1)
        };
    }
    out.push_str(&format!("{i}headers: {headers}\n"));
    out.push_str("}, (res) => {\n");
    out.push_str(&format!("{i}let data = '';\n{i}res.on('data', (chunk) => (data += chunk));\n{i}res.on('end', () => console.log(data));\n"));
    out.push_str("});\n");
    out.push_str("req.on('error', console.error);\n");
    if let Some(b) = body {
        let b = if b == "body" && !multipart {
            "body.toString()".to_string()
        } else {
            b
        };
        out.push_str(&format!("req.write({b});\n"));
    }
    out.push_str("req.end();");
    out
}

fn node_undici(r: &CodeRequest, i: &str) -> String {
    let mut out = String::from("import { request } from 'undici';\n");
    if needs_fs(r) {
        out.push_str("import fs from 'node:fs/promises';\n");
    }
    out.push('\n');
    let (setup, body) = js_body(r, i, true);
    out.push_str(&setup);
    out.push_str(&format!(
        "const {{ statusCode, body: res }} = await request({}, {{\n",
        sq(&r.url)
    ));
    out.push_str(&format!("{i}method: {},\n", sq(&r.method)));
    out.push_str(&format!(
        "{i}headers: {}",
        js_headers(r, i, 1, matches!(r.body, CodeBody::Multipart(_)))
    ));
    if let Some(b) = body {
        let b = if matches!(r.body, CodeBody::Form(_)) {
            "body.toString()".into()
        } else {
            b
        };
        out.push_str(&format!(",\n{i}body: {b}"));
    }
    out.push_str("\n});\n\nconsole.log(statusCode, await res.text());");
    out
}

// ----- Python ---------------------------------------------------------------------------------

fn py_dict(pairs: &[(String, String)], i: &str) -> String {
    if pairs.is_empty() {
        return "{}".into();
    }
    format!(
        "{{\n{}\n}}",
        pairs
            .iter()
            .map(|(k, v)| format!("{i}{}: {}", dq(k), dq(v)))
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

fn py_requests(r: &CodeRequest, i: &str, lib: &str) -> String {
    let mut out = format!("import {lib}\n\n");
    out.push_str(&format!("url = {}\n", dq(&r.url)));
    let headers: Vec<(String, String)> = r
        .headers
        .iter()
        .filter(|(k, _)| {
            !(matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type"))
        })
        .cloned()
        .collect();
    out.push_str(&format!("headers = {}\n", py_dict(&headers, i)));
    let mut args = vec![dq(&r.method), "url".into(), "headers=headers".into()];
    match &r.body {
        CodeBody::None => {}
        CodeBody::Text(t) if is_json(r) && serde_json::from_str::<serde_json::Value>(t).is_ok() => {
            out.push_str(&format!("payload = {}\n", dq(t)));
            args.push(if lib == "httpx" {
                "content=payload".into()
            } else {
                "data=payload".into()
            });
        }
        CodeBody::Text(t) => {
            out.push_str(&format!("payload = {}\n", dq(t)));
            args.push(if lib == "httpx" {
                "content=payload".into()
            } else {
                "data=payload".into()
            });
        }
        CodeBody::Form(pairs) => {
            out.push_str(&format!("payload = {}\n", py_dict(pairs, i)));
            args.push("data=payload".into());
        }
        CodeBody::Multipart(ps) => {
            let fields: Vec<(String, String)> = ps
                .iter()
                .filter_map(|p| p.value.clone().map(|v| (p.name.clone(), v)))
                .collect();
            out.push_str(&format!("payload = {}\n", py_dict(&fields, i)));
            let files: Vec<String> = ps
                .iter()
                .filter_map(|p| {
                    p.file
                        .as_ref()
                        .map(|f| format!("{i}{}: open({}, \"rb\")", dq(&p.name), dq(f)))
                })
                .collect();
            out.push_str(&format!(
                "files = {}\n",
                if files.is_empty() {
                    "{}".into()
                } else {
                    format!("{{\n{}\n}}", files.join(",\n"))
                }
            ));
            args.push("data=payload".into());
            args.push("files=files".into());
        }
        CodeBody::File(f) => {
            out.push_str(&format!("payload = open({}, \"rb\")\n", dq(f)));
            args.push(if lib == "httpx" {
                "content=payload".into()
            } else {
                "data=payload".into()
            });
        }
    }
    out.push_str(&format!(
        "\nresponse = {lib}.request({})\n\nprint(response.text)",
        args.join(", ")
    ));
    out
}

fn py_http_client(r: &CodeRequest, i: &str) -> String {
    let (scheme, rest) = r.url.split_once("://").unwrap_or(("http", &r.url));
    let (host, path) = match rest.find('/') {
        Some(p) => (&rest[..p], &rest[p..]),
        None => (rest, "/"),
    };
    let class = if scheme == "https" {
        "HTTPSConnection"
    } else {
        "HTTPConnection"
    };
    let mut out = String::from("import http.client\n\n");
    out.push_str(&format!("conn = http.client.{class}({})\n", dq(host)));
    out.push_str(&format!("headers = {}\n", py_dict(&r.headers, i)));
    let body = match &r.body {
        CodeBody::File(f) => {
            out.push_str(&format!("payload = open({}, \"rb\").read()\n", dq(f)));
            Some("payload")
        }
        CodeBody::Multipart(ps) => {
            out.push_str(&format!("boundary = {}\n", dq(BOUNDARY)));
            out.push_str("parts = []\n");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => {
                        out.push_str(&format!(
                            "parts.append(f\"--{{boundary}}\\r\\nContent-Disposition: form-data; name={}; filename={}\\r\\n\\r\\n\".encode())\n",
                            dq(&p.name).replace('"', "\\\""),
                            dq(file_name(f)).replace('"', "\\\"")
                        ));
                        out.push_str(&format!("parts.append(open({}, \"rb\").read() + b\"\\r\\n\")\n", dq(f)));
                    }
                    (_, v) => out.push_str(&format!(
                        "parts.append(f\"--{{boundary}}\\r\\nContent-Disposition: form-data; name={}\\r\\n\\r\\n\".encode() + {}.encode() + b\"\\r\\n\")\n",
                        dq(&p.name).replace('"', "\\\""),
                        dq(&v.clone().unwrap_or_default())
                    )),
                }
            }
            out.push_str(
                "parts.append(f\"--{boundary}--\\r\\n\".encode())\npayload = b\"\".join(parts)\n",
            );
            out.push_str(
                "headers[\"Content-Type\"] = f\"multipart/form-data; boundary={boundary}\"\n",
            );
            Some("payload")
        }
        _ => match r.body_text() {
            Some(t) => {
                out.push_str(&format!("payload = {}\n", dq(&t)));
                Some("payload")
            }
            None => None,
        },
    };
    out.push_str(&format!(
        "conn.request({}, {}, {}headers=headers)\n",
        dq(&r.method),
        dq(path),
        body.map(|b| format!("body={b}, ")).unwrap_or_default()
    ));
    out.push_str("res = conn.getresponse()\nprint(res.read().decode())");
    out
}

// ----- Go, Rust ---------------------------------------------------------------------------------

fn go(r: &CodeRequest, i: &str) -> String {
    let mut imports = vec!["\"fmt\"", "\"io\"", "\"net/http\""];
    let mut setup = String::new();
    let body_expr = match &r.body {
        CodeBody::None => "nil".to_string(),
        CodeBody::File(f) => {
            imports.push("\"os\"");
            setup.push_str(&format!(
                "{i}payload, _ := os.Open({})\n{i}defer payload.Close()\n",
                dq(f)
            ));
            "payload".into()
        }
        CodeBody::Multipart(ps) => {
            imports.extend(["\"bytes\"", "\"mime/multipart\"", "\"os\""]);
            setup.push_str(&format!(
                "{i}payload := &bytes.Buffer{{}}\n{i}writer := multipart.NewWriter(payload)\n"
            ));
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => setup.push_str(&format!(
                        "{i}file, _ := os.ReadFile({})\n{i}part, _ := writer.CreateFormFile({}, {})\n{i}part.Write(file)\n",
                        dq(f),
                        dq(&p.name),
                        dq(f.rsplit('/').next().unwrap_or(f))
                    )),
                    (_, v) => setup.push_str(&format!("{i}writer.WriteField({}, {})\n", dq(&p.name), dq(&v.clone().unwrap_or_default()))),
                }
            }
            setup.push_str(&format!("{i}writer.Close()\n"));
            "payload".into()
        }
        _ => {
            imports.push("\"strings\"");
            setup.push_str(&format!(
                "{i}payload := strings.NewReader({})\n",
                dq(&r.body_text().unwrap_or_default())
            ));
            "payload".into()
        }
    };
    imports.sort();
    imports.dedup();
    let mut out = format!(
        "package main\n\nimport (\n{}\n)\n\nfunc main() {{\n",
        imports
            .iter()
            .map(|m| format!("{i}{m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    out.push_str(&setup);
    out.push_str(&format!(
        "{i}req, _ := http.NewRequest({}, {}, {body_expr})\n",
        dq(&r.method),
        dq(&r.url)
    ));
    for (k, v) in &r.headers {
        out.push_str(&format!("{i}req.Header.Add({}, {})\n", dq(k), dq(v)));
    }
    if matches!(r.body, CodeBody::Multipart(_)) {
        out.push_str(&format!(
            "{i}req.Header.Set(\"Content-Type\", writer.FormDataContentType())\n"
        ));
    }
    out.push_str(&format!("{i}res, err := http.DefaultClient.Do(req)\n{i}if err != nil {{\n{i}{i}panic(err)\n{i}}}\n{i}defer res.Body.Close()\n{i}body, _ := io.ReadAll(res.Body)\n{i}fmt.Println(string(body))\n}}"));
    out
}

fn rust(r: &CodeRequest, i: &str) -> String {
    let mut out = String::from(
        "#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n",
    );
    out.push_str(&format!("{i}let client = reqwest::Client::new();\n"));
    let mut chain = format!(
        "{i}let response = client\n{i}{i}.request(reqwest::Method::from_bytes(b{})?, {})",
        dq(&r.method),
        dq(&r.url)
    );
    for (k, v) in &r.headers {
        if matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        chain.push_str(&format!("\n{i}{i}.header({}, {})", dq(k), dq(v)));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => chain.push_str(&format!("\n{i}{i}.body(std::fs::read({})?)", dq(f))),
        CodeBody::Form(pairs) => {
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("({}, {})", dq(k), dq(v)))
                .collect();
            chain.push_str(&format!("\n{i}{i}.form(&[{}])", items.join(", ")));
        }
        CodeBody::Multipart(ps) => {
            let mut form = String::from("reqwest::multipart::Form::new()");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => form.push_str(&format!(
                        "\n{i}{i}.part({}, reqwest::multipart::Part::bytes(std::fs::read({})?).file_name({}))",
                        dq(&p.name),
                        dq(f),
                        dq(f.rsplit('/').next().unwrap_or(f))
                    )),
                    (_, v) => form.push_str(&format!("\n{i}{i}.text({}, {})", dq(&p.name), dq(&v.clone().unwrap_or_default()))),
                }
            }
            out.push_str(&format!("{i}let form = {form};\n"));
            chain.push_str(&format!("\n{i}{i}.multipart(form)"));
        }
        CodeBody::Text(t) => chain.push_str(&format!("\n{i}{i}.body({})", dq(t))),
    }
    chain.push_str(&format!("\n{i}{i}.send()\n{i}{i}.await?;\n"));
    out.push_str(&chain);
    out.push_str(&format!(
        "{i}println!(\"{{}}\", response.text().await?);\n{i}Ok(())\n}}"
    ));
    out
}

// ----- JVM ---------------------------------------------------------------------------------------

fn okhttp(r: &CodeRequest, i: &str, kotlin: bool) -> String {
    let lit = |s: &str| if kotlin { dq_dollar(s) } else { dq(s) };
    let content_type = r.header("content-type").unwrap_or("text/plain").to_string();
    let (new, end) = if kotlin { ("", "") } else { ("new ", ";") };
    let decl = |name: &str, ty: &str| {
        if kotlin {
            format!("val {name}")
        } else {
            format!("{ty} {name}")
        }
    };
    // Kotlin uses the OkHttp 4 extension functions, Java the static factories.
    let media = |t: &str| {
        if kotlin {
            format!("{}.toMediaType()", lit(t))
        } else {
            format!("MediaType.parse({})", lit(t))
        }
    };
    let text_body = |text: &str, t: &str| {
        if kotlin {
            format!("{}.toRequestBody({})", lit(text), media(t))
        } else {
            format!("RequestBody.create({}, {})", lit(text), media(t))
        }
    };
    let file_body = |path: &str, t: &str| {
        if kotlin {
            format!("File({}).asRequestBody({})", lit(path), media(t))
        } else {
            format!("RequestBody.create(new File({}), {})", lit(path), media(t))
        }
    };
    let mut out = format!(
        "{} = {new}OkHttpClient(){end}\n",
        decl("client", "OkHttpClient")
    );
    let body = match &r.body {
        CodeBody::None if matches!(r.method.as_str(), "POST" | "PUT" | "PATCH") => {
            Some(if kotlin {
                "ByteArray(0).toRequestBody()".to_string()
            } else {
                "RequestBody.create(new byte[0], null)".to_string()
            })
        }
        CodeBody::None => None,
        CodeBody::File(f) => {
            out.push_str(&format!(
                "{} = {}{end}\n",
                decl("body", "RequestBody"),
                file_body(f, &content_type)
            ));
            Some("body".into())
        }
        CodeBody::Multipart(ps) => {
            let mut s = format!(
                "{} = {new}MultipartBody.Builder()\n{i}.setType(MultipartBody.FORM)",
                decl("body", "RequestBody")
            );
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => s.push_str(&format!(
                        "\n{i}.addFormDataPart({}, {}, {})",
                        lit(&p.name),
                        lit(f.rsplit('/').next().unwrap_or(f)),
                        file_body(f, "application/octet-stream")
                    )),
                    (_, v) => s.push_str(&format!(
                        "\n{i}.addFormDataPart({}, {})",
                        lit(&p.name),
                        lit(&v.clone().unwrap_or_default())
                    )),
                }
            }
            s.push_str(&format!("\n{i}.build(){end}\n"));
            out.push_str(&s);
            Some("body".into())
        }
        _ => {
            out.push_str(&format!(
                "{} = {}{end}\n",
                decl("body", "RequestBody"),
                text_body(&r.body_text().unwrap_or_default(), &content_type)
            ));
            Some("body".into())
        }
    };
    out.push_str(&format!(
        "{} = {new}Request.Builder()\n{i}.url({})\n",
        decl("request", "Request"),
        lit(&r.url)
    ));
    out.push_str(&format!(
        "{i}.method({}, {})\n",
        lit(&r.method),
        body.unwrap_or_else(|| "null".into())
    ));
    for (k, v) in &r.headers {
        if matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!("{i}.addHeader({}, {})\n", lit(k), lit(v)));
    }
    out.push_str(&format!("{i}.build(){end}\n\n"));
    if kotlin {
        out.push_str(&format!("client.newCall(request).execute().use {{ response ->\n{i}println(response.body?.string())\n}}"));
    } else {
        out.push_str(&format!("try (Response response = client.newCall(request).execute()) {{\n{i}System.out.println(response.body().string());\n}}"));
    }
    out
}

fn java_http_client(r: &CodeRequest, i: &str) -> String {
    let publisher = match &r.body {
        CodeBody::None => "HttpRequest.BodyPublishers.noBody()".to_string(),
        CodeBody::File(f) => format!(
            "HttpRequest.BodyPublishers.ofFile(java.nio.file.Path.of({}))",
            dq(f)
        ),
        CodeBody::Multipart(_) => {
            "HttpRequest.BodyPublishers.ofByteArray(body.toByteArray())".to_string()
        }
        _ => format!(
            "HttpRequest.BodyPublishers.ofString({})",
            dq(&r.body_text().unwrap_or_default())
        ),
    };
    let mut out = String::new();
    if let CodeBody::Multipart(ps) = &r.body {
        out.push_str(&format!("String boundary = {};\n", dq(BOUNDARY)));
        out.push_str("var body = new java.io.ByteArrayOutputStream();\n");
        for p in ps {
            match (&p.file, &p.value) {
                (Some(f), _) => {
                    out.push_str(&format!(
                        "body.write((\"--\" + boundary + \"\\r\\nContent-Disposition: form-data; name=\" + {} + \"; filename=\" + {} + \"\\r\\n\\r\\n\").getBytes());\n",
                        dq(&dq(&p.name)),
                        dq(&dq(file_name(f)))
                    ));
                    out.push_str(&format!("body.write(java.nio.file.Files.readAllBytes(java.nio.file.Path.of({})));\nbody.write(\"\\r\\n\".getBytes());\n", dq(f)));
                }
                (_, v) => out.push_str(&format!(
                    "body.write((\"--\" + boundary + \"\\r\\nContent-Disposition: form-data; name=\" + {} + \"\\r\\n\\r\\n\" + {} + \"\\r\\n\").getBytes());\n",
                    dq(&dq(&p.name)),
                    dq(&v.clone().unwrap_or_default())
                )),
            }
        }
        out.push_str("body.write((\"--\" + boundary + \"--\\r\\n\").getBytes());\n\n");
    }
    out.push_str("HttpRequest request = HttpRequest.newBuilder()\n");
    out.push_str(&format!("{i}.uri(URI.create({}))\n", dq(&r.url)));
    if matches!(r.body, CodeBody::Multipart(_)) {
        out.push_str(&format!(
            "{i}.header(\"Content-Type\", \"multipart/form-data; boundary=\" + boundary)\n"
        ));
    }
    for (k, v) in &r.headers {
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("connection") {
            continue;
        }
        out.push_str(&format!("{i}.header({}, {})\n", dq(k), dq(v)));
    }
    out.push_str(&format!(
        "{i}.method({}, {publisher})\n{i}.build();\n",
        dq(&r.method)
    ));
    out.push_str("HttpResponse<String> response = HttpClient.newHttpClient().send(request, HttpResponse.BodyHandlers.ofString());\n");
    out.push_str("System.out.println(response.body());");
    out
}

// ----- .NET ----------------------------------------------------------------------------------------

fn csharp_http_client(r: &CodeRequest, i: &str) -> String {
    let content_type = r.header("content-type").map(str::to_string);
    let mut out = String::from("using var client = new HttpClient();\n");
    out.push_str(&format!(
        "var request = new HttpRequestMessage(new HttpMethod({}), {});\n",
        dq(&r.method),
        dq(&r.url)
    ));
    for (k, v) in &r.headers {
        if k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!(
            "request.Headers.TryAddWithoutValidation({}, {});\n",
            dq(k),
            dq(v)
        ));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => {
            out.push_str(&format!(
                "request.Content = new ByteArrayContent(File.ReadAllBytes({}));\n",
                dq(f)
            ));
            if let Some(ct) = &content_type {
                out.push_str(&format!(
                    "request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse({});\n",
                    dq(ct)
                ));
            }
        }
        CodeBody::Form(pairs) => {
            out.push_str(
                "request.Content = new FormUrlEncodedContent(new Dictionary<string, string>\n{\n",
            );
            for (k, v) in pairs {
                out.push_str(&format!("{i}{{ {}, {} }},\n", dq(k), dq(v)));
            }
            out.push_str("});\n");
        }
        CodeBody::Multipart(ps) => {
            out.push_str("var content = new MultipartFormDataContent();\n");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => out.push_str(&format!(
                        "content.Add(new ByteArrayContent(File.ReadAllBytes({})), {}, {});\n",
                        dq(f),
                        dq(&p.name),
                        dq(f.rsplit('/').next().unwrap_or(f))
                    )),
                    (_, v) => out.push_str(&format!(
                        "content.Add(new StringContent({}), {});\n",
                        dq(&v.clone().unwrap_or_default()),
                        dq(&p.name)
                    )),
                }
            }
            out.push_str("request.Content = content;\n");
        }
        CodeBody::Text(t) => {
            out.push_str(&format!(
                "request.Content = new StringContent({}, System.Text.Encoding.UTF8, {});\n",
                dq(t),
                dq(content_type
                    .as_deref()
                    .unwrap_or("text/plain")
                    .split(';')
                    .next()
                    .unwrap_or("text/plain"))
            ));
        }
    }
    out.push_str("using var response = await client.SendAsync(request);\n");
    out.push_str("Console.WriteLine(await response.Content.ReadAsStringAsync());");
    out
}

fn csharp_restsharp(r: &CodeRequest) -> String {
    let method = r
        .method
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + &r.method[1..].to_lowercase())
        .unwrap_or_default();
    let mut out = format!("var client = new RestClient({});\n", dq(&r.url));
    out.push_str(&format!(
        "var request = new RestRequest(\"\", Method.{method});\n"
    ));
    for (k, v) in &r.headers {
        if matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!("request.AddHeader({}, {});\n", dq(k), dq(v)));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!("request.AddParameter(\"application/octet-stream\", File.ReadAllBytes({}), ParameterType.RequestBody);\n", dq(f))),
        CodeBody::Form(pairs) => {
            for (k, v) in pairs {
                out.push_str(&format!("request.AddParameter({}, {});\n", dq(k), dq(v)));
            }
        }
        CodeBody::Multipart(ps) => {
            out.push_str("request.AlwaysMultipartFormData = true;\n");
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => out.push_str(&format!("request.AddFile({}, {});\n", dq(&p.name), dq(f))),
                    (_, v) => out.push_str(&format!("request.AddParameter({}, {});\n", dq(&p.name), dq(&v.clone().unwrap_or_default()))),
                }
            }
        }
        CodeBody::Text(t) => out.push_str(&format!(
            "request.AddStringBody({}, {});\n",
            dq(t),
            dq(r.header("content-type").unwrap_or("text/plain"))
        )),
    }
    out.push_str(
        "var response = await client.ExecuteAsync(request);\nConsole.WriteLine(response.Content);",
    );
    out
}

// ----- PHP, Ruby ------------------------------------------------------------------------------------

fn php_curl(r: &CodeRequest, i: &str) -> String {
    let q = sq_literal;
    let mut out = String::from("<?php\n\n$curl = curl_init();\n\ncurl_setopt_array($curl, [\n");
    out.push_str(&format!("{i}CURLOPT_URL => {},\n", q(&r.url)));
    out.push_str(&format!(
        "{i}CURLOPT_RETURNTRANSFER => true,\n{i}CURLOPT_CUSTOMREQUEST => {},\n",
        q(&r.method)
    ));
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!(
            "{i}CURLOPT_POSTFIELDS => file_get_contents({}),\n",
            q(f)
        )),
        CodeBody::Multipart(ps) => {
            out.push_str(&format!("{i}CURLOPT_POSTFIELDS => [\n"));
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => out.push_str(&format!(
                        "{i}{i}{} => new CURLFile({}),\n",
                        q(&p.name),
                        q(f)
                    )),
                    (_, v) => out.push_str(&format!(
                        "{i}{i}{} => {},\n",
                        q(&p.name),
                        q(&v.clone().unwrap_or_default())
                    )),
                }
            }
            out.push_str(&format!("{i}],\n"));
        }
        _ => out.push_str(&format!(
            "{i}CURLOPT_POSTFIELDS => {},\n",
            q(&r.body_text().unwrap_or_default())
        )),
    }
    let headers: Vec<_> = r
        .headers
        .iter()
        .filter(|(k, _)| {
            !(matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type"))
        })
        .collect();
    if !headers.is_empty() {
        out.push_str(&format!("{i}CURLOPT_HTTPHEADER => [\n"));
        for (k, v) in headers {
            out.push_str(&format!("{i}{i}{},\n", q(&format!("{k}: {v}"))));
        }
        out.push_str(&format!("{i}],\n"));
    }
    out.push_str("]);\n\n$response = curl_exec($curl);\ncurl_close($curl);\n\necho $response;");
    out
}

fn php_guzzle(r: &CodeRequest, i: &str) -> String {
    let q = sq_literal;
    let mut out = String::from(
        "<?php\n\nrequire 'vendor/autoload.php';\n\n$client = new \\GuzzleHttp\\Client();\n\n",
    );
    out.push_str(&format!(
        "$response = $client->request({}, {}, [\n",
        q(&r.method),
        q(&r.url)
    ));
    let headers: Vec<_> = r
        .headers
        .iter()
        .filter(|(k, _)| {
            !(matches!(r.body, CodeBody::Multipart(_) | CodeBody::Form(_))
                && k.eq_ignore_ascii_case("content-type"))
        })
        .collect();
    if !headers.is_empty() {
        out.push_str(&format!("{i}'headers' => [\n"));
        for (k, v) in headers {
            out.push_str(&format!("{i}{i}{} => {},\n", q(k), q(v)));
        }
        out.push_str(&format!("{i}],\n"));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!("{i}'body' => fopen({}, 'r'),\n", q(f))),
        CodeBody::Form(pairs) => {
            out.push_str(&format!("{i}'form_params' => [\n"));
            for (k, v) in pairs {
                out.push_str(&format!("{i}{i}{} => {},\n", q(k), q(v)));
            }
            out.push_str(&format!("{i}],\n"));
        }
        CodeBody::Multipart(ps) => {
            out.push_str(&format!("{i}'multipart' => [\n"));
            for p in ps {
                let contents = match (&p.file, &p.value) {
                    (Some(f), _) => format!("fopen({}, 'r')", q(f)),
                    (_, v) => q(&v.clone().unwrap_or_default()),
                };
                out.push_str(&format!(
                    "{i}{i}['name' => {}, 'contents' => {contents}],\n",
                    q(&p.name)
                ));
            }
            out.push_str(&format!("{i}],\n"));
        }
        CodeBody::Text(t) => out.push_str(&format!("{i}'body' => {},\n", q(t))),
    }
    out.push_str("]);\n\necho $response->getBody();");
    out
}

fn ruby(r: &CodeRequest) -> String {
    let q = sq_literal;
    let class = {
        let m = r.method.to_lowercase();
        let mut c = m.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    };
    let mut out = String::from("require 'uri'\nrequire 'net/http'\n\n");
    out.push_str(&format!("url = URI({})\n\n", q(&r.url)));
    out.push_str(
        "http = Net::HTTP.new(url.host, url.port)\nhttp.use_ssl = url.scheme == 'https'\n\n",
    );
    let known = [
        "Get", "Post", "Put", "Delete", "Patch", "Head", "Options", "Trace",
    ]
    .contains(&class.as_str());
    if known {
        out.push_str(&format!("request = Net::HTTP::{class}.new(url)\n"));
    } else {
        out.push_str(&format!(
            "request = Net::HTTPGenericRequest.new({}, true, true, url)\n",
            q(&r.method)
        ));
    }
    for (k, v) in &r.headers {
        if matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!("request[{}] = {}\n", q(k), q(v)));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!("request.body = File.binread({})\n", q(f))),
        CodeBody::Multipart(ps) => {
            let items: Vec<String> = ps
                .iter()
                .map(|p| match (&p.file, &p.value) {
                    (Some(f), _) => format!("[{}, File.open({})]", q(&p.name), q(f)),
                    (_, v) => format!("[{}, {}]", q(&p.name), q(&v.clone().unwrap_or_default())),
                })
                .collect();
            out.push_str(&format!(
                "request.set_form([{}], 'multipart/form-data')\n",
                items.join(", ")
            ));
        }
        _ => out.push_str(&format!(
            "request.body = {}\n",
            q(&r.body_text().unwrap_or_default())
        )),
    }
    out.push_str("\nresponse = http.request(request)\nputs response.read_body");
    out
}

// ----- Swift, Dart -----------------------------------------------------------------------------------

fn swift(r: &CodeRequest, i: &str) -> String {
    let mut out = String::from("import Foundation\n\n");
    out.push_str(&format!(
        "var request = URLRequest(url: URL(string: {})!)\n",
        dq(&r.url)
    ));
    out.push_str(&format!("request.httpMethod = {}\n", dq(&r.method)));
    for (k, v) in &r.headers {
        if matches!(r.body, CodeBody::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!(
            "request.setValue({}, forHTTPHeaderField: {})\n",
            dq(v),
            dq(k)
        ));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!(
            "request.httpBody = try Data(contentsOf: URL(fileURLWithPath: {}))\n",
            dq(f)
        )),
        CodeBody::Multipart(ps) => {
            out.push_str("let boundary = UUID().uuidString\n");
            out.push_str("request.setValue(\"multipart/form-data; boundary=\\(boundary)\", forHTTPHeaderField: \"Content-Type\")\n");
            out.push_str("var body = Data()\n");
            for p in ps {
                out.push_str("body.append(\"--\\(boundary)\\r\\n\".data(using: .utf8)!)\n");
                match (&p.file, &p.value) {
                    (Some(f), _) => {
                        out.push_str(&format!(
                            "body.append({}.data(using: .utf8)!)\n",
                            dq(&format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\r\n", p.name, f.rsplit('/').next().unwrap_or(f)))
                        ));
                        out.push_str(&format!(
                            "body.append(try Data(contentsOf: URL(fileURLWithPath: {})))\n",
                            dq(f)
                        ));
                    }
                    (_, v) => {
                        out.push_str(&format!(
                            "body.append({}.data(using: .utf8)!)\n",
                            dq(&format!(
                                "Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}",
                                p.name,
                                v.clone().unwrap_or_default()
                            ))
                        ));
                    }
                }
                out.push_str("body.append(\"\\r\\n\".data(using: .utf8)!)\n");
            }
            out.push_str("body.append(\"--\\(boundary)--\\r\\n\".data(using: .utf8)!)\nrequest.httpBody = body\n");
        }
        _ => out.push_str(&format!(
            "request.httpBody = {}.data(using: .utf8)\n",
            dq(&r.body_text().unwrap_or_default())
        )),
    }
    out.push_str("\nlet (data, _) = try await URLSession.shared.data(for: request)\n");
    out.push_str("print(String(decoding: data, as: UTF8.self))");
    let _ = i;
    out
}

fn dart(r: &CodeRequest, i: &str) -> String {
    let q = dq_dollar;
    let mut out = String::from("import 'package:http/http.dart' as http;\n");
    if needs_fs(r) {
        out.push_str("import 'dart:io';\n");
    }
    out.push_str("\nFuture<void> main() async {\n");
    let multipart = matches!(r.body, CodeBody::Multipart(_));
    if multipart {
        out.push_str(&format!(
            "{i}final request = http.MultipartRequest({}, Uri.parse({}));\n",
            q(&r.method),
            q(&r.url)
        ));
    } else {
        out.push_str(&format!(
            "{i}final request = http.Request({}, Uri.parse({}));\n",
            q(&r.method),
            q(&r.url)
        ));
    }
    for (k, v) in &r.headers {
        if multipart && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!("{i}request.headers[{}] = {};\n", q(k), q(v)));
    }
    match &r.body {
        CodeBody::None => {}
        CodeBody::File(f) => out.push_str(&format!(
            "{i}request.bodyBytes = await File({}).readAsBytes();\n",
            q(f)
        )),
        CodeBody::Form(pairs) => {
            out.push_str(&format!("{i}request.bodyFields = {{\n"));
            for (k, v) in pairs {
                out.push_str(&format!("{i}{i}{}: {},\n", q(k), q(v)));
            }
            out.push_str(&format!("{i}}};\n"));
        }
        CodeBody::Multipart(ps) => {
            for p in ps {
                match (&p.file, &p.value) {
                    (Some(f), _) => out.push_str(&format!(
                        "{i}request.files.add(await http.MultipartFile.fromPath({}, {}));\n",
                        q(&p.name),
                        q(f)
                    )),
                    (_, v) => out.push_str(&format!(
                        "{i}request.fields[{}] = {};\n",
                        q(&p.name),
                        q(&v.clone().unwrap_or_default())
                    )),
                }
            }
        }
        CodeBody::Text(t) => out.push_str(&format!("{i}request.body = {};\n", q(t))),
    }
    out.push_str(&format!("\n{i}final response = await request.send();\n{i}print(await response.stream.bytesToString());\n}}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals() {
        assert_eq!(dq("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(sq("it's"), "'it\\'s'");
        assert_eq!(dq_dollar("$x"), "\"\\$x\"");
        assert_eq!(shell_quote("it's", Shell::Bash), "'it'\\''s'");
        assert_eq!(shell_quote("plain-value", Shell::Bash), "plain-value");
        assert_eq!(shell_quote("it's", Shell::Pwsh), "'it''s'");
        assert_eq!(shell_quote("100%", Shell::Cmd), "\"100%%\"");
    }

    #[test]
    fn every_target_has_metadata() {
        assert_eq!(targets().len(), TARGETS.len());
        let ids: std::collections::HashSet<_> = TARGETS.iter().map(|t| t.0).collect();
        assert_eq!(ids.len(), TARGETS.len());
    }
}
