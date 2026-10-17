//! Sending requests, response history, cookies and file uploads.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqly_core::model::{
    FolderData, NodeData, NodeKind, RequestSettings, ResponseRecord, ResponseSummary,
};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::{CancelToken, ClientIdentity, ExecOptions, HttpResponse, ResponseBody};
use reqly_http::prepare::{PreparedBody, PreparedRequest, prepare_with_base};
use reqly_http::snapshot::RequestSnapshot;
use reqly_storage::cookies::{Cookie, CookieRepo};
use reqly_storage::history::{HistoryLimits, HistoryRepo, NewResponse};
use reqly_storage::settings::SettingsRepo;
use reqly_storage::tree::TreeRepo;
use reqly_template::{RenderOptions, Renderer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::template::{self, Scope, SendHost};
use super::tree::{IdInput, IdsInput, WorkspaceIdInput};
use super::{AppContext, Empty};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<RequestSend>()
        .register::<RequestCancel>()
        .register::<ResponseList>()
        .register::<ResponseGet>()
        .register::<ResponseBodyChunk>()
        .register::<ResponseDelete>()
        .register::<HistoryClear>()
        .register::<HistoryWorkspace>()
        .register::<CookiesList>()
        .register::<CookiesUpsert>()
        .register::<CookiesDelete>()
        .register::<CookiesClear>()
        .register::<BlobUpload>()
        .register::<ResponseSaveBody>()
        .register::<ResponseFilter>()
        .register::<MaintenanceCleanup>()
        .register::<CurlParse>()
        .register::<RequestToCurl>();
}

// ----- settings stored by the UI --------------------------------------------

