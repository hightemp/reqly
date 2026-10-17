//! Pre-request and after-response scripts around `request.send`: order
//! (collection → folders outer to inner → request), variable scopes and
//! their persistence, request edits and `sendRequest`.

use std::sync::Arc;

use reqly_core::model::{
    Body, BodyKind, EnvironmentKind, HttpRequest, KeyValue, KeyValueKind, NodeData, RequestSettings,
};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::{CancelToken, ExecOptions};
use reqly_script::{
    Limits, LogEntry, Pair, Phase, Scopes, ScriptBody, ScriptHost, ScriptInfo, ScriptInput,
    ScriptRequest, ScriptResponse, SendSpec, TestResult, VariableScope,
};
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::tree::TreeRepo;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::template::{active_global_key, active_key, changed, get_id, set_id};
use super::{AppContext, Empty};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<RuntimeVariablesList>()
        .register::<RuntimeVariablesClear>();
}

pub const SCRIPT_SETTINGS_KEY: &str = "scripts.settings";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ScriptSettings {
    /// Time limit per script in milliseconds (waits for requests excluded).
    pub timeout_ms: u64,
    pub memory_mb: u64,
}

impl Default for ScriptSettings {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            memory_mb: 64,
        }
    }
}

/// One script with where it comes from.
struct Source {
    label: String,
    text: String,
}

/// What a phase produced, kept with the response.
#[derive(Debug, Default)]
pub(crate) struct PhaseResult {
    pub logs: Vec<LogEntry>,
    pub tests: Vec<TestResult>,
    pub skip: bool,
}

impl PhaseResult {
    fn absorb(&mut self, out: reqly_script::ScriptOutput) {
        self.logs.extend(out.logs);
        self.tests.extend(out.tests);
        self.skip |= out.skip_request;
    }
}

async fn sources(ctx: &AppContext, request_id: Id, phase: Phase) -> CoreResult<Vec<Source>> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let collection = tree.get_collection(node.collection_id).await?;
    let pick = |s: &reqly_core::model::Scripts| match phase {
        Phase::Pre => s.pre_request.clone(),
        Phase::Post => s.after_response.clone(),
    };
    let suffix = match phase {
        Phase::Pre => "pre-request",
        Phase::Post => "after-response",
    };
    let mut out = vec![Source {
        label: format!("{} / {suffix}", collection.name),
        text: pick(&collection.scripts),
    }];
    for folder in tree.ancestors(request_id).await? {
        if let NodeData::Folder(f) = &folder.data {
            out.push(Source {
                label: format!("{} / {suffix}", folder.name),
                text: pick(&f.scripts),
            });
        }
    }
    if let NodeData::Request(r) = &node.data {
        out.push(Source {
            label: format!("{} / {suffix}", node.name),
            text: pick(&r.scripts),
        });
    }
    out.retain(|s| !s.text.trim().is_empty());
    Ok(out)
}

/// Whether a request has any script for `phase` (cheap check before work).
pub(crate) async fn has_scripts(
    ctx: &AppContext,
    request_id: Id,
    phase: Phase,
) -> CoreResult<bool> {
    Ok(!sources(ctx, request_id, phase).await?.is_empty())
}

// ----- conversions -----------------------------------------------------------------

fn pairs(rows: &[KeyValue]) -> Vec<Pair> {
    rows.iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| Pair {
            key: r.name.clone(),
            value: r.value.clone(),
            disabled: !r.enabled,
        })
        .collect()
}

/// Rows from pairs, keeping ids and descriptions of rows that did not change.
fn rows(pairs: &[Pair], old: &[KeyValue]) -> Vec<KeyValue> {
    pairs
        .iter()
        .map(|p| {
            match old
                .iter()
                .find(|o| o.name == p.key && o.value == p.value && o.enabled != p.disabled)
            {
                Some(o) => o.clone(),
                None => {
                    let mut row = KeyValue::new(p.key.clone(), p.value.clone());
                    row.enabled = !p.disabled;
                    row
                }
            }
        })
        .collect()
}

fn to_script(name: &str, id: Id, r: &HttpRequest) -> ScriptRequest {
    let mode = match r.body.kind {
        BodyKind::None => "none",
        BodyKind::FormUrlencoded => "urlencoded",
        BodyKind::Multipart => "formdata",
        BodyKind::Binary => "file",
        BodyKind::Graphql => "graphql",
        _ => "raw",
    };
    ScriptRequest {
        id: id.to_string(),
        name: name.to_string(),
        method: r.method.clone(),
        url: r.url.clone(),
        query: pairs(&r.params),
        headers: pairs(&r.headers),
        body: ScriptBody {
            mode: mode.into(),
            raw: if r.body.kind == BodyKind::Graphql {
                r.body.graphql.query.clone()
            } else {
                r.body.text.clone()
            },
            fields: pairs(&r.body.fields),
        },
        auth: serde_json::to_value(&r.auth).unwrap_or_default(),
    }
}

