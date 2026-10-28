//! `reqly-cli`: runs a collection's requests and checks from the command
//! line, for CI (PRD 7.16). The collection is imported into an in-memory
//! core, from an export file or from a Reqly server, and run there.

mod args;
mod server;

use std::io::Write;

use reqly_api::commands::{AppContext, Platform, registry};
use reqly_core::model::{AssertionOperator, NodeKind};
use reqly_storage::Storage;
use reqly_storage::blobs::BlobStore;
use reqly_testing::data::{DataFormat, parse_data};
use reqly_testing::report::{ReportFormat, render_report};
use reqly_testing::run::{RunItem, RunReport};
use serde_json::{Map, Value, json};

use args::{Command, Reporter, RunArgs, USAGE};

/// Runs the CLI without colors; returns the exit code.
pub async fn main_with(args: Vec<String>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    run_cli(args, out, err, false).await
}

/// Runs the CLI; `color` allows colors in the pretty output.
pub async fn run_cli(
    args: Vec<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
    color: bool,
) -> i32 {
    let command = match args::parse(&args) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "error: {e}\n\n{USAGE}");
            return 2;
        }
    };
    match command {
        Command::Help => {
            let _ = write!(out, "{USAGE}");
            0
        }
        Command::Version => {
            let _ = writeln!(out, "reqly-cli {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Command::Run(run) => {
            let color = color && !run.no_color && std::env::var_os("NO_COLOR").is_none();
            match execute(&run, out, color).await {
                Ok(report) if report.passed() => 0,
                Ok(_) => 1,
                Err(e) => {
                    let _ = writeln!(err, "error: {e}");
                    2
                }
            }
        }
    }
}

async fn call(ctx: &AppContext, name: &str, input: Value) -> Result<Value, String> {
    registry()
        .dispatch(ctx, name, input)
        .await
        .map_err(|e| e.message)
}

/// The export to run: file name, content and whether it is base64 (binary).
async fn source(run: &RunArgs) -> Result<(String, String, bool), String> {
    if let Some(server) = &run.server {
        let token = run
            .token
            .clone()
            .or_else(|| std::env::var("REQLY_TOKEN").ok());
        let wanted = run.collection.as_deref().unwrap_or_default();
        let (name, text) = server::export_collection(server, token.as_deref(), wanted).await?;
        return Ok((name, text, false));
    }
    let path = run.file.as_deref().unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    Ok(match String::from_utf8(bytes) {
        Ok(text) => (name, text, false),
        // Zip archives (Bruno, Insomnia).
        Err(e) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(e.into_bytes());
            (name, b64, true)
        }
    })
}

/// Imports the export and picks the collection to run.
async fn import(ctx: &AppContext, run: &RunArgs) -> Result<Value, String> {
    let (name, content, base64) = source(run).await?;
    let workspace = call(ctx, "workspace.ensureDefault", json!({})).await?[0]["id"].clone();
    let imported = call(
        ctx,
        "import.apply",
        json!({
            "sources": [{ "name": name, "content": content, "base64": base64 }],
            "workspaceId": workspace,
            "target": { "kind": "newCollections" },
        }),
    )
    .await?;
    let ids: Vec<Value> = imported["collectionIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let collections = call(ctx, "collection.list", json!({ "workspaceId": workspace })).await?;
    let imported: Vec<&Value> = collections
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| ids.contains(&c["id"]))
        .collect();
    let wanted = run.collection.as_deref().filter(|_| run.server.is_none());
    let pick = match (wanted, imported.as_slice()) {
        (_, []) => return Err(format!("{name}: no collection to run")),
        (Some(w), list) => list
            .iter()
            .find(|c| c["name"] == w || c["id"] == w)
            .ok_or_else(|| format!("{name}: no collection `{w}`"))?,
        (None, [one]) => one,
        (None, list) => {
            let names: Vec<&str> = list.iter().filter_map(|c| c["name"].as_str()).collect();
            return Err(format!(
                "{name} has several collections; pick one with --collection ({})",
                names.join(", ")
            ));
        }
    };
    Ok((*pick).clone())
}

