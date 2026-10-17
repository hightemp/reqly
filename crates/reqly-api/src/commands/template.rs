//! Environments, variable resolution and template rendering for sends.
//!
//! Precedence (later wins): global < base < active sub-environment <
//! folders (outer → inner) < request.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use reqly_core::model::{
    Auth, Environment, EnvironmentKind, FolderData, HttpRequest, KeyValue, KeyValueKind, Node,
    NodeData,
};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_storage::cookies::CookieRepo;
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::history::HistoryRepo;
use reqly_storage::settings::SettingsRepo;
use reqly_storage::tree::TreeRepo;
use reqly_template::ast::{Expr, Segment};
use reqly_template::{FunctionInfo, Host, Locale, RenderOptions, Renderer, value_to_string};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use ts_rs::TS;

use super::tree::IdInput;
use super::{AppContext, Empty, Platform};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<EnvironmentList>()
        .register::<EnvironmentCreate>()
        .register::<EnvironmentUpdate>()
        .register::<EnvironmentDuplicate>()
        .register::<EnvironmentDelete>()
        .register::<EnvironmentSetActive>()
        .register::<EnvironmentSetActiveGlobal>()
        .register::<TemplatePreview>()
        .register::<TemplateVariables>()
        .register::<TemplateFunctions>()
        .register::<TemplatePrompts>();
}

/// Settings key holding the active sub-environment of a collection.
pub fn active_key(collection_id: Id) -> String {
    format!("env.active.{collection_id}")
}

/// Settings key holding the active global environment of a workspace.
pub fn active_global_key(workspace_id: Id) -> String {
    format!("env.global.{workspace_id}")
}

/// Faker locale for every collection (`"en"` / `"ru"`).
pub const FAKER_LOCALE_KEY: &str = "template.fakerLocale";

/// Per-collection override of [`FAKER_LOCALE_KEY`].
pub fn faker_locale_key(collection_id: Id) -> String {
    format!("{FAKER_LOCALE_KEY}.{collection_id}")
}

/// Maximum depth of requests sent because of `response` tags.
const MAX_CHAIN: usize = 8;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentsChanged {
    pub workspace_id: Id,
    pub collection_id: Option<Id>,
}

fn changed(ctx: &AppContext, env: &Environment) {
    ctx.events.publish(Event::new(
        "environments.changed",
        EnvironmentsChanged {
            workspace_id: env.workspace_id,
            collection_id: env.collection_id,
        },
    ));
}

async fn get_id(ctx: &AppContext, key: &str) -> CoreResult<Option<Id>> {
    Ok(SettingsRepo::new(&ctx.storage)
        .get(key)
        .await?
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok())))
}

async fn set_id(ctx: &AppContext, key: &str, id: Option<Id>) -> CoreResult<()> {
    let repo = SettingsRepo::new(&ctx.storage);
    match id {
        Some(id) => repo.set(key, &json!(id), ctx.clock.as_ref()).await,
        None => repo.delete(key).await,
    }
}

// ----- variable context -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VariableSource {
    Global,
    Base,
    Sub,
    Folder,
    Request,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VariableInfo {
    pub name: String,
    #[ts(type = "unknown")]
    pub value: Value,
    pub source: VariableSource,
    /// Name of the environment / folder / request that defines it.
    pub source_name: String,
    pub secret: bool,
}

/// Source, source name, variables and secret keys of one precedence level.
type Layer = (VariableSource, String, Map<String, Value>, Vec<String>);

/// Everything needed to render templates in the context of a node.
pub struct Scope {
    pub workspace_id: Id,
    pub collection_id: Id,
    /// The request or folder being rendered, with its ancestors (outermost first).
    pub node: Option<Node>,
    pub ancestors: Vec<Node>,
    pub vars: Value,
    pub variables: Vec<VariableInfo>,
}

