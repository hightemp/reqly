//! The collection runner (PRD 7.16): sends the requests of a collection or
//! folder in order, once per iteration, and collects their checks.
//!
//! What a run changes about sending (iteration data, the environment, the
//! iteration number, `setNextRequest`) travels in a task-local [`RunScope`]
//! that variable scopes and scripts consult.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqly_core::model::{Node, NodeData, NodeKind, NodeSummary, Protocol};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_script::TestResult;
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::tree::TreeRepo;
use reqly_testing::AssertionResult;
use reqly_testing::data::{DataFormat, parse_data};
use reqly_testing::report::{RenderedReport, ReportFormat, render_report};
use reqly_testing::run::{RunItem, RunReport};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::watch;
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<RunnerRun>()
        .register::<RunnerCancel>()
        .register::<RunnerReport>();
}

// ----- run scope ---------------------------------------------------------------------

/// The run a send belongs to.
#[derive(Debug, Clone)]
pub(crate) struct RunScope {
    pub collection_id: Id,
    /// Replaces the collection's active environment.
    pub environment_id: Option<Id>,
    pub data: Map<String, Value>,
    pub iteration: u32,
    pub iteration_count: u32,
    /// `setNextRequest` from the scripts of the current request.
    pub next_request: Arc<Mutex<Option<Option<String>>>>,
}

tokio::task_local! {
    static RUN: RunScope;
}

/// The run the current task sends for, if it applies to `collection_id`.
pub(crate) fn current(collection_id: Id) -> Option<RunScope> {
    RUN.try_with(|r| r.clone())
        .ok()
        .filter(|r| r.collection_id == collection_id)
}

impl RunScope {
    pub fn set_next_request(&self, next: Option<String>) {
        if let Ok(mut slot) = self.next_request.lock() {
            *slot = Some(next);
        }
    }

    fn take_next_request(&self) -> Option<Option<String>> {
        self.next_request.lock().ok().and_then(|mut s| s.take())
    }
}

// ----- runs in progress --------------------------------------------------------------

struct RunHandle {
    owner: Option<Id>,
    cancel: watch::Sender<bool>,
    /// The request being sent, cancelled with the run.
    current: Mutex<Option<Id>>,
}

/// Runs in progress, by run id.
#[derive(Default)]
pub struct Runs(Mutex<HashMap<String, Arc<RunHandle>>>);

struct Registered<'a> {
    runs: &'a Runs,
    id: String,
}

impl Drop for Registered<'_> {
    fn drop(&mut self) {
        if let Ok(mut map) = self.runs.0.lock() {
            map.remove(&self.id);
        }
    }
}