pub const HTTP_DEFAULTS_KEY: &str = "http.defaults";
pub const HTTP_TLS_KEY: &str = "http.tls";
pub const HISTORY_LIMITS_KEY: &str = "history.limits";

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ClientCertificate {
    pub enabled: bool,
    /// `host`, `host:port` or `*.domain` (any port when omitted).
    pub host: String,
    pub cert_pem: String,
    pub key_pem: String,
    /// PKCS#12 bundle (base64) instead of PEM.
    pub pfx_base64: String,
    pub passphrase: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TlsSettings {
    /// Extra trusted CA certificates (PEM).
    pub ca_pems: Vec<String>,
    pub client_certificates: Vec<ClientCertificate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct HistoryLimitSettings {
    pub per_request: u32,
    pub total_megabytes: u64,
    /// `None` keeps records forever.
    pub max_age_days: Option<u32>,
}

impl Default for HistoryLimitSettings {
    fn default() -> Self {
        let d = HistoryLimits::default();
        Self {
            per_request: d.per_request,
            total_megabytes: d.total_bytes / (1024 * 1024),
            max_age_days: None,
        }
    }
}

impl From<HistoryLimitSettings> for HistoryLimits {
    fn from(s: HistoryLimitSettings) -> Self {
        Self {
            per_request: s.per_request.max(1),
            total_bytes: s.total_megabytes.saturating_mul(1024 * 1024),
            max_age_ms: s.max_age_days.map(|d| i64::from(d) * 86_400_000),
        }
    }
}

async fn setting<T: serde::de::DeserializeOwned + Default>(
    ctx: &AppContext,
    key: &str,
) -> CoreResult<T> {
    Ok(SettingsRepo::new(&ctx.storage)
        .get(key)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

// ----- events ----------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RequestStarted {
    pub request_id: Id,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RequestProgress {
    pub request_id: Id,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RequestFinished {
    pub request_id: Id,
    /// `None` when cancelled.
    pub response_id: Option<Id>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CookiesChanged {
    pub workspace_id: Id,
}

// ----- sending ---------------------------------------------------------------

fn host_matches(pattern: &str, url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    let port = u.port_or_known_default();
    let pattern = pattern.trim().to_ascii_lowercase();
    let (p_host, p_port) = match pattern.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse::<u16>().ok())
        }
        _ => (pattern.clone(), None),
    };
    let host_ok = match p_host.strip_prefix("*.") {
        Some(suffix) => host.ends_with(&format!(".{suffix}")) || host == suffix,
        None => host == p_host,
    };
    host_ok && p_port.is_none_or(|p| Some(p) == port)
}

fn client_identity(tls: &TlsSettings, url: &str) -> CoreResult<Option<ClientIdentity>> {
    let Some(cert) = tls
        .client_certificates
        .iter()
        .find(|c| c.enabled && host_matches(&c.host, url))
    else {
        return Ok(None);
    };
    if !cert.pfx_base64.is_empty() {
        let der = STANDARD
            .decode(cert.pfx_base64.trim())
            .map_err(|e| CoreError::new("http.client_cert", e.to_string()))?;
        return Ok(Some(ClientIdentity::Pkcs12 {
            der,
            passphrase: cert.passphrase.clone(),
        }));
    }
    Ok(Some(ClientIdentity::Pem {
        cert: cert.cert_pem.clone(),
        key: cert.key_pem.clone(),
    }))
}

/// Replaces blob references with paths in the blob store.
fn resolve_blobs(ctx: &AppContext, request: &mut PreparedRequest) -> CoreResult<()> {
    match &mut request.body {
        PreparedBody::Blob(hash) => {
            request.body = PreparedBody::File(ctx.blobs.path_of(hash)?);
        }
        PreparedBody::Multipart(parts) => {
            for part in parts {
                if let Some(hash) = part.blob.take() {
                    part.file = Some(ctx.blobs.path_of(&hash)?);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

struct InflightGuard<'a> {
    ctx: &'a AppContext,
    request_id: Id,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut map) = self.ctx.inflight.lock() {
            map.remove(&self.request_id);
        }
    }
}

async fn store_body(ctx: &AppContext, body: ResponseBody) -> CoreResult<String> {
    let blob = match body {
        ResponseBody::Memory(bytes) => ctx.blobs.put_bytes(&bytes).await?,
        ResponseBody::File(file) => {
            let reader = tokio::fs::File::open(file.path())
                .await
                .map_err(|e| CoreError::new("http.io", e.to_string()))?;
            ctx.blobs.put_reader(reader).await?
        }
    };
    Ok(blob.hash)
}

/// After history changes: apply limits, then drop unreferenced blobs.
async fn housekeeping(ctx: &AppContext) -> CoreResult<()> {
    let limits: HistoryLimitSettings = setting(ctx, HISTORY_LIMITS_KEY).await?;
    let history = HistoryRepo::new(&ctx.storage, ctx.clock.as_ref());
    if history.enforce_limits(&limits.into()).await? > 0 {
        collect_garbage(ctx).await?;
    }
    Ok(())
}

pub async fn collect_garbage(ctx: &AppContext) -> CoreResult<usize> {
    let mut live = HistoryRepo::new(&ctx.storage, ctx.clock.as_ref())
        .live_body_hashes()
        .await?;
    live.extend(
        TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
            .blob_refs()
            .await?,
    );
    ctx.blobs.gc(&live).await
}

/// Renders templates and prepares a stored request with its folders,
/// collection and global settings.
pub(crate) async fn prepare_stored(
    ctx: &AppContext,
    request_id: Id,
    prompts: &HashMap<String, String>,
    chain: &[Id],
) -> CoreResult<(PreparedRequest, Id)> {
    prepare_stored_with(ctx, request_id, prompts, chain, true).await
}

/// Like [`prepare_stored`]; with `render: false` templates stay as written.
pub(crate) async fn prepare_stored_with(
    ctx: &AppContext,
    request_id: Id,
    prompts: &HashMap<String, String>,
    chain: &[Id],
    render: bool,
) -> CoreResult<(PreparedRequest, Id)> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    // WebSocket handshakes and gRPC calls are prepared like HTTP requests.
    let converted;
    let http = match &node.data {
        NodeData::Request(r) => r,
        NodeData::WebSocket(w) => {
            converted = w.as_http();
            &converted
        }
        NodeData::Grpc(g) => {
            converted = g.as_http();
            &converted
        }
        NodeData::Folder(_) => {
            return Err(
                CoreError::new("request.not_http", "only requests can be sent")
                    .with_param("id", request_id.to_string()),
            );
        }
    };
    let scope = Scope::load(ctx, node.collection_id, Some(request_id)).await?;
    let host = SendHost {
        ctx,
        scope: &scope,
        prompt_values: prompts,
        chain,
    };
    let options = RenderOptions {
        stable: true,
        locale: template::faker_locale(ctx, node.collection_id).await,
        ..RenderOptions::default()
    };
    let renderer = Renderer::new(&scope.vars, &host, options);

    let mut http = http.clone();
    if render {
        template::render_request(&renderer, &mut http).await?;
    }
    let mut rendered_folders: Vec<(Id, FolderData)> = Vec::new();
    for n in &scope.ancestors {
        if let NodeData::Folder(f) = &n.data {
            let mut f = f.clone();
            if render {
                template::render_folder(&renderer, &mut f).await?;
            }
            rendered_folders.push((n.meta.id, f));
        }
    }
    let folders: Vec<&FolderData> = rendered_folders.iter().map(|(_, f)| f).collect();
    let collection = tree.get_collection(node.collection_id).await?;
    let global: RequestSettings = setting(ctx, HTTP_DEFAULTS_KEY).await?;
    let base = collection.settings.merge(&global);

    let mut prepared = prepare_with_base(&http, &folders, &base)?;
    if render {
        let mut sign =
            reqly_http::auth::SignContext::system(ctx.platform != super::Platform::Server);
        // OAuth 2.0 tokens belong to the node that defines the auth.
        let owner = match &http.auth {
            reqly_core::model::Auth::OAuth2(config) => Some((request_id, config.clone())),
            reqly_core::model::Auth::Inherit => rendered_folders
                .iter()
                .rev()
                .find_map(|(id, f)| match &f.auth {
                    reqly_core::model::Auth::Inherit => None,
                    reqly_core::model::Auth::OAuth2(config) => Some(Some((*id, config.clone()))),
                    _ => Some(None),
                })
                .flatten(),
            _ => None,
        };
        if let Some((owner, config)) = owner {
            sign.oauth2_token = Some(super::oauth2::access_token(ctx, owner, &config).await?);
        }
        reqly_http::auth::finalize(&mut prepared, &sign)?;
    }
    Ok((prepared, collection.workspace_id))
}

/// TLS settings and the workspace cookies for a request to `url`.
pub(crate) async fn exec_options(
    ctx: &AppContext,
    workspace_id: Id,
    url: &str,
) -> CoreResult<ExecOptions> {
    let tls: TlsSettings = setting(ctx, HTTP_TLS_KEY).await?;
    let cookies_repo = CookieRepo::new(&ctx.storage, ctx.clock.as_ref());
    Ok(ExecOptions {
        ca_pems: tls.ca_pems.clone(),
        client_identity: client_identity(&tls, url)?,
        cookies: cookies_repo.netscape_lines(workspace_id).await?,
        ..ExecOptions::default()
    })
}

/// TLS options for connections that do not go through libcurl (WebSocket, gRPC).
pub(crate) async fn stream_tls(
    ctx: &AppContext,
    url: &str,
    settings: &RequestSettings,
) -> CoreResult<reqly_protocols::tls::TlsOptions> {
    let tls: TlsSettings = setting(ctx, HTTP_TLS_KEY).await?;
    let global: RequestSettings = setting(ctx, HTTP_DEFAULTS_KEY).await?;
    let identity = match client_identity(&tls, url)? {
        Some(ClientIdentity::Pem { cert, key }) => Some((cert, key)),
        Some(ClientIdentity::Pkcs12 { .. }) => {
            tracing::warn!("PKCS#12 client certificates are not supported for WebSocket and gRPC");
            None
        }
        None => None,
    };
    Ok(reqly_protocols::tls::TlsOptions {
        verify: settings.verify_tls.or(global.verify_tls).unwrap_or(true),
        ca_pems: tls.ca_pems,
        identity,
        alpn: Vec::new(),
    })
}

/// Renders `text` in the scope of a request (variables, environments, tags).
pub(crate) async fn render_in_request(
    ctx: &AppContext,
    request_id: Id,
    text: &str,
) -> CoreResult<String> {
    if !reqly_template::has_template(text) {
        return Ok(text.to_string());
    }
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let scope = Scope::load(ctx, node.collection_id, Some(request_id)).await?;
    let prompts = HashMap::new();
    let host = SendHost {
        ctx,
        scope: &scope,
        prompt_values: &prompts,
        chain: &[],
    };
    let options = RenderOptions {
        locale: template::faker_locale(ctx, node.collection_id).await,
        ..RenderOptions::default()
    };
    Renderer::new(&scope.vars, &host, options)
        .render(text)
        .await
}

/// Sends through the network policy when the server has one.
pub(crate) async fn execute(
    ctx: &AppContext,
    prepared: PreparedRequest,
    options: ExecOptions,
    cancel: CancelToken,
) -> CoreResult<HttpResponse> {
    match &ctx.network_policy {
        Some(policy) => {
            reqly_http::policy::send_checked(prepared, options, cancel, None, policy).await
        }
        None => reqly_http::engine::execute(prepared, options, cancel, None).await,
    }
}

pub async fn send_request(ctx: &AppContext, request_id: Id) -> CoreResult<ResponseRecord> {
    send_request_with(ctx, request_id, &HashMap::new(), &[request_id]).await
}

/// Sends a request; `chain` holds the requests already being sent because
/// of `response` tags (including this one).
pub async fn send_request_with(
    ctx: &AppContext,
    request_id: Id,
    prompts: &HashMap<String, String>,
    chain: &[Id],
) -> CoreResult<ResponseRecord> {
    let node = TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
        .get_node(request_id)
        .await?;
    if matches!(node.data, NodeData::WebSocket(_) | NodeData::Grpc(_)) {
        return Err(CoreError::new(
            "request.use_connect",
            "this request opens a connection; connect instead",
        ));
    }
    let (mut prepared, workspace_id) = prepare_stored(ctx, request_id, prompts, chain).await?;
    let snapshot = serde_json::to_value(RequestSnapshot::from(&prepared)).unwrap_or_default();
    resolve_blobs(ctx, &mut prepared)?;

    let cookies_repo = CookieRepo::new(&ctx.storage, ctx.clock.as_ref());
    let options = exec_options(ctx, workspace_id, &prepared.url).await?;
    let store_cookies = prepared.settings.store_cookies.unwrap_or(true);

    let cancel = CancelToken::new();
    ctx.inflight
        .lock()
        .map_err(|_| CoreError::internal("inflight lock poisoned"))?
        .insert(request_id, cancel.clone());
    let _guard = InflightGuard { ctx, request_id };
    ctx.events
        .publish(Event::new("request.started", RequestStarted { request_id }));

    // Progress events at most every 100 ms.
    let events = ctx.events.clone();
    let last = Mutex::new(Instant::now());
    let progress = Box::new(move |downloaded: u64, total: Option<u64>| {
        if let Ok(mut last) = last.lock()
            && last.elapsed().as_millis() >= 100
        {
            *last = Instant::now();
            events.publish(Event::new(
                "request.progress",
                RequestProgress {
                    request_id,
                    downloaded,
                    total,
                },
            ));
        }
    });

    let result = match &ctx.network_policy {
        Some(policy) => {
            reqly_http::policy::send_checked(prepared, options, cancel, Some(progress), policy)
                .await
        }
        None => reqly_http::engine::execute(prepared, options, cancel, Some(progress)).await,
    };

    let history = HistoryRepo::new(&ctx.storage, ctx.clock.as_ref());
    let record = match result {
        Ok(res) => {
            let HttpResponse {
                status,
                status_text,
                http_version,
                headers,
                body,
                body_size,
                headers_size,
                timings,
                timeline,
                redirects,
                final_url,
                remote_addr,
                cookies,
            } = res;
            if store_cookies && !cookies.is_empty() {
                cookies_repo.replace_all(workspace_id, &cookies).await?;
                ctx.events.publish(Event::new(
                    "cookies.changed",
                    CookiesChanged { workspace_id },
                ));
            }
            let content_type = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone());
            let body_hash = store_body(ctx, body).await?;
            history
                .insert(NewResponse {
                    request_id,
                    workspace_id,
                    status: Some(status),
                    status_text,
                    http_version,
                    headers,
                    body_hash: Some(body_hash),
                    body_size,
                    headers_size,
                    content_type,
                    timings: serde_json::to_value(timings).unwrap_or_default(),
                    timeline: serde_json::to_value(timeline).unwrap_or_default(),
                    request_snapshot: snapshot,
                    redirects,
                    final_url,
                    remote_addr,
                    error: None,
                })
                .await?
        }
        Err(err) if err.code == "http.cancelled" => {
            ctx.events.publish(Event::new(
                "request.finished",
                RequestFinished {
                    request_id,
                    response_id: None,
                },
            ));
            return Err(err);
        }
        Err(mut err) => {
            let timeline = err
                .params
                .remove("timeline")
                .unwrap_or(Value::Array(Vec::new()));
            let url = snapshot
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            history
                .insert(NewResponse {
                    request_id,
                    workspace_id,
                    status: None,
                    status_text: String::new(),
                    http_version: String::new(),
                    headers: Vec::new(),
                    body_hash: None,
                    body_size: 0,
                    headers_size: 0,
                    content_type: None,
                    timings: serde_json::json!({ "totalMs": 0.0 }),
                    timeline,
                    request_snapshot: snapshot,
                    redirects: 0,
                    final_url: url,
                    remote_addr: None,
                    error: Some(err),
                })
                .await?
        }
    };

    if let Err(e) = housekeeping(ctx).await {
        tracing::warn!(error = %e, "history housekeeping failed");
    }
    ctx.events.publish(Event::new(
        "request.finished",
        RequestFinished {
            request_id,
            response_id: Some(record.id),
        },
    ));
    Ok(record)
}

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
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
pub struct RequestIdInput {
    pub request_id: Id,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RequestSendInput {
    pub request_id: Id,
    /// Values for `{% prompt %}` tags, by prompt key.
    #[ts(optional)]
    pub prompt_values: Option<HashMap<String, String>>,
}

command!(
    RequestSend,
    "request.send",
    RequestSendInput,
    ResponseRecord,
    |ctx, input| {
        send_request_with(
            ctx,
            input.request_id,
            &input.prompt_values.unwrap_or_default(),
            &[input.request_id],
        )
        .await
    }
);

command!(
    RequestCancel,
    "request.cancel",
    RequestIdInput,
    Empty,
    |ctx, input| {
        if let Some(token) = ctx
            .inflight
            .lock()
            .ok()
            .and_then(|m| m.get(&input.request_id).cloned())
        {
            token.cancel();
        }
        Ok(Empty {})
    }
);

// ----- history ---------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResponseListInput {
    pub request_id: Id,
    #[ts(optional)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResponseBodyInput {
    pub id: Id,
    pub offset: u64,
    pub limit: u32,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResponseBodyChunkOutput {
    pub base64: String,
    pub offset: u64,
    pub total: u64,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryWorkspaceInput {
    pub workspace_id: Id,
    #[ts(optional)]
    pub filter: Option<String>,
    #[ts(optional)]
    pub limit: Option<u32>,
}

fn history(ctx: &AppContext) -> HistoryRepo<'_> {
    HistoryRepo::new(&ctx.storage, ctx.clock.as_ref())
}

command!(
    ResponseList,
    "response.list",
    ResponseListInput,
    Vec<ResponseSummary>,
    |ctx, input| {
        history(ctx)
            .list_for_request(input.request_id, input.limit.unwrap_or(100))
            .await
    }
);

command!(
    ResponseGet,
    "response.get",
    IdInput,
    ResponseRecord,
    |ctx, input| { history(ctx).get(input.id).await }
);

command!(
    /// Reads part of a response body; bodies are paged into the UI so large
    /// responses never travel in one message.

    ResponseBodyChunk,
    "response.body",
    ResponseBodyInput,
    ResponseBodyChunkOutput,
    |ctx, input| {
        let record = history(ctx).get(input.id).await?;
        let Some(hash) = record.body_hash else {
            return Ok(ResponseBodyChunkOutput {
                base64: String::new(),
                offset: input.offset,
                total: 0,
            });
        };
        let limit = input.limit.min(16 * 1024 * 1024) as usize;
        let bytes = ctx.blobs.read_range(&hash, input.offset, limit).await?;
        Ok(ResponseBodyChunkOutput {
            base64: STANDARD.encode(bytes),
            offset: input.offset,
            total: record.body_size,
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ResponseSaveInput {
    pub id: Id,
    /// Destination chosen in the native save dialog.
    pub path: String,
}

command!(
    /// Writes a response body to a local file. Desktop only: on the server
    /// the path would be on the server's disk.
    ResponseSaveBody,
    "response.saveBody",
    ResponseSaveInput,
    Empty,
    |ctx, input| {
        if ctx.platform != super::Platform::Desktop && ctx.platform != super::Platform::Test {
            return Err(CoreError::new("command.unavailable", "only available in the desktop app"));
        }
        let record = history(ctx).get(input.id).await?;
        let write_err = |e: std::io::Error| {
            CoreError::new("file.write", e.to_string()).with_param("path", input.path.clone())
        };
        match record.body_hash {
            Some(hash) => {
                tokio::fs::copy(ctx.blobs.path_of(&hash)?, &input.path).await.map_err(write_err)?;
            }
            None => tokio::fs::write(&input.path, b"").await.map_err(write_err)?,
        }
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ResponseFilterInput {
    pub id: Id,
    pub kind: reqly_http::filter::FilterKind,
    pub expression: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ResponseFilterOutput {
    pub result: String,
}

/// Bodies larger than this are not filtered (the UI suggests saving instead).
const FILTER_LIMIT: usize = 64 * 1024 * 1024;

command!(
    /// Applies a JSONPath, jq or XPath expression to a response body.
    ResponseFilter,
    "response.filter",
    ResponseFilterInput,
    ResponseFilterOutput,
    |ctx, input| {
        let record = history(ctx).get(input.id).await?;
        let Some(hash) = record.body_hash else {
            return Ok(ResponseFilterOutput { result: String::new() });
        };
        if record.body_size as usize > FILTER_LIMIT {
            return Err(CoreError::new("filter.too_large", "response body is too large to filter"));
        }
        let bytes = ctx.blobs.read(&hash).await?;
        let body = String::from_utf8_lossy(&bytes).into_owned();
        let expression = input.expression;
        let kind = input.kind;
        let result = tokio::task::spawn_blocking(move || reqly_http::filter::apply_filter(kind, &body, &expression))
            .await
            .map_err(|e| CoreError::internal(e.to_string()))??;
        Ok(ResponseFilterOutput { result })
    }
);

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CleanupOutput {
    pub removed_history: u64,
    pub removed_trash: u64,
    pub removed_blobs: usize,
}

/// Trash entries older than this are purged by cleanup.
const TRASH_RETENTION_MS: i64 = 30 * 86_400_000;

command!(
    /// Applies history limits, empties old trash and deletes unreferenced files.
    MaintenanceCleanup,
    "maintenance.cleanup",
    super::Empty,
    CleanupOutput,
    |ctx, _input| {
        let limits: HistoryLimitSettings = setting(ctx, HISTORY_LIMITS_KEY).await?;
        let removed_history = history(ctx).enforce_limits(&limits.into()).await?;
        let cutoff = reqly_core::Timestamp(ctx.clock.now().0 - TRASH_RETENTION_MS);
        let removed_trash = TreeRepo::new(&ctx.storage, ctx.clock.as_ref()).purge_deleted_before(cutoff).await?;
        let removed_blobs = collect_garbage(ctx).await?;
        Ok(CleanupOutput { removed_history, removed_trash, removed_blobs })
    }
);

command!(
    ResponseDelete,
    "response.delete",
    IdsInput,
    Empty,
    |ctx, input| {
        history(ctx).delete(&input.ids).await?;
        collect_garbage(ctx).await?;
        Ok(Empty {})
    }
);

command!(
    /// Clears history of requests, or of every request inside the given folders.

    HistoryClear,
    "history.clear",
    IdsInput,
    Empty,
    |ctx, input| {
        let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut request_ids = Vec::new();
        for id in input.ids {
            let node = tree.get_node(id).await?;
            match node.data.kind() {
                NodeKind::Request => request_ids.push(id),
                NodeKind::Folder => {
                    let all = tree.list_tree(node.collection_id).await?;
                    let mut frontier = vec![id];
                    while let Some(parent) = frontier.pop() {
                        for child in all.iter().filter(|n| n.parent_id == Some(parent)) {
                            match child.kind {
                                NodeKind::Request => request_ids.push(child.id),
                                NodeKind::Folder => frontier.push(child.id),
                            }
                        }
                    }
                }
            }
        }
        history(ctx).clear_for_requests(&request_ids).await?;
        collect_garbage(ctx).await?;
        Ok(Empty {})
    }
);

command!(
    HistoryWorkspace,
    "history.workspace",
    HistoryWorkspaceInput,
    Vec<ResponseSummary>,
    |ctx, input| {
        history(ctx)
            .list_for_workspace(
                input.workspace_id,
                input.filter.as_deref(),
                input.limit.unwrap_or(200),
            )
            .await
    }
);

// ----- cookies ----------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CookieUpsertInput {
    pub workspace_id: Id,
    pub cookie: Cookie,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CookieDeleteInput {
    pub workspace_id: Id,
    pub ids: Vec<Id>,
}

fn cookies(ctx: &AppContext) -> CookieRepo<'_> {
    CookieRepo::new(&ctx.storage, ctx.clock.as_ref())
}

fn cookies_changed(ctx: &AppContext, workspace_id: Id) {
    ctx.events.publish(Event::new(
        "cookies.changed",
        CookiesChanged { workspace_id },
    ));
}

command!(
    CookiesList,
    "cookies.list",
    WorkspaceIdInput,
    Vec<Cookie>,
    |ctx, input| { cookies(ctx).list(input.workspace_id).await }
);

command!(
    CookiesUpsert,
    "cookies.upsert",
    CookieUpsertInput,
    Empty,
    |ctx, input| {
        cookies(ctx)
            .upsert(input.workspace_id, &input.cookie)
            .await?;
        cookies_changed(ctx, input.workspace_id);
        Ok(Empty {})
    }
);

command!(
    CookiesDelete,
    "cookies.delete",
    CookieDeleteInput,
    Empty,
    |ctx, input| {
        cookies(ctx).delete(input.workspace_id, &input.ids).await?;
        cookies_changed(ctx, input.workspace_id);
        Ok(Empty {})
    }
);

command!(
    CookiesClear,
    "cookies.clear",
    WorkspaceIdInput,
    Empty,
    |ctx, input| {
        cookies(ctx).clear(input.workspace_id).await?;
        cookies_changed(ctx, input.workspace_id);
        Ok(Empty {})
    }
);

// ----- uploads ----------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct BlobUploadInput {
    pub name: String,
    pub base64: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct BlobUploadOutput {
    pub hash: String,
    pub name: String,
    pub size: u64,
}

command!(
    /// Stores a file for request bodies (server mode, where the browser cannot
    /// hand the server a local path).

    BlobUpload,
    "blob.upload",
    BlobUploadInput,
    BlobUploadOutput,
    |ctx, input| {
        let bytes = STANDARD
            .decode(input.base64.trim())
            .map_err(|e| CoreError::new("command.invalid_input", e.to_string()))?;
        let blob = ctx.blobs.put_bytes(&bytes).await?;
        Ok(BlobUploadOutput {
            hash: blob.hash,
            name: input.name,
            size: blob.size,
        })
    }
);

// ----- cURL ---------------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CurlParseInput {
    pub command: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CurlParseOutput {
    pub request: reqly_core::model::HttpRequest,
    pub warnings: Vec<String>,
}

command!(
    /// Parses one or more cURL commands into request data.
    CurlParse,
    "curl.parse",
    CurlParseInput,
    Vec<CurlParseOutput>,
    |_ctx, input| {
        Ok(reqly_http::curl::parse_curl_many(&input.command)?
            .into_iter()
            .map(|p| CurlParseOutput { request: p.request, warnings: p.warnings })
            .collect())
    }
);

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CurlOutput {
    pub command: String,
}

command!(
    /// "Copy as cURL": the request as it would be sent (inherited headers and auth included).
    RequestToCurl,
    "request.toCurl",
    RequestIdInput,
    CurlOutput,
    |ctx, input| {
        let (prepared, _) =
            prepare_stored(ctx, input.request_id, &HashMap::new(), &[input.request_id]).await?;
        Ok(CurlOutput { command: reqly_http::curl::to_curl(&prepared) })
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_certificate_host_patterns() {
        assert!(host_matches("api.test", "https://api.test/x"));
        assert!(host_matches("api.test:443", "https://api.test/x"));
        assert!(!host_matches("api.test:8443", "https://api.test/x"));
        assert!(host_matches("*.test", "https://a.b.test/"));
        assert!(!host_matches("*.test", "https://atest/"));
        assert!(!host_matches("other.test", "https://api.test/"));
    }

    #[test]
    fn history_limit_settings_convert() {
        let l: HistoryLimits = HistoryLimitSettings {
            per_request: 0,
            total_megabytes: 2,
            max_age_days: Some(1),
        }
        .into();
        assert_eq!(l.per_request, 1);
        assert_eq!(l.total_bytes, 2 * 1024 * 1024);
        assert_eq!(l.max_age_ms, Some(86_400_000));
    }
}