impl Scope {
    pub async fn load(
        ctx: &AppContext,
        collection_id: Id,
        node_id: Option<Id>,
    ) -> CoreResult<Self> {
        let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
        let collection = tree.get_collection(collection_id).await?;
        let workspace_id = collection.workspace_id;
        let (node, ancestors) = match node_id {
            Some(id) => (Some(tree.get_node(id).await?), tree.ancestors(id).await?),
            None => (None, Vec::new()),
        };

        let envs = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut layers: Vec<Layer> = Vec::new();
        if let Some(id) = get_id(ctx, &active_global_key(workspace_id)).await?
            && let Ok(env) = envs.get(id).await
        {
            layers.push((VariableSource::Global, env.name, env.data, env.secrets));
        }
        let collection_envs = envs.list_for_collection(collection_id).await?;
        let active = get_id(ctx, &active_key(collection_id)).await?;
        for env in collection_envs {
            match env.kind {
                EnvironmentKind::Base => {
                    layers.push((VariableSource::Base, env.name, env.data, env.secrets))
                }
                EnvironmentKind::Sub if Some(env.meta.id) == active => {
                    layers.push((VariableSource::Sub, env.name, env.data, env.secrets))
                }
                _ => {}
            }
        }
        let own = node.iter();
        for n in ancestors.iter().chain(own) {
            match &n.data {
                NodeData::Folder(f) => layers.push((
                    VariableSource::Folder,
                    n.name.clone(),
                    f.variables.clone(),
                    Vec::new(),
                )),
                NodeData::Request(r) => layers.push((
                    VariableSource::Request,
                    n.name.clone(),
                    r.variables.clone(),
                    Vec::new(),
                )),
            }
        }

        let mut vars = Map::new();
        let mut infos: Vec<VariableInfo> = Vec::new();
        for (source, source_name, data, secrets) in layers {
            for (name, value) in data {
                infos.retain(|i| i.name != name);
                infos.push(VariableInfo {
                    secret: secrets.contains(&name),
                    name: name.clone(),
                    value: value.clone(),
                    source,
                    source_name: source_name.clone(),
                });
                vars.insert(name, value);
            }
        }
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            workspace_id,
            collection_id,
            node,
            ancestors,
            vars: Value::Object(vars),
            variables: infos,
        })
    }
}

// ----- host functions --------------------------------------------------------------

pub struct SendHost<'a> {
    pub ctx: &'a AppContext,
    pub scope: &'a Scope,
    pub prompt_values: &'a HashMap<String, String>,
    /// Requests currently being sent in this chain (for cycle detection).
    pub chain: &'a [Id],
}

fn not_allowed(name: &str) -> CoreError {
    CoreError::new(
        "template.not_allowed",
        format!("{name} is not available on the server"),
    )
    .with_param("function", name)
}

fn arg(args: &[Value], i: usize) -> String {
    args.get(i).map(value_to_string).unwrap_or_default()
}

fn cookie_matches(c: &reqly_storage::cookies::Cookie, url: &url::Url) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let domain = c.domain.to_ascii_lowercase();
    let domain_ok =
        host == domain || (c.include_subdomains && host.ends_with(&format!(".{domain}")));
    let path_ok = url.path().starts_with(&c.path);
    domain_ok && path_ok && (!c.secure || url.scheme() == "https")
}