async fn environment_id(
    ctx: &AppContext,
    collection_id: &Value,
    name: &str,
) -> Result<Value, String> {
    let envs = call(
        ctx,
        "environment.list",
        json!({ "collectionId": collection_id }),
    )
    .await?;
    let subs: Vec<&Value> = envs["collection"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "sub")
        .collect();
    subs.iter()
        .find(|e| e["name"] == name)
        .map(|e| e["id"].clone())
        .ok_or_else(|| {
            let names: Vec<&str> = subs.iter().filter_map(|e| e["name"].as_str()).collect();
            format!(
                "no environment `{name}` (there are: {})",
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            )
        })
}

/// A folder by name or by `Parent/Child` path.
async fn folder_id(ctx: &AppContext, collection_id: &Value, wanted: &str) -> Result<Value, String> {
    let nodes = call(ctx, "tree.list", json!({ "collectionId": collection_id })).await?;
    let nodes = nodes.as_array().cloned().unwrap_or_default();
    let path_of = |node: &Value| {
        let mut parts = vec![node["name"].as_str().unwrap_or_default().to_string()];
        let mut parent = node["parentId"].clone();
        while let Some(p) = nodes
            .iter()
            .find(|n| n["id"] == parent && !parent.is_null())
        {
            parts.insert(0, p["name"].as_str().unwrap_or_default().to_string());
            parent = p["parentId"].clone();
        }
        parts.join("/")
    };
    let folder = serde_json::to_value(NodeKind::Folder).unwrap_or_default();
    nodes
        .iter()
        .filter(|n| n["kind"] == folder)
        .find(|n| n["name"] == wanted || path_of(n) == wanted.trim_matches('/'))
        .map(|n| n["id"].clone())
        .ok_or_else(|| format!("no folder `{wanted}`"))
}

async fn execute(run: &RunArgs, out: &mut dyn Write, color: bool) -> Result<RunReport, String> {
    let blobs = tempfile::tempdir().map_err(|e| e.to_string())?;
    let storage = Storage::in_memory().await.map_err(|e| e.message)?;
    // A local run, like the desktop app (Unix sockets, files, netrc).
    let ctx = AppContext::new(Platform::Desktop, storage, BlobStore::new(blobs.path()));
    let collection = import(&ctx, run).await?;
    let collection_id = collection["id"].clone();

    let mut input = json!({ "runId": "cli", "collectionId": collection_id });
    if let Some(env) = &run.env {
        input["environmentId"] = environment_id(&ctx, &collection_id, env).await?;
    }
    if let Some(folder) = &run.folder {
        input["folderId"] = folder_id(&ctx, &collection_id, folder).await?;
    }
    let mut rows = 0;
    if let Some(path) = &run.data {
        let format = DataFormat::from_path(path)
            .ok_or_else(|| format!("{path}: use a .csv or .json data file"))?;
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        rows = parse_data(format, &text)
            .map_err(|e| format!("{path}: {}", e.message))?
            .len();
        input["data"] = json!({ "format": format, "text": text });
    }
    if let Some(n) = run.iterations {
        input["iterations"] = json!(n);
    }
    if let Some(ms) = run.delay_ms {
        input["delayMs"] = json!(ms);
    }
    input["stopOnFailure"] = json!(run.bail);
    if !run.vars.is_empty() {
        let vars: Map<String, Value> = run
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let id: reqly_core::Id =
            serde_json::from_value(collection_id.clone()).map_err(|e| e.to_string())?;
        if let Ok(mut runtime) = ctx.runtime_vars.lock() {
            runtime.insert(id, vars);
        }
    }

    let iterations = run.iterations.unwrap_or(rows.max(1) as u32);
    let pretty = run.reporter == Reporter::Pretty;
    let mut printer = Pretty {
        color,
        iterations,
        last_iteration: None,
    };
    if pretty {
        let _ = writeln!(
            out,
            "{}\n",
            printer.bold(collection["name"].as_str().unwrap_or_default())
        );
    }

    let mut events = ctx.events.subscribe();
    let running = call(&ctx, "runner.run", input);
    tokio::pin!(running);
    let report = loop {
        tokio::select! {
            done = &mut running => break done?,
            Ok(event) = events.recv() => {
                if pretty && event.topic == "runner.progress"
                    && let Ok(item) = serde_json::from_value::<RunItem>(event.payload["item"].clone())
                {
                    printer.item(out, &item);
                }
            }
        }
    };
    // Items published after the last event was read.
    while let Ok(event) = events.try_recv() {
        if pretty
            && event.topic == "runner.progress"
            && let Ok(item) = serde_json::from_value::<RunItem>(event.payload["item"].clone())
        {
            printer.item(out, &item);
        }
    }
    let report: RunReport = serde_json::from_value(report).map_err(|e| e.to_string())?;

    match run.reporter {
        Reporter::Pretty => printer.summary(out, &report),
        Reporter::Json => {
            let _ = writeln!(out, "{}", render_report(&report, ReportFormat::Json).text);
        }
        Reporter::Junit => {
            let _ = write!(out, "{}", render_report(&report, ReportFormat::Junit).text);
        }
    }
    for path in &run.reports {
        let format = match path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
            Some(e) if e == "html" || e == "htm" => ReportFormat::Html,
            Some(e) if e == "xml" => ReportFormat::Junit,
            Some(e) if e == "json" => ReportFormat::Json,
            _ => return Err(format!("{path}: use .html, .xml or .json for a report")),
        };
        std::fs::write(path, render_report(&report, format).text)
            .map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(report)
}

