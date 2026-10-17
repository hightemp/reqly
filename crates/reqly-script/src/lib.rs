//! Pre-request and after-response scripts (ADR-006): a QuickJS sandbox with
//! time and memory limits, the `rq` API and Postman (`pm`) and Insomnia
//! (`insomnia`) compatible shims. No file system or network access except
//! `sendRequest`, which goes through the [`ScriptHost`].

mod modules;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqly_core::{CoreError, CoreResult};
use rquickjs::prelude::{Async, Func};
use rquickjs::{
    AsyncContext, AsyncRuntime, CatchResultExt, CaughtError, Ctx, Function, Promise, Value,
};
use serde::{Deserialize, Serialize};
use serde_json::Map;
use ts_rs::TS;

const PRELUDE: &str = include_str!("prelude.js");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Phase {
    Pre,
    Post,
}

/// A key/value row as scripts see it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pair {
    pub key: String,
    pub value: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptBody {
    /// `none`, `raw`, `urlencoded`, `formdata`, `file` or `graphql`.
    pub mode: String,
    pub raw: String,
    pub fields: Vec<Pair>,
}

/// The request a script reads and changes (unrendered: templates stay).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    /// Without the query string.
    pub url: String,
    pub query: Vec<Pair>,
    pub headers: Vec<Pair>,
    pub body: ScriptBody,
    /// Read-only view of the auth settings.
    pub auth: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptResponse {
    pub code: u16,
    pub status: String,
    pub headers: Vec<(String, String)>,
    /// Body as text (lossy UTF-8).
    pub body: String,
    pub time_ms: f64,
    pub size: u64,
    /// Set when `sendRequest` failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Variable scopes as Postman names them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scopes {
    pub globals: Map<String, serde_json::Value>,
    /// Collection (base environment).
    pub collection: Map<String, serde_json::Value>,
    /// The active environment.
    pub environment: Map<String, serde_json::Value>,
    /// Iteration data (runner).
    pub data: Map<String, serde_json::Value>,
    /// Runtime variables (`pm.variables.set`), plus folder and request variables.
    pub local: Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptInfo {
    pub request_name: String,
    pub request_id: String,
    pub iteration: u32,
    pub iteration_count: u32,
}

#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub phase: Phase,
    /// Shown in errors and logs, e.g. `Folder A / pre-request`.
    pub label: String,
    pub script: String,
    pub request: ScriptRequest,
    pub response: Option<ScriptResponse>,
    pub variables: Scopes,
    pub info: ScriptInfo,
    pub limits: Limits,
    pub locale: reqly_template::Locale,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Script run time, not counting waits for `sendRequest` and timers.
    pub time: Duration,
    pub memory: usize,
    /// Upper bound for the whole run including waits.
    pub wall: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(5),
            memory: 64 << 20,
            wall: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VariableScope {
    Globals,
    Collection,
    Environment,
    Data,
    Local,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableChange {
    pub scope: VariableScope,
    pub key: String,
    pub value: serde_json::Value,
    pub unset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogEntry {
    /// `log`, `info`, `warn`, `error`, `debug`.
    pub level: String,
    pub text: String,
    /// Which script wrote it.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub skipped: bool,
    #[ts(optional)]
    pub error: Option<String>,
}

impl Default for TestResult {
    fn default() -> Self {
        Self {
            name: String::new(),
            passed: true,
            skipped: false,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptOutput {
    pub request: ScriptRequest,
    pub changes: Vec<VariableChange>,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogEntry>,
    pub skip_request: bool,
    /// `setNextRequest` (runner): `Some(None)` stops, `Some(Some(name))` jumps.
    pub next_request: Option<Option<String>>,
}

/// What a script may ask of the application.
pub trait ScriptHost: Send + Sync {
    /// `pm.sendRequest`; the request uses the same templates and settings.
    fn send(&self, request: SendSpec) -> impl Future<Output = ScriptResponse> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SendSpec {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Default for SendSpec {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            headers: Vec::new(),
            body: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Collected {
    request: ScriptRequest,
    changes: Vec<VariableChange>,
    tests: Vec<TestResult>,
    skip_request: bool,
    #[serde(default)]
    next_request: Option<Option<String>>,
}

fn script_error(code: &str, label: &str, message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new(code, format!("{label}: {message}"))
        .with_param("script", label.to_string())
        .with_param("message", message)
}

fn caught(err: CaughtError<'_>) -> String {
    match err {
        CaughtError::Exception(e) => {
            let mut text = e.message().unwrap_or_default();
            if let Some(stack) = e.stack().filter(|s| !s.is_empty()) {
                // The first frame points into the user's script.
                if let Some(line) = stack.lines().find(|l| l.contains("script:")) {
                    text.push_str(&format!(" ({})", line.trim().trim_start_matches("at ")));
                }
            }
            text
        }
        CaughtError::Value(v) => format!("{v:?}"),
        CaughtError::Error(e) => e.to_string(),
    }
}

/// Runs one script; errors in the script come back as `script.error` (with
/// the logs written before it in `params.logs`), limits as `script.timeout`
/// and `script.memory`.
pub async fn run<H: ScriptHost + 'static>(
    host: Arc<H>,
    input: ScriptInput,
) -> CoreResult<ScriptOutput> {
    let label = input.label.clone();
    let rt =
        AsyncRuntime::new().map_err(|e| script_error("script.error", &label, e.to_string()))?;
    rt.set_memory_limit(input.limits.memory).await;
    rt.set_max_stack_size(1 << 20).await;
    // The deadline moves forward while the script waits for the host.
    let deadline = Arc::new(Mutex::new(Instant::now() + input.limits.time));
    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let deadline = deadline.clone();
        let interrupted = interrupted.clone();
        rt.set_interrupt_handler(Some(Box::new(move || {
            let over = deadline.lock().map(|d| Instant::now() > *d).unwrap_or(true);
            if over {
                interrupted.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            over
        })))
        .await;
    }
    let ctx = AsyncContext::full(&rt)
        .await
        .map_err(|e| script_error("script.error", &label, e.to_string()))?;
    let logs: Arc<Mutex<Vec<LogEntry>>> = Arc::default();

    let input_json = serde_json::json!({
        "phase": input.phase,
        "request": input.request,
        "response": input.response,
        "variables": input.variables,
        "info": input.info,
    })
    .to_string();
    let script = input.script.clone();
    let locale = input.locale;
    let run_logs = logs.clone();
    let run_label = label.clone();
    let run = ctx.async_with(async move |ctx| -> Result<String, String> {
        install(&ctx, host, deadline, run_logs, run_label, locale).map_err(|e| e.to_string())?;
        ctx.globals()
            .set("__input", input_json)
            .map_err(|e| e.to_string())?;
        ctx.eval::<(), _>(PRELUDE).catch(&ctx).map_err(caught)?;
        // The script body runs as an async function so top-level `await` works.
        let source = format!("(async function () {{\n{script}\n}})().then(() => __settle())");
        let promise: Promise = ctx
            .eval_with_options(source, {
                let mut o = rquickjs::context::EvalOptions::default();
                o.filename = Some("script".into());
                o
            })
            .catch(&ctx)
            .map_err(caught)?;
        promise
            .into_future::<Value>()
            .await
            .catch(&ctx)
            .map_err(caught)?;
        let collect: Function = ctx.globals().get("__collect").map_err(|e| e.to_string())?;
        collect.call::<_, String>(()).catch(&ctx).map_err(caught)
    });
    let result = tokio::time::timeout(input.limits.wall, run).await;
    let logs = logs.lock().map(|l| l.clone()).unwrap_or_default();
    let with_logs =
        |e: CoreError| e.with_param("logs", serde_json::to_value(&logs).unwrap_or_default());
    let collected = match result {
        Err(_) => {
            return Err(with_logs(script_error(
                "script.timeout",
                &label,
                "the script did not finish in time",
            )));
        }
        Ok(Err(message)) => {
            if interrupted.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(with_logs(
                    script_error(
                        "script.timeout",
                        &label,
                        format!("stopped after {} s", input.limits.time.as_secs_f64()),
                    )
                    .with_param("seconds", input.limits.time.as_secs_f64()),
                ));
            }
            let code = if message.contains("out of memory") {
                "script.memory"
            } else {
                "script.error"
            };
            return Err(with_logs(script_error(code, &label, message)));
        }
        Ok(Ok(json)) => json,
    };
    let c: Collected = serde_json::from_str(&collected).map_err(|e| {
        with_logs(script_error(
            "script.error",
            &label,
            format!("invalid script state: {e}"),
        ))
    })?;
    Ok(ScriptOutput {
        request: c.request,
        changes: c.changes,
        tests: c.tests,
        logs,
        skip_request: c.skip_request,
        next_request: c.next_request,
    })
}

/// Native helpers used by the prelude.
fn install<'js, H: ScriptHost + 'static>(
    ctx: &Ctx<'js>,
    host: Arc<H>,
    deadline: Arc<Mutex<Instant>>,
    logs: Arc<Mutex<Vec<LogEntry>>>,
    label: String,
    locale: reqly_template::Locale,
) -> rquickjs::Result<()> {
    let g = ctx.globals();
    let source = label.clone();
    g.set(
        "__rq_log",
        Func::from(move |level: String, text: String| {
            if let Ok(mut l) = logs.lock() {
                // Keep the panel usable when a script logs in a loop.
                if l.len() < 5000 {
                    let text = if text.len() > 100_000 {
                        format!("{}…", &text[..text.floor_char_boundary(100_000)])
                    } else {
                        text
                    };
                    l.push(LogEntry {
                        level,
                        text,
                        source: source.clone(),
                    });
                }
            }
        }),
    )?;
    let send_deadline = deadline.clone();
    g.set(
        "__rq_send",
        Function::new(
            ctx.clone(),
            Async(move |json: String| {
                let host = host.clone();
                let deadline = send_deadline.clone();
                async move {
                    let started = Instant::now();
                    let spec: SendSpec = serde_json::from_str(&json).unwrap_or_default();
                    let response = host.send(spec).await;
                    extend(&deadline, started.elapsed());
                    rquickjs::Result::Ok(serde_json::to_string(&response).unwrap_or_default())
                }
            }),
        )?,
    )?;
    let sleep_deadline = deadline;
    g.set(
        "__rq_sleep",
        Function::new(
            ctx.clone(),
            Async(move |ms: f64| {
                let deadline = sleep_deadline.clone();
                async move {
                    let d = Duration::from_millis(ms.clamp(0.0, 60_000.0) as u64);
                    tokio::time::sleep(d).await;
                    extend(&deadline, d);
                    rquickjs::Result::Ok(())
                }
            }),
        )?,
    )?;
    g.set("__rq_uuid", Func::from(|| uuid::Uuid::new_v4().to_string()))?;
    g.set(
        "__rq_faker",
        Func::from(
            move |path: String, args: String| -> rquickjs::Result<String> {
                let mut values = vec![serde_json::Value::String(path)];
                if let Ok(serde_json::Value::Array(a)) = serde_json::from_str(&args) {
                    values.extend(a);
                }
                match reqly_template::functions::call("faker", &values, locale) {
                    Some(Ok(v)) => Ok(reqly_template::value_to_string(&v)),
                    Some(Err(e)) => Err(rquickjs::Error::new_from_js_message(
                        "faker", "string", e.message,
                    )),
                    None => Ok(String::new()),
                }
            },
        ),
    )?;
    g.set(
        "__rq_module",
        Func::from(|ctx: Ctx<'js>, name: String| modules::load(&ctx, &name)),
    )?;
    Ok(())
}

fn extend(deadline: &Mutex<Instant>, by: Duration) {
    if let Ok(mut d) = deadline.lock() {
        *d += by;
    }
}

/// Postman's sandbox API names used by a script that Reqly does not
/// implement (for import reports).
pub fn unsupported_calls(script: &str) -> Vec<String> {
    const UNSUPPORTED: &[&str] = &[
        "pm.vault",
        "pm.visualizer",
        "pm.cookies.jar",
        "pm.require(",
        "require('cheerio')",
        "require(\"cheerio\")",
        "require('xml2js')",
        "require(\"xml2js\")",
        "require('tv4')",
        "require(\"tv4\")",
        "require('ajv')",
        "require(\"ajv\")",
        "require('csv-parse",
        "require(\"csv-parse",
        "xml2Json(",
        "insomnia.parentFolders",
    ];
    let mut out: Vec<String> = UNSUPPORTED
        .iter()
        .filter(|u| script.contains(**u))
        .map(|u| u.trim_end_matches('(').to_string())
        .collect();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::unsupported_calls;

    #[test]
    fn finds_unsupported_calls() {
        let found = unsupported_calls(
            "pm.visualizer.set(t); const $ = require('cheerio'); pm.environment.set('a', 1)",
        );
        assert_eq!(found, ["pm.visualizer", "require('cheerio')"]);
        assert!(unsupported_calls("pm.test('x', () => pm.expect(1).to.eql(1))").is_empty());
    }
}