impl SendHost<'_> {
    async fn cookie(&self, args: &[Value]) -> CoreResult<Value> {
        let url = url::Url::parse(&arg(args, 0)).map_err(|e| {
            CoreError::new("template.bad_argument", e.to_string()).with_param("function", "cookie")
        })?;
        let name = arg(args, 1);
        let cookies = CookieRepo::new(&self.ctx.storage, self.ctx.clock.as_ref())
            .list(self.scope.workspace_id)
            .await?;
        let mut matching: Vec<_> = cookies
            .into_iter()
            .filter(|c| c.name == name && cookie_matches(c, &url))
            .collect();
        // The most specific path wins.
        matching.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        matching
            .into_iter()
            .next()
            .map(|c| Value::String(c.value))
            .ok_or_else(|| {
                CoreError::new("template.cookie_missing", format!("no cookie {name}"))
                    .with_param("name", name)
            })
    }

    fn request(&self, args: &[Value]) -> CoreResult<Value> {
        let node = self.scope.node.as_ref();
        let http = node.and_then(|n| match &n.data {
            NodeData::Request(r) => Some(r),
            NodeData::Folder(_) => None,
        });
        let field = arg(args, 0);
        Ok(Value::String(match field.as_str() {
            "name" => node.map(|n| n.name.clone()).unwrap_or_default(),
            "folder" => self
                .scope
                .ancestors
                .last()
                .map(|n| n.name.clone())
                .unwrap_or_default(),
            "id" => node.map(|n| n.meta.id.to_string()).unwrap_or_default(),
            "method" => http.map(|r| r.method.to_uppercase()).unwrap_or_default(),
            "url" => http.map(|r| r.url.clone()).unwrap_or_default(),
            "header" => {
                let name = arg(args, 1);
                http.and_then(|r| {
                    r.headers
                        .iter()
                        .find(|h| h.enabled && h.name.eq_ignore_ascii_case(&name))
                        .map(|h| h.value.clone())
                })
                .unwrap_or_default()
            }
            other => {
                return Err(CoreError::new(
                    "template.bad_argument",
                    format!("unknown field {other}"),
                )
                .with_param("function", "request"));
            }
        }))
    }

    fn prompt(&self, args: &[Value]) -> CoreResult<Value> {
        let title = arg(args, 0);
        let key = Some(arg(args, 3))
            .filter(|k| !k.is_empty())
            .unwrap_or(title.clone());
        self.prompt_values
            .get(&key)
            .map(|v| Value::String(v.clone()))
            .ok_or_else(|| {
                CoreError::new(
                    "template.prompt_required",
                    format!("a value for {title} is required"),
                )
                .with_param("key", key)
                .with_param("title", title)
            })
    }

    /// Desktop: a local path. Server: an uploaded blob (by its hash).
    async fn file(&self, args: &[Value]) -> CoreResult<Value> {
        let path = arg(args, 0);
        let is_blob = path.len() == 64 && path.bytes().all(|b| b.is_ascii_hexdigit());
        if is_blob && self.ctx.blobs.exists(&path) {
            let bytes = self.ctx.blobs.read(&path).await?;
            return Ok(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        }
        if self.ctx.platform == Platform::Server {
            return Err(not_allowed("file"));
        }
        tokio::fs::read_to_string(&path)
            .await
            .map(Value::String)
            .map_err(|e| CoreError::new("template.file", e.to_string()).with_param("path", path))
    }

    fn os(&self, args: &[Value]) -> CoreResult<Value> {
        let field = arg(args, 0);
        Ok(Value::String(match field.as_str() {
            "platform" => std::env::consts::OS.to_string(),
            "arch" => std::env::consts::ARCH.to_string(),
            "hostname" | "username" | "env" if self.ctx.platform == Platform::Server => {
                return Err(not_allowed("os"));
            }
            "hostname" => hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_default(),
            "username" => std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_default(),
            "env" => std::env::var(arg(args, 1)).unwrap_or_default(),
            other => {
                return Err(CoreError::new(
                    "template.bad_argument",
                    format!("unknown field {other}"),
                )
                .with_param("function", "os"));
            }
        }))
    }

    /// `response(field, requestId, path?, trigger?, maxAgeSeconds?)`.
    async fn response(&self, args: &[Value]) -> CoreResult<Value> {
        let field = arg(args, 0);
        let request_id: Id = arg(args, 1).parse().map_err(|_| {
            CoreError::new("template.bad_argument", "invalid request id")
                .with_param("function", "response")
        })?;
        let path = arg(args, 2);
        let trigger = Some(arg(args, 3))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "never".into());
        let max_age = args.get(4).and_then(|v| match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        });

        let history = HistoryRepo::new(&self.ctx.storage, self.ctx.clock.as_ref());
        let latest = history
            .list_for_request(request_id, 1)
            .await?
            .into_iter()
            .next();
        let expired = latest.as_ref().is_some_and(|r| {
            max_age
                .is_some_and(|age| (self.ctx.clock.now().0 - r.created_at.0) as f64 > age * 1000.0)
        });
        let send = match trigger.as_str() {
            "always" => true,
            "no-history" => latest.is_none(),
            "when-expired" => latest.is_none() || expired,
            _ => false,
        };
        let record = if send {
            if self.chain.contains(&request_id) || self.chain.len() >= MAX_CHAIN {
                return Err(CoreError::new(
                    "template.response_cycle",
                    "requests depend on each other's responses",
                )
                .with_param("requestId", request_id.to_string()));
            }
            let mut chain = self.chain.to_vec();
            chain.push(request_id);
            send_chained(self.ctx, request_id, self.prompt_values.clone(), chain).await?
        } else {
            match latest {
                Some(s) => history.get(s.id).await?,
                None => {
                    return Err(CoreError::new(
                        "template.response_missing",
                        "the referenced request has no response yet",
                    )
                    .with_param("requestId", request_id.to_string()));
                }
            }
        };

        let body = || async {
            match &record.body_hash {
                Some(hash) => Ok::<_, CoreError>(
                    String::from_utf8_lossy(&self.ctx.blobs.read(hash).await?).into_owned(),
                ),
                None => Ok(String::new()),
            }
        };
        Ok(match field.as_str() {
            "status" => json!(record.status),
            "url" => Value::String(record.final_url.clone()),
            "raw" => Value::String(body().await?),
            "header" => record
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&path))
                .map(|(_, v)| Value::String(v.clone()))
                .ok_or_else(|| {
                    CoreError::new("template.response_header", format!("no header {path}"))
                        .with_param("name", path.clone())
                })?,
            "body" => {
                let text = body().await?;
                if path.is_empty() {
                    Value::String(text)
                } else if path.starts_with('$') {
                    reqly_template::functions::call(
                        "jsonpath",
                        &[Value::String(text), Value::String(path)],
                        Locale::En,
                    )
                    .expect("built-in")?
                } else {
                    Value::String(
                        reqly_http::filter::apply_filter(
                            reqly_http::filter::FilterKind::XPath,
                            &text,
                            &path,
                        )?
                        .trim()
                        .to_string(),
                    )
                }
            }
            other => {
                return Err(CoreError::new(
                    "template.bad_argument",
                    format!("unknown field {other}"),
                )
                .with_param("function", "response"));
            }
        })
    }
}