fn apply_request(r: &mut HttpRequest, s: &ScriptRequest) {
    r.method = s.method.clone();
    r.url = s.url.clone();
    r.params = rows(&s.query, &r.params);
    r.headers = rows(&s.headers, &r.headers);
    match s.body.mode.as_str() {
        "none" => r.body.kind = BodyKind::None,
        "graphql" => r.body.graphql.query = s.body.raw.clone(),
        "urlencoded" | "formdata" => {
            r.body.kind = if s.body.mode == "urlencoded" {
                BodyKind::FormUrlencoded
            } else {
                BodyKind::Multipart
            };
            let files: Vec<&KeyValue> = r
                .body
                .fields
                .iter()
                .filter(|f| f.kind == KeyValueKind::File)
                .collect();
            let mut fields = rows(&s.body.fields, &r.body.fields);
            // File fields keep their kind when unchanged.
            for f in &mut fields {
                if files.iter().any(|o| o.id == f.id) {
                    f.kind = KeyValueKind::File;
                }
            }
            r.body.fields = fields;
        }
        "raw" => {
            if !r.body.kind.is_text() {
                let looks_json = serde_json::from_str::<Value>(&s.body.raw)
                    .is_ok_and(|v| v.is_object() || v.is_array());
                r.body.kind = if looks_json {
                    BodyKind::Json
                } else {
                    BodyKind::Text
                };
            }
            r.body.text = s.body.raw.clone();
        }
        _ => {}
    }
}

// ----- scopes ---------------------------------------------------------------------------

/// Where `pm.environment` writes: the active environment, or the base one.
struct Targets {
    workspace_id: Id,
    globals: Option<Id>,
    base: Option<Id>,
    environment: Option<Id>,
}

async fn scopes(ctx: &AppContext, request_id: Id) -> CoreResult<(Scopes, Targets, Id)> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let collection = tree.get_collection(node.collection_id).await?;
    let envs = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
    let mut scopes = Scopes::default();
    let mut targets = Targets {
        workspace_id: collection.workspace_id,
        globals: None,
        base: None,
        environment: None,
    };
    if let Some(id) = get_id(ctx, &active_global_key(collection.workspace_id)).await?
        && let Ok(env) = envs.get(id).await
    {
        scopes.globals = env.data;
        targets.globals = Some(id);
    }
    let active = get_id(ctx, &active_key(collection.meta.id)).await?;
    for env in envs.list_for_collection(collection.meta.id).await? {
        match env.kind {
            EnvironmentKind::Base => {
                scopes.collection = env.data.clone();
                targets.base = Some(env.meta.id);
            }
            EnvironmentKind::Sub if Some(env.meta.id) == active => {
                scopes.environment = env.data.clone();
                targets.environment = Some(env.meta.id);
            }
            _ => {}
        }
    }
    if targets.environment.is_none() {
        scopes.environment = scopes.collection.clone();
        targets.environment = targets.base;
    }
    // Folder and request variables, then runtime ones.
    for n in tree
        .ancestors(request_id)
        .await?
        .iter()
        .chain(std::iter::once(&node))
    {
        let vars = match &n.data {
            NodeData::Folder(f) => &f.variables,
            NodeData::Request(r) => &r.variables,
            _ => continue,
        };
        scopes.local.extend(vars.clone());
    }
    scopes
        .local
        .extend(ctx.runtime_variables(collection.meta.id));
    Ok((scopes, targets, collection.meta.id))
}

