//! Run reports as JSON, JUnit XML and a standalone HTML page.

use std::fmt::Write;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::run::{RunItem, RunReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReportFormat {
    Json,
    Junit,
    Html,
}

impl ReportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Junit => "xml",
            Self::Html => "html",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RenderedReport {
    /// Suggested file name, e.g. `My API-report.xml`.
    pub file_name: String,
    pub text: String,
}

pub fn render_report(report: &RunReport, format: ReportFormat) -> RenderedReport {
    let text = match format {
        ReportFormat::Json => json(report),
        ReportFormat::Junit => junit(report),
        ReportFormat::Html => html(report),
    };
    RenderedReport {
        file_name: format!("{}-report.{}", file_stem(&report.name), format.extension()),
        text,
    }
}

fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let stem = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if stem.is_empty() { "run".into() } else { stem }
}

fn json(report: &RunReport) -> String {
    let mut value = serde_json::to_value(report).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("passed".into(), report.passed().into());
        map.insert(
            "stats".into(),
            serde_json::to_value(report.stats()).unwrap_or_default(),
        );
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Escapes text for XML and HTML content and attributes.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            // Not allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// `Folder / Request`, plus the iteration when the run had several.
fn title(report: &RunReport, item: &RunItem) -> String {
    let mut parts = item.path.clone();
    parts.push(item.name.clone());
    let mut name = parts.join(" / ");
    if report.iterations > 1 {
        name.push_str(&format!(" (iteration {})", item.iteration + 1));
    }
    name
}

fn seconds(ms: Option<f64>) -> String {
    format!("{:.3}", ms.unwrap_or(0.0) / 1000.0)
}

/// One check of a request: (name, failure message, skipped).
fn checks(item: &RunItem) -> Vec<(String, Option<String>, bool)> {
    let mut out = Vec::new();
    for a in &item.assertions {
        let name = a.describe();
        let failure = (!a.passed).then(|| {
            if let Some(e) = &a.error {
                format!("{}: {}", e.code, e.message)
            } else if let Some(d) = &a.detail {
                d.clone()
            } else {
                match &a.actual {
                    Some(actual) => format!("got {actual}"),
                    None => "no value".to_string(),
                }
            }
        });
        out.push((name, failure, false));
    }
    for t in &item.tests {
        let failure = (!t.passed).then(|| t.error.clone().unwrap_or_default());
        out.push((t.name.clone(), failure, t.skipped));
    }
    out
}

fn junit(report: &RunReport) -> String {
    let mut suites = String::new();
    let (mut tests, mut failures, mut errors, mut total_ms) = (0, 0, 0, 0.0);
    for item in &report.items {
        let checks = checks(item);
        let suite_failures = checks.iter().filter(|c| c.1.is_some()).count();
        let suite_skipped = checks.iter().filter(|c| c.2).count();
        let suite_errors = usize::from(item.error.is_some());
        let suite_tests = checks.len() + suite_errors;
        tests += suite_tests;
        failures += suite_failures;
        errors += suite_errors;
        total_ms += item.total_ms.unwrap_or(0.0);
        let name = title(report, item);
        let _ = writeln!(
            suites,
            r#"  <testsuite name="{}" tests="{suite_tests}" failures="{suite_failures}" errors="{suite_errors}" skipped="{suite_skipped}" time="{}">"#,
            esc(&name),
            seconds(item.total_ms),
        );
        let class = esc(&format!("{} {}", item.method, item.url));
        if let Some(e) = &item.error {
            let _ = writeln!(
                suites,
                r#"    <testcase name="{}" classname="{class}"><error type="{}" message="{}"/></testcase>"#,
                esc(&item.name),
                esc(&e.code),
                esc(&e.message),
            );
        }
        for (check, failure, skipped) in checks {
            let _ = write!(
                suites,
                r#"    <testcase name="{}" classname="{class}">"#,
                esc(&check)
            );
            if skipped {
                suites.push_str("<skipped/>");
            } else if let Some(message) = failure {
                let _ = write!(suites, r#"<failure message="{}"/>"#, esc(&message));
            }
            suites.push_str("</testcase>\n");
        }
        suites.push_str("  </testsuite>\n");
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"{}\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\" time=\"{}\">\n{suites}</testsuites>\n",
        esc(&report.name),
        seconds(Some(total_ms)),
    )
}

const STYLE: &str = r#"
:root { color-scheme: light dark; --fg: #1f2328; --muted: #656d76; --bg: #fff; --line: #d0d7de; --ok: #1a7f37; --bad: #cf222e; }
@media (prefers-color-scheme: dark) { :root { --fg: #e6edf3; --muted: #8d96a0; --bg: #0d1117; --line: #30363d; --ok: #3fb950; --bad: #f85149; } }
body { margin: 0 auto; max-width: 960px; padding: 24px 16px; font: 14px/1.5 system-ui, sans-serif; color: var(--fg); background: var(--bg); }
h1 { font-size: 20px; margin: 0 0 4px; }
.meta, .muted { color: var(--muted); }
.stats { display: flex; flex-wrap: wrap; gap: 16px; margin: 16px 0; padding: 12px 16px; border: 1px solid var(--line); border-radius: 8px; }
.stats b { display: block; font-size: 18px; }
details { border-top: 1px solid var(--line); padding: 8px 0; }
summary { cursor: pointer; display: flex; gap: 8px; align-items: baseline; }
.ok { color: var(--ok); } .bad { color: var(--bad); }
code { font: 12px ui-monospace, monospace; word-break: break-all; }
ul { margin: 8px 0 0; padding-left: 24px; }
li .why { display: block; font: 12px ui-monospace, monospace; color: var(--bad); white-space: pre-wrap; }
"#;

fn html(report: &RunReport) -> String {
    let stats = report.stats();
    let mut body = String::new();
    let mut iteration = None;
    for item in &report.items {
        if report.iterations > 1 && iteration != Some(item.iteration) {
            iteration = Some(item.iteration);
            let _ = writeln!(body, "<h2>Iteration {}</h2>", item.iteration + 1);
        }
        let passed = item.passed();
        let status = match (&item.error, item.status) {
            (Some(e), _) => format!("{}: {}", e.code, e.message),
            (None, Some(s)) => s.to_string(),
            (None, None) => String::new(),
        };
        let _ = writeln!(
            body,
            r#"<details{}><summary><span class="{}">{}</span> <b>{}</b> {} <span class="muted">{}{}</span></summary>"#,
            if passed { "" } else { " open" },
            if passed { "ok" } else { "bad" },
            if passed { "✓" } else { "✗" },
            esc(&item.method),
            esc(&title(report, item)),
            esc(&status),
            item.total_ms
                .map(|ms| format!(" · {ms:.0} ms"))
                .unwrap_or_default(),
        );
        let _ = writeln!(body, "<code>{}</code>", esc(&item.url));
        let checks = checks(item);
        if !checks.is_empty() {
            body.push_str("<ul>\n");
            for (name, failure, skipped) in checks {
                let (class, mark) = match (&failure, skipped) {
                    (_, true) => ("muted", "–"),
                    (Some(_), _) => ("bad", "✗"),
                    (None, _) => ("ok", "✓"),
                };
                let _ = write!(
                    body,
                    r#"<li><span class="{class}">{mark}</span> {}"#,
                    esc(&name)
                );
                if let Some(why) = failure {
                    let _ = write!(body, r#"<span class="why">{}</span>"#, esc(&why));
                }
                body.push_str("</li>\n");
            }
            body.push_str("</ul>\n");
        }
        body.push_str("</details>\n");
    }
    let state = if report.cancelled {
        "Stopped"
    } else if report.stopped {
        "Stopped at the first failure"
    } else if report.passed() {
        "Passed"
    } else {
        "Failed"
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{name} — run report</title>
<style>{STYLE}</style>
</head>
<body>
<h1>{name}</h1>
<p class="meta">{environment}{iterations} iteration(s) · {duration:.1} s · <span class="{state_class}">{state}</span></p>
<div class="stats">
<div><b>{requests}</b>requests</div>
<div><b class="{failed_class}">{failed_requests}</b>failed requests</div>
<div><b>{checks}</b>checks</div>
<div><b class="{failed_checks_class}">{failed_checks}</b>failed checks</div>
<div><b>{skipped}</b>skipped</div>
</div>
{body}</body>
</html>
"#,
        name = esc(&report.name),
        environment = report
            .environment
            .as_deref()
            .map(|e| format!("Environment: {} · ", esc(e)))
            .unwrap_or_default(),
        iterations = report.iterations,
        duration = stats.duration_ms as f64 / 1000.0,
        state_class = if report.passed() { "ok" } else { "bad" },
        requests = stats.requests,
        failed_requests = stats.failed_requests,
        failed_class = if stats.failed_requests > 0 {
            "bad"
        } else {
            "ok"
        },
        checks = stats.checks,
        failed_checks = stats.failed_checks,
        failed_checks_class = if stats.failed_checks > 0 { "bad" } else { "ok" },
        skipped = stats.skipped_checks,
    )
}