#[async_trait::async_trait]
impl Host for SendHost<'_> {
    async fn call(&self, name: &str, args: &[Value]) -> Option<CoreResult<Value>> {
        Some(match name {
            "cookie" => self.cookie(args).await,
            "request" => self.request(args),
            "prompt" => self.prompt(args),
            "file" => self.file(args).await,
            "os" => self.os(args),
            "response" => self.response(args).await,
            _ => return None,
        })
    }
}

fn send_chained(
    ctx: &AppContext,
    request_id: Id,
    prompts: HashMap<String, String>,
    chain: Vec<Id>,
) -> Pin<Box<dyn Future<Output = CoreResult<reqly_core::model::ResponseRecord>> + Send + '_>> {
    Box::pin(async move { super::http::send_request_with(ctx, request_id, &prompts, &chain).await })
}

/// The collection's faker locale, else the global one, else English.
pub async fn faker_locale(ctx: &AppContext, collection_id: Id) -> Locale {
    let repo = SettingsRepo::new(&ctx.storage);
    for key in [
        faker_locale_key(collection_id),
        FAKER_LOCALE_KEY.to_string(),
    ] {
        if let Ok(Some(v)) = repo.get(&key).await
            && let Ok(locale) = serde_json::from_value(v)
        {
            return locale;
        }
    }
    Locale::default()
}