async fn apply_changes(
    ctx: &AppContext,
    changes: &[reqly_script::VariableChange],
    targets: &Targets,
    collection_id: Id,
) -> CoreResult<()> {
    let envs = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
    // `pm.globals` always exists in Postman: use or create a global environment.
    let mut globals = targets.globals;
    if globals.is_none() && changes.iter().any(|c| c.scope == VariableScope::Globals) {
        let existing = envs.list_global(targets.workspace_id).await?;
        let env = match existing.into_iter().next() {
            Some(env) => env,
            None => {
                envs.create(
                    targets.workspace_id,
                    None,
                    EnvironmentKind::Global,
                    "Globals",
                )
                .await?
            }
        };
        set_id(
            ctx,
            &active_global_key(targets.workspace_id),
            Some(env.meta.id),
        )
        .await?;
        globals = Some(env.meta.id);
        changed(ctx, &env);
    }
    let mut by_env: Vec<(Id, Vec<&reqly_script::VariableChange>)> = Vec::new();
    for c in changes {
        let target = match c.scope {
            VariableScope::Globals => globals,
            VariableScope::Collection => targets.base,
            VariableScope::Environment => targets.environment,
            VariableScope::Data => None,
            VariableScope::Local => {
                if let Ok(mut all) = ctx.runtime_vars.lock() {
                    let map = all.entry(collection_id).or_default();
                    if c.unset {
                        map.remove(&c.key);
                    } else {
                        map.insert(c.key.clone(), c.value.clone());
                    }
                }
                ctx.events.publish(Event::new(
                    "variables.runtimeChanged",
                    RuntimeChanged { collection_id },
                ));
                continue;
            }
        };
        let Some(id) = target else { continue };
        match by_env.iter_mut().find(|(e, _)| *e == id) {
            Some((_, list)) => list.push(c),
            None => by_env.push((id, vec![c])),
        }
    }
    for (id, list) in by_env {
        let env = envs.get(id).await?;
        let mut data: Map<String, Value> = env.data.clone();
        for c in list {
            if c.unset {
                data.remove(&c.key);
            } else {
                data.insert(c.key.clone(), c.value.clone());
            }
        }
        let env = envs.update(id, None, None, Some(data), None).await?;
        changed(ctx, &env);
    }
    Ok(())
}

// ----- host ----------------------------------------------------------------------------

/// `sendRequest` from scripts: plain requests (no templates), the workspace's
/// TLS settings, the server's network policy.
struct Host {
    options: ExecOptions,
    settings: RequestSettings,
    policy: Option<reqly_http::policy::NetworkPolicy>,
}

impl ScriptHost for Host {
    async fn send(&self, spec: SendSpec) -> ScriptResponse {
        let fail = |e: String| ScriptResponse {
            error: Some(e),
            ..ScriptResponse::default()
        };
        let request = HttpRequest {
            method: spec.method,
            url: spec.url,
            headers: spec
                .headers
                .iter()
                .map(|(k, v)| KeyValue::new(k, v))
                .collect(),
            body: match spec.body {
                Some(text) => Body {
                    kind: BodyKind::Text,
                    text,
                    ..Body::default()
                },
                None => Body::default(),
            },
            ..HttpRequest::default()
        };
        let prepared = match reqly_http::prepare::prepare_with_base(&request, &[], &self.settings) {
            Ok(mut p) => {
                // `Content-Type: text/plain` is only a default for the text body.
                if spec
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                {
                    let mut seen = false;
                    p.headers.retain(|(k, _)| {
                        if !k.eq_ignore_ascii_case("content-type") {
                            return true;
                        }
                        let keep = !seen;
                        seen = true;
                        keep
                    });
                }
                p
            }
            Err(e) => return fail(e.message),
        };
        let started = std::time::Instant::now();
        let result = match &self.policy {
            Some(policy) => {
                reqly_http::policy::send_checked(
                    prepared,
                    self.options.clone(),
                    CancelToken::new(),
                    None,
                    policy,
                )
                .await
            }
            None => {
                reqly_http::engine::execute(
                    prepared,
                    self.options.clone(),
                    CancelToken::new(),
                    None,
                )
                .await
            }
        };
        match result {
            Ok(res) => ScriptResponse {
                code: res.status,
                status: res.status_text.clone(),
                headers: res.headers.clone(),
                body: res
                    .body
                    .to_vec()
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default(),
                time_ms: started.elapsed().as_secs_f64() * 1000.0,
                size: res.body_size,
                error: None,
            },
            Err(e) => fail(e.message),
        }
    }
}

async fn host(ctx: &AppContext, workspace_id: Id) -> CoreResult<Arc<Host>> {
    let settings: RequestSettings =
        super::http::setting(ctx, super::http::HTTP_DEFAULTS_KEY).await?;
    let options = super::http::exec_options(ctx, workspace_id, "").await?;
    Ok(Arc::new(Host {
        options,
        settings,
        policy: ctx.network_policy.clone(),
    }))
}

async fn limits(ctx: &AppContext) -> CoreResult<Limits> {
    let s: ScriptSettings = super::http::setting(ctx, SCRIPT_SETTINGS_KEY).await?;
    Ok(Limits {
        time: std::time::Duration::from_millis(s.timeout_ms.clamp(100, 600_000)),
        memory: (s.memory_mb.clamp(8, 1024) as usize) << 20,
        ..Limits::default()
    })
}

// ----- phases ---------------------------------------------------------------------------