// ----- commands ----------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunData {
    pub format: DataFormat,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunnerInput {
    /// Chosen by the caller; names the run in events and `runner.cancel`.
    pub run_id: String,
    pub collection_id: Id,
    /// Run this folder instead of the whole collection.
    #[serde(default)]
    #[ts(optional)]
    pub folder_id: Option<Id>,
    /// The requests to send, in this order (default: the tree order).
    #[serde(default)]
    #[ts(optional)]
    pub request_ids: Option<Vec<Id>>,
    /// Instead of the active environment.
    #[serde(default)]
    #[ts(optional)]
    pub environment_id: Option<Id>,
    /// Default: one per data row, or one.
    #[serde(default)]
    #[ts(optional)]
    pub iterations: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub data: Option<RunData>,
    /// Pause between requests.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub delay_ms: Option<u64>,
    /// Stop at the first request with an error or a failed check.
    #[serde(default)]
    #[ts(optional)]
    pub stop_on_failure: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunProgress {
    pub run_id: String,
    /// Requests planned: the request count times the iterations.
    pub total: u32,
    /// Requests done so far, this one included.
    pub done: u32,
    pub item: RunItem,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunFinished {
    pub run_id: String,
    pub passed: bool,
}

pub struct RunnerRun;

impl Command<AppContext> for RunnerRun {
    const NAME: &'static str = "runner.run";
    type Input = RunnerInput;
    type Output = RunReport;

    async fn run(ctx: &AppContext, input: RunnerInput) -> CoreResult<RunReport> {
        run(ctx, input).await
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunnerCancelInput {
    pub run_id: String,
}

pub struct RunnerCancel;

impl Command<AppContext> for RunnerCancel {
    const NAME: &'static str = "runner.cancel";
    type Input = RunnerCancelInput;
    type Output = Empty;

    async fn run(ctx: &AppContext, input: RunnerCancelInput) -> CoreResult<Empty> {
        let handle = ctx
            .runs
            .0
            .lock()
            .map_err(|_| CoreError::internal("runs lock poisoned"))?
            .get(&input.run_id)
            .cloned();
        let caller = crate::access::current().user_id();
        if let Some(handle) = handle.filter(|h| h.owner == caller) {
            handle.cancel.send_replace(true);
            let current = handle.current.lock().ok().and_then(|c| *c);
            if let Some(request_id) = current
                && let Ok(inflight) = ctx.inflight.lock()
                && let Some(token) = inflight.get(&request_id)
            {
                token.cancel();
            }
        }
        Ok(Empty {})
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunnerReportInput {
    pub report: RunReport,
    pub format: ReportFormat,
}

/// Renders a finished run as a file (JSON, JUnit XML or HTML).
pub struct RunnerReport;

impl Command<AppContext> for RunnerReport {
    const NAME: &'static str = "runner.report";
    type Input = RunnerReportInput;
    type Output = RenderedReport;

    async fn run(_ctx: &AppContext, input: RunnerReportInput) -> CoreResult<RenderedReport> {
        Ok(render_report(&input.report, input.format))
    }
}

// ----- the run -----------------------------------------------------------------------

/// The requests under `root` (the collection when `None`), depth first in tree order.
fn requests_under(tree: &[NodeSummary], root: Option<Id>) -> Vec<Id> {
    let mut children: HashMap<Option<Id>, Vec<&NodeSummary>> = HashMap::new();
    for node in tree {
        children.entry(node.parent_id).or_default().push(node);
    }
    let mut out = Vec::new();
    // Depth first, keeping the sibling order.
    fn visit(
        parent: Option<Id>,
        children: &HashMap<Option<Id>, Vec<&NodeSummary>>,
        out: &mut Vec<Id>,
    ) {
        for node in children.get(&parent).into_iter().flatten() {
            match node.kind {
                NodeKind::Folder => visit(Some(node.id), children, out),
                _ if sendable(node.protocol) => out.push(node.id),
                _ => {}
            }
        }
    }
    visit(root, &children, &mut out);
    out
}

/// Requests a run can send: not streams.
fn sendable(protocol: Option<Protocol>) -> bool {
    matches!(protocol, Some(Protocol::Http | Protocol::Graphql))
}

struct Planned {
    node: Node,
    path: Vec<String>,
}

async fn plan(ctx: &AppContext, input: &RunnerInput) -> CoreResult<(String, Vec<Planned>)> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let collection = tree.get_collection(input.collection_id).await?;
    let mut name = collection.name.clone();
    let ids = match &input.request_ids {
        Some(ids) => ids.clone(),
        None => {
            if let Some(folder) = input.folder_id {
                name = tree.get_node(folder).await?.name;
            }
            requests_under(&tree.list_tree(input.collection_id).await?, input.folder_id)
        }
    };
    let mut planned = Vec::new();
    for id in ids {
        let node = tree.get_node(id).await?;
        if node.collection_id != input.collection_id || node.meta.is_deleted() {
            return Err(CoreError::not_found("request", id));
        }
        let NodeData::Request(r) = &node.data else {
            continue;
        };
        if !sendable(Some(r.protocol())) {
            continue;
        }
        let path = tree
            .ancestors(id)
            .await?
            .into_iter()
            .map(|n| n.name)
            .collect();
        planned.push(Planned { node, path });
    }
    Ok((name, planned))
}

async fn environment_name(ctx: &AppContext, input: &RunnerInput) -> CoreResult<Option<String>> {
    let id = match input.environment_id {
        Some(id) => Some(id),
        None => {
            super::template::get_id(ctx, &super::template::active_key(input.collection_id)).await?
        }
    };
    let envs = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
    Ok(match id {
        Some(id) => envs.get(id).await.ok().map(|e| e.name),
        None => None,
    })
}

fn item_from(planned: &Planned, iteration: u32, sent: CoreResult<Value>) -> RunItem {
    let (method, url) = planned.node.data.method_and_url();
    let mut item = RunItem {
        iteration,
        request_id: planned.node.meta.id,
        name: planned.node.name.clone(),
        path: planned.path.clone(),
        method: method.unwrap_or("GET").to_string(),
        url: url.unwrap_or("").to_string(),
        status: None,
        total_ms: None,
        response_id: None,
        error: None,
        tests: Vec::new(),
        assertions: Vec::new(),
    };
    match sent {
        Ok(record) => {
            let snapshot = &record["requestSnapshot"];
            if let Some(m) = snapshot["method"].as_str() {
                item.method = m.to_string();
            }
            if let Some(u) = snapshot["url"].as_str() {
                item.url = u.to_string();
            }
            item.status = record["status"].as_u64().map(|s| s as u16);
            item.total_ms = record["timings"]["totalMs"].as_f64();
            item.response_id = serde_json::from_value(record["id"].clone()).ok();
            item.error = serde_json::from_value(record["error"].clone()).ok();
            item.tests = serde_json::from_value::<Vec<TestResult>>(record["tests"].clone())
                .unwrap_or_default();
            item.assertions =
                serde_json::from_value::<Vec<AssertionResult>>(record["assertions"].clone())
                    .unwrap_or_default();
        }
        Err(e) => item.error = Some(e),
    }
    item
}

/// Where `setNextRequest(name)` continues: a request of the run by name or id.
fn position_of(planned: &[Planned], target: &str) -> Option<usize> {
    planned
        .iter()
        .position(|p| p.node.name == target)
        .or_else(|| {
            planned
                .iter()
                .position(|p| p.node.meta.id.to_string() == target)
        })
}

pub async fn run(ctx: &AppContext, input: RunnerInput) -> CoreResult<RunReport> {
    let rows = match &input.data {
        Some(data) => parse_data(data.format, &data.text)?,
        None => Vec::new(),
    };
    let iterations = input.iterations.unwrap_or(rows.len().max(1) as u32).max(1);
    let (name, planned) = plan(ctx, &input).await?;
    let environment = environment_name(ctx, &input).await?;
    let owner = crate::access::current().user_id();

    let (cancel_tx, mut cancelled_rx) = watch::channel(false);
    let handle = Arc::new(RunHandle {
        owner,
        cancel: cancel_tx,
        current: Mutex::new(None),
    });
    {
        let mut runs = ctx
            .runs
            .0
            .lock()
            .map_err(|_| CoreError::internal("runs lock poisoned"))?;
        if runs.contains_key(&input.run_id) {
            return Err(
                CoreError::new("runner.duplicate", "this run is already running")
                    .with_param("runId", input.run_id.clone()),
            );
        }
        runs.insert(input.run_id.clone(), handle.clone());
    }
    let _registered = Registered {
        runs: &ctx.runs,
        id: input.run_id.clone(),
    };

    let delay = Duration::from_millis(input.delay_ms.unwrap_or(0));
    let stop_on_failure = input.stop_on_failure.unwrap_or(false);
    let total = planned.len() as u32 * iterations;
    let mut report = RunReport {
        run_id: input.run_id.clone(),
        name,
        environment,
        started_at: ctx.clock.now().0,
        finished_at: 0,
        iterations,
        items: Vec::new(),
        cancelled: false,
        stopped: false,
    };
    let is_cancelled = |rx: &watch::Receiver<bool>| *rx.borrow();

    'run: for iteration in 0..iterations {
        let data = rows
            .get(iteration as usize)
            .or(rows.last())
            .cloned()
            .unwrap_or_default();
        let mut index = 0;
        while let Some(request) = planned.get(index) {
            if !report.items.is_empty() && !delay.is_zero() {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancelled_rx.wait_for(|c| *c) => {}
                }
            }
            if is_cancelled(&cancelled_rx) {
                report.cancelled = true;
                break 'run;
            }
            let scope = RunScope {
                collection_id: input.collection_id,
                environment_id: input.environment_id,
                data: data.clone(),
                iteration,
                iteration_count: iterations,
                next_request: Arc::default(),
            };
            if let Ok(mut current) = handle.current.lock() {
                *current = Some(request.node.meta.id);
            }
            let sent = RUN
                .scope(
                    scope.clone(),
                    super::http::send_request(ctx, request.node.meta.id),
                )
                .await
                .map(|record| serde_json::to_value(record).unwrap_or_default());
            if let Ok(mut current) = handle.current.lock() {
                *current = None;
            }
            if is_cancelled(&cancelled_rx) || matches!(&sent, Err(e) if e.code == "http.cancelled")
            {
                report.cancelled = true;
                break 'run;
            }
            let item = item_from(request, iteration, sent);
            let failed = !item.passed();
            report.items.push(item.clone());
            ctx.events.publish(
                Event::new(
                    "runner.progress",
                    RunProgress {
                        run_id: input.run_id.clone(),
                        total,
                        done: report.items.len() as u32,
                        item,
                    },
                )
                .by(owner),
            );
            if failed && stop_on_failure {
                report.stopped = true;
                break 'run;
            }
            index = match scope.take_next_request() {
                None => index + 1,
                Some(None) => break,
                Some(Some(target)) => match position_of(&planned, &target) {
                    Some(i) => i,
                    None => break,
                },
            };
        }
    }

    report.finished_at = ctx.clock.now().0;
    ctx.events.publish(
        Event::new(
            "runner.finished",
            RunFinished {
                run_id: input.run_id.clone(),
                passed: report.passed(),
            },
        )
        .by(owner),
    );
    Ok(report)
}