// ----- rendering stored models ---------------------------------------------------

async fn render_str(r: &Renderer<'_>, s: &mut String) -> CoreResult<()> {
    if reqly_template::has_template(s) {
        *s = r.render(s).await?;
    }
    Ok(())
}

async fn render_rows(r: &Renderer<'_>, rows: &mut [KeyValue]) -> CoreResult<()> {
    for row in rows.iter_mut().filter(|row| row.enabled) {
        render_str(r, &mut row.name).await?;
        if row.kind == KeyValueKind::Text {
            render_str(r, &mut row.value).await?;
        }
    }
    Ok(())
}

async fn render_auth(r: &Renderer<'_>, auth: &mut Auth) -> CoreResult<()> {
    match auth {
        Auth::Basic { username, password } => {
            render_str(r, username).await?;
            render_str(r, password).await
        }
        Auth::Bearer { token, prefix } => {
            render_str(r, token).await?;
            render_str(r, prefix).await
        }
        Auth::ApiKey { key, value, .. } => {
            render_str(r, key).await?;
            render_str(r, value).await
        }
        Auth::Inherit | Auth::None => Ok(()),
    }
}

pub async fn render_request(r: &Renderer<'_>, req: &mut HttpRequest) -> CoreResult<()> {
    render_str(r, &mut req.url).await?;
    render_rows(r, &mut req.params).await?;
    render_rows(r, &mut req.path_params).await?;
    render_rows(r, &mut req.headers).await?;
    render_str(r, &mut req.body.text).await?;
    render_rows(r, &mut req.body.fields).await?;
    render_auth(r, &mut req.auth).await
}

pub async fn render_folder(r: &Renderer<'_>, folder: &mut FolderData) -> CoreResult<()> {
    render_rows(r, &mut folder.headers).await?;
    render_auth(r, &mut folder.auth).await
}

/// Prompt tags (`{% prompt … %}`) in the given strings, deduplicated by key.
fn collect_prompts(texts: &[&str]) -> Vec<PromptInfo> {
    fn walk(expr: &Expr, out: &mut Vec<PromptInfo>) {
        if let Expr::Call { name, args } = expr {
            if name == "prompt" {
                let lit = |i: usize| match args.get(i) {
                    Some(Expr::Lit(v)) => value_to_string(v),
                    _ => String::new(),
                };
                let title = lit(0);
                let key = Some(lit(3))
                    .filter(|k| !k.is_empty())
                    .unwrap_or(title.clone());
                if !out.iter().any(|p| p.key == key) {
                    out.push(PromptInfo {
                        key,
                        title,
                        label: lit(1),
                        default_value: lit(2),
                    });
                }
            }
            args.iter().for_each(|a| walk(a, out));
        }
    }
    let mut out = Vec::new();
    for text in texts {
        if let Ok(segments) = reqly_template::parse(text) {
            for s in segments {
                if let Segment::Expr { expr, .. } = s {
                    walk(&expr, &mut out);
                }
            }
        }
    }
    out
}

fn rows_text(rows: &[KeyValue]) -> impl Iterator<Item = &str> {
    rows.iter()
        .filter(|r| r.enabled)
        .flat_map(|r| [r.name.as_str(), r.value.as_str()])
}

fn auth_text(auth: &Auth) -> Vec<&str> {
    match auth {
        Auth::Basic { username, password } => vec![username, password],
        Auth::Bearer { token, prefix } => vec![token, prefix],
        Auth::ApiKey { key, value, .. } => vec![key, value],
        Auth::Inherit | Auth::None => vec![],
    }
}

// ----- commands ------------------------------------------------------------------