/// Runs the pre-request scripts on `request` (unrendered). On a script error
/// the error carries the logs written so far in `params.logs`.
pub(crate) async fn run_pre(
    ctx: &AppContext,
    request_id: Id,
    request: &mut HttpRequest,
) -> CoreResult<PhaseResult> {
    run_phase(ctx, request_id, request, None).await
}

/// Runs the after-response scripts. Script errors become `error` log entries;
/// the response is kept either way.
pub(crate) async fn run_post(
    ctx: &AppContext,
    request_id: Id,
    request: &HttpRequest,
    response: ScriptResponse,
) -> PhaseResult {
    let mut request = request.clone();
    match run_phase(ctx, request_id, &mut request, Some(response)).await {
        Ok(r) => r,
        Err(e) => {
            let mut r = PhaseResult::default();
            if let Some(Value::Array(logs)) = e.params.get("logs") {
                r.logs.extend(
                    logs.iter()
                        .filter_map(|l| serde_json::from_value(l.clone()).ok()),
                );
            }
            r.logs.push(LogEntry {
                level: "error".into(),
                text: e.message.clone(),
                source: e
                    .params
                    .get("script")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
            r
        }
    }
}

async fn run_phase(
    ctx: &AppContext,
    request_id: Id,
    request: &mut HttpRequest,
    response: Option<ScriptResponse>,
) -> CoreResult<PhaseResult> {
    let phase = if response.is_some() {
        Phase::Post
    } else {
        Phase::Pre
    };
    let list = sources(ctx, request_id, phase).await?;
    let mut result = PhaseResult::default();
    if list.is_empty() {
        return Ok(result);
    }
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let collection = tree.get_collection(node.collection_id).await?;
    let host = host(ctx, collection.workspace_id).await?;
    let limits = limits(ctx).await?;
    let locale = super::template::faker_locale(ctx, node.collection_id).await;
    for source in list {
        // Each script sees what the previous ones changed.
        let (variables, targets, collection_id) = scopes(ctx, request_id).await?;
        let input = ScriptInput {
            phase,
            label: source.label.clone(),
            script: source.text,
            request: to_script(&node.name, request_id, request),
            response: response.clone(),
            variables,
            info: ScriptInfo {
                request_name: node.name.clone(),
                request_id: request_id.to_string(),
                iteration: 0,
                iteration_count: 1,
            },
            limits,
            locale,
        };
        match reqly_script::run(host.clone(), input).await {
            Ok(out) => {
                apply_changes(ctx, &out.changes, &targets, collection_id).await?;
                if phase == Phase::Pre {
                    apply_request(request, &out.request);
                }
                result.absorb(out);
            }
            Err(mut e) => {
                // Logs of the earlier scripts come first.
                let mut logs = std::mem::take(&mut result.logs);
                if let Some(Value::Array(more)) = e.params.get("logs") {
                    logs.extend(
                        more.iter()
                            .filter_map(|l| serde_json::from_value(l.clone()).ok()),
                    );
                }
                e.params.insert(
                    "logs".into(),
                    serde_json::to_value(&logs).unwrap_or_default(),
                );
                e.params.insert(
                    "tests".into(),
                    serde_json::to_value(&result.tests).unwrap_or_default(),
                );
                return Err(e);
            }
        }
    }
    Ok(result)
}

// ----- commands ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeChanged {
    pub collection_id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeVariablesInput {
    pub collection_id: Id,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeVariablesOutput {
    #[ts(type = "Record<string, unknown>")]
    pub variables: Map<String, Value>,
}

pub struct RuntimeVariablesList;

impl Command<AppContext> for RuntimeVariablesList {
    const NAME: &'static str = "variables.runtime";
    type Input = RuntimeVariablesInput;
    type Output = RuntimeVariablesOutput;

    async fn run(
        ctx: &AppContext,
        input: RuntimeVariablesInput,
    ) -> CoreResult<RuntimeVariablesOutput> {
        Ok(RuntimeVariablesOutput {
            variables: ctx.runtime_variables(input.collection_id),
        })
    }
}

pub struct RuntimeVariablesClear;

impl Command<AppContext> for RuntimeVariablesClear {
    const NAME: &'static str = "variables.clearRuntime";
    type Input = RuntimeVariablesInput;
    type Output = Empty;

    async fn run(ctx: &AppContext, input: RuntimeVariablesInput) -> CoreResult<Empty> {
        ctx.runtime_vars
            .lock()
            .map_err(|_| CoreError::internal("runtime variables lock"))?
            .remove(&input.collection_id);
        ctx.events.publish(Event::new(
            "variables.runtimeChanged",
            RuntimeChanged {
                collection_id: input.collection_id,
            },
        ));
        Ok(Empty {})
    }
}