struct Pretty {
    color: bool,
    iterations: u32,
    last_iteration: Option<u32>,
}

impl Pretty {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    fn mark(&self, passed: bool) -> String {
        if passed {
            self.paint("32", "✓")
        } else {
            self.paint("31", "✗")
        }
    }

    fn item(&mut self, out: &mut dyn Write, item: &RunItem) {
        if self.iterations > 1 && self.last_iteration != Some(item.iteration) {
            self.last_iteration = Some(item.iteration);
            let heading = format!("Iteration {}/{}", item.iteration + 1, self.iterations);
            let _ = writeln!(out, "{}", self.bold(&heading));
        }
        let mut name = item.path.clone();
        name.push(item.name.clone());
        let outcome = match (&item.error, item.status) {
            (Some(e), _) => self.paint("31", &e.message),
            (None, Some(s)) => s.to_string(),
            (None, None) => String::new(),
        };
        let time = item
            .total_ms
            .map(|ms| format!(" · {ms:.0} ms"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {} {} {} → {outcome}{}",
            self.mark(item.passed()),
            item.method,
            name.join(" / "),
            self.paint("2", &time),
        );
        for a in &item.assertions {
            let why = if a.passed {
                String::new()
            } else if let Some(e) = &a.error {
                format!(" — {}", e.message)
            } else if let Some(d) = &a.detail {
                format!(" — {}", d.replace('\n', "; "))
            } else if a.operator == AssertionOperator::NotExists || a.actual.is_none() {
                " — no value".into()
            } else {
                format!(" — got {}", a.actual.as_deref().unwrap_or_default())
            };
            let _ = writeln!(
                out,
                "      {} {}{}",
                self.mark(a.passed),
                a.describe(),
                self.paint("31", &why)
            );
        }
        for t in &item.tests {
            let mark = if t.skipped {
                self.paint("2", "–")
            } else {
                self.mark(t.passed)
            };
            let why = t
                .error
                .as_deref()
                .filter(|_| !t.passed)
                .map(|e| format!(" — {e}"))
                .unwrap_or_default();
            let _ = writeln!(out, "      {mark} {}{}", t.name, self.paint("31", &why));
        }
    }

    fn summary(&self, out: &mut dyn Write, report: &RunReport) {
        let s = report.stats();
        let failed = |n: u32| {
            let text = format!("{n} failed");
            if n > 0 { self.paint("31", &text) } else { text }
        };
        let _ = writeln!(
            out,
            "\n{} requests, {} · {} checks, {}, {} skipped · {:.1} s",
            s.requests,
            failed(s.failed_requests),
            s.checks,
            failed(s.failed_checks),
            s.skipped_checks,
            s.duration_ms as f64 / 1000.0,
        );
        if report.cancelled {
            let _ = writeln!(out, "{}", self.paint("31", "Stopped"));
        } else if report.stopped {
            let _ = writeln!(out, "{}", self.paint("31", "Stopped at the first failure"));
        }
    }
}