macro_rules! command {
    ($ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionIdInput {
    pub collection_id: Id,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentListOutput {
    /// Base first, then sub-environments.
    pub collection: Vec<Environment>,
    pub global: Vec<Environment>,
    pub active_id: Option<Id>,
    pub active_global_id: Option<Id>,
}

command!(
    EnvironmentList,
    "environment.list",
    CollectionIdInput,
    EnvironmentListOutput,
    |ctx, input| {
        let repo = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
        let collection = repo.list_for_collection(input.collection_id).await?;
        let workspace_id = TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
            .get_collection(input.collection_id)
            .await?
            .workspace_id;
        let global = repo.list_global(workspace_id).await?;
        let active_id = get_id(ctx, &active_key(input.collection_id))
            .await?
            .filter(|id| collection.iter().any(|e| e.meta.id == *id));
        let active_global_id = get_id(ctx, &active_global_key(workspace_id))
            .await?
            .filter(|id| global.iter().any(|e| e.meta.id == *id));
        Ok(EnvironmentListOutput {
            collection,
            global,
            active_id,
            active_global_id,
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentCreateInput {
    pub workspace_id: Id,
    pub collection_id: Option<Id>,
    pub kind: EnvironmentKind,
    pub name: String,
    #[ts(optional, type = "Record<string, unknown>")]
    pub data: Option<Map<String, Value>>,
}

command!(
    EnvironmentCreate,
    "environment.create",
    EnvironmentCreateInput,
    Environment,
    |ctx, input| {
        let repo = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut env = repo
            .create(
                input.workspace_id,
                input.collection_id,
                input.kind,
                &input.name,
            )
            .await?;
        if let Some(data) = input.data {
            env = repo
                .update(env.meta.id, None, None, Some(data), None)
                .await?;
        }
        changed(ctx, &env);
        Ok(env)
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentUpdateInput {
    pub id: Id,
    #[ts(optional)]
    pub name: Option<String>,
    /// An empty string clears the color.
    #[ts(optional)]
    pub color: Option<String>,
    #[ts(optional, type = "Record<string, unknown>")]
    pub data: Option<Map<String, Value>>,
    #[ts(optional)]
    pub secrets: Option<Vec<String>>,
}

command!(
    EnvironmentUpdate,
    "environment.update",
    EnvironmentUpdateInput,
    Environment,
    |ctx, input| {
        let env = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref())
            .update(
                input.id,
                input.name.as_deref(),
                input
                    .color
                    .as_deref()
                    .map(|c| Some(c).filter(|c| !c.is_empty())),
                input.data,
                input.secrets,
            )
            .await?;
        changed(ctx, &env);
        Ok(env)
    }
);

command!(
    EnvironmentDuplicate,
    "environment.duplicate",
    IdInput,
    Environment,
    |ctx, input| {
        let env = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref())
            .duplicate(input.id)
            .await?;
        changed(ctx, &env);
        Ok(env)
    }
);

command!(
    EnvironmentDelete,
    "environment.delete",
    IdInput,
    Empty,
    |ctx, input| {
        let repo = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
        let env = repo.get(input.id).await?;
        repo.delete(input.id).await?;
        let key = match env.collection_id {
            Some(col) => active_key(col),
            None => active_global_key(env.workspace_id),
        };
        if get_id(ctx, &key).await? == Some(env.meta.id) {
            set_id(ctx, &key, None).await?;
        }
        changed(ctx, &env);
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentSetActiveInput {
    pub collection_id: Id,
    pub environment_id: Option<Id>,
}

command!(
    EnvironmentSetActive,
    "environment.setActive",
    EnvironmentSetActiveInput,
    Empty,
    |ctx, input| {
        let workspace_id = TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
            .get_collection(input.collection_id)
            .await?
            .workspace_id;
        set_id(ctx, &active_key(input.collection_id), input.environment_id).await?;
        ctx.events.publish(Event::new(
            "environments.changed",
            EnvironmentsChanged {
                workspace_id,
                collection_id: Some(input.collection_id),
            },
        ));
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentSetActiveGlobalInput {
    pub workspace_id: Id,
    pub environment_id: Option<Id>,
}

command!(
    EnvironmentSetActiveGlobal,
    "environment.setActiveGlobal",
    EnvironmentSetActiveGlobalInput,
    Empty,
    |ctx, input| {
        set_id(
            ctx,
            &active_global_key(input.workspace_id),
            input.environment_id,
        )
        .await?;
        ctx.events.publish(Event::new(
            "environments.changed",
            EnvironmentsChanged {
                workspace_id: input.workspace_id,
                collection_id: None,
            },
        ));
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplatePreviewInput {
    pub collection_id: Id,
    /// Request or folder whose variables apply.
    #[ts(optional)]
    pub node_id: Option<Id>,
    pub text: String,
    #[ts(optional)]
    pub prompt_values: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplatePreviewOutput {
    pub value: String,
}

command!(
    TemplatePreview,
    "template.preview",
    TemplatePreviewInput,
    TemplatePreviewOutput,
    |ctx, input| {
        let scope = Scope::load(ctx, input.collection_id, input.node_id).await?;
        let prompts = input.prompt_values.unwrap_or_default();
        let host = SendHost {
            ctx,
            scope: &scope,
            prompt_values: &prompts,
            chain: &[],
        };
        let options = RenderOptions {
            locale: faker_locale(ctx, input.collection_id).await,
            ..RenderOptions::default()
        };
        let value = Renderer::new(&scope.vars, &host, options)
            .render(&input.text)
            .await?;
        Ok(TemplatePreviewOutput { value })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateVariablesInput {
    pub collection_id: Id,
    #[ts(optional)]
    pub node_id: Option<Id>,
}

command!(
    TemplateVariables,
    "template.variables",
    TemplateVariablesInput,
    Vec<VariableInfo>,
    |ctx, input| {
        Ok(Scope::load(ctx, input.collection_id, input.node_id)
            .await?
            .variables)
    }
);

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FakerInfo {
    pub name: String,
    /// Argument hint, e.g. `min?, max?`.
    pub args: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateFunctionsOutput {
    pub functions: Vec<FunctionInfo>,
    pub faker: Vec<FakerInfo>,
}

command!(
    TemplateFunctions,
    "template.functions",
    Empty,
    TemplateFunctionsOutput,
    |_ctx, _input| {
        Ok(TemplateFunctionsOutput {
            functions: reqly_template::catalog(),
            faker: reqly_template::FAKER_FUNCTIONS
                .iter()
                .map(|(name, args)| FakerInfo {
                    name: name.to_string(),
                    args: args.to_string(),
                })
                .collect(),
        })
    }
);

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PromptInfo {
    /// Key to send the value under in `promptValues`.
    pub key: String,
    pub title: String,
    pub label: String,
    pub default_value: String,
}

/// Prompts used by a request, its folders and the variables in scope.
pub async fn prompts_for(ctx: &AppContext, request_id: Id) -> CoreResult<Vec<PromptInfo>> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let scope = Scope::load(ctx, node.collection_id, Some(request_id)).await?;
    let mut texts: Vec<&str> = Vec::new();
    if let NodeData::Request(r) = &node.data {
        texts.push(&r.url);
        texts.extend(rows_text(&r.params));
        texts.extend(rows_text(&r.path_params));
        texts.extend(rows_text(&r.headers));
        texts.push(&r.body.text);
        texts.extend(rows_text(&r.body.fields));
        texts.extend(auth_text(&r.auth));
    }
    for n in &scope.ancestors {
        if let NodeData::Folder(f) = &n.data {
            texts.extend(rows_text(&f.headers));
            texts.extend(auth_text(&f.auth));
        }
    }
    let var_texts: Vec<String> = scope
        .variables
        .iter()
        .filter_map(|v| v.value.as_str().map(str::to_string))
        .collect();
    texts.extend(var_texts.iter().map(String::as_str));
    Ok(collect_prompts(&texts))
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplatePromptsInput {
    pub request_id: Id,
}

command!(
    TemplatePrompts,
    "template.prompts",
    TemplatePromptsInput,
    Vec<PromptInfo>,
    |ctx, input| { prompts_for(ctx, input.request_id).await }
);
