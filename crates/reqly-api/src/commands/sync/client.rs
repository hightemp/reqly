//! The desktop side of sync: the connection to a server, sync rounds and
//! their status (ADR-013).

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reqly_core::model::{Body, BodyKind, HttpRequest, KeyValue, RequestSettings};
use reqly_core::{CoreError, CoreResult, Id, Timestamp};
use reqly_http::engine::{CancelToken, ExecOptions};
use reqly_storage::settings::SettingsRepo;
use reqly_storage::sync::SyncClient;
use reqly_storage::tree::TreeRepo;
use reqly_sync::{
    Conflict, Kind, LocalStore, PullInput, PullOutput, PushInput, PushOutput, Remote, Report,
    Resolution,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::super::AppContext;
use super::super::Empty;
use super::super::http::{HTTP_DEFAULTS_KEY, HTTP_TLS_KEY, TlsSettings, setting};
use super::publish_changes;
use crate::events::Event;
use crate::registry::{Command, Registry};
use reqly_core::model::Workspace;

/// The connection (setting `sync`); the token is encrypted.
const CONNECTION_KEY: &str = "sync";
/// How often a connected app syncs on its own.
const INTERVAL: Duration = Duration::from_secs(30);
/// How long after a local change the app syncs.
const DEBOUNCE: Duration = Duration::from_secs(2);
const LOG_SIZE: usize = 50;

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<SyncStatusCommand>()
        .register::<SyncConnect>()
        .register::<SyncDisconnect>()
        .register::<SyncWorkspaces>()
        .register::<SyncFollow>()
        .register::<SyncUnfollow>()
        .register::<SyncSettingsCommand>()
        .register::<SyncNow>()
        .register::<SyncConflicts>()
        .register::<SyncResolve>()
        .register::<SyncLog>();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    url: String,
    /// Encrypted bearer token (`None`: an open server).
    token: Option<String>,
    /// Who the token belongs to (multi-user servers).
    user: Option<String>,
    /// Sync the values of secret environment variables.
    #[serde(default)]
    secrets: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncLogEntry {
    pub at: Timestamp,
    pub pulled: u32,
    pub pushed: u32,
    pub conflicts: u32,
    /// Local changes the server refused (no edit access…).
    pub rejected: u32,
    pub error: Option<CoreError>,
}

/// Sync state kept in memory by the app (the connection is a setting).
#[derive(Default)]
pub struct SyncState {
    round: tokio::sync::Mutex<()>,
    live: Mutex<Live>,
    wake: tokio::sync::Notify,
}

#[derive(Default)]
struct Live {
    syncing: bool,
    last_sync_at: Option<Timestamp>,
    last_error: Option<CoreError>,
    log: VecDeque<SyncLogEntry>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncStatus {
    /// The server, when connected.
    pub url: Option<String>,
    pub user: Option<String>,
    pub secrets: bool,
    /// The synced workspaces.
    pub workspaces: Vec<Id>,
    pub syncing: bool,
    pub last_sync_at: Option<Timestamp>,
    /// The error of the last round, if it failed.
    pub last_error: Option<CoreError>,
    /// Local changes not sent yet.
    pub pending: u32,
    pub conflicts: u32,
}

fn network(detail: impl Into<String>) -> CoreError {
    let detail = detail.into();
    CoreError::new("sync.network", format!("cannot reach the server: {detail}"))
        .with_param("detail", detail)
}

async fn connection(ctx: &AppContext) -> CoreResult<Option<Connection>> {
    let Some(raw) = SettingsRepo::new(&ctx.storage).get(CONNECTION_KEY).await? else {
        return Ok(None);
    };
    Ok(serde_json::from_value(raw).ok())
}

async fn save_connection(ctx: &AppContext, connection: Option<&Connection>) -> CoreResult<()> {
    let value = match connection {
        Some(c) => serde_json::to_value(c).map_err(|e| CoreError::internal(e.to_string()))?,
        None => Value::Null,
    };
    SettingsRepo::new(&ctx.storage)
        .set(CONNECTION_KEY, &value, ctx.clock.as_ref())
        .await
}

/// A server reached over HTTP with the app's proxy and CA settings.
pub struct HttpServer<'a> {
    ctx: &'a AppContext,
    url: String,
    token: Option<String>,
}

impl<'a> HttpServer<'a> {
    fn new(ctx: &'a AppContext, url: &str, token: Option<String>) -> Self {
        Self {
            ctx,
            url: url.trim_end_matches('/').to_string(),
            token,
        }
    }

    async fn connected(ctx: &'a AppContext, c: &Connection) -> CoreResult<Self> {
        let token = c
            .token
            .as_deref()
            .map(|t| ctx.storage.secrets().decrypt(t))
            .transpose()?;
        Ok(Self::new(ctx, &c.url, token))
    }

    /// Sends a request to `path` (under the server URL); returns the status,
    /// the headers and the body.
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> CoreResult<(u16, Vec<(String, String)>, Vec<u8>)> {
        let global: RequestSettings = setting(self.ctx, HTTP_DEFAULTS_KEY).await?;
        let tls: TlsSettings = setting(self.ctx, HTTP_TLS_KEY).await?;
        let mut headers = vec![
            KeyValue::new("Accept", "application/json"),
            KeyValue::new("User-Agent", format!("Reqly/{}", env!("CARGO_PKG_VERSION"))),
        ];
        if let Some(token) = &self.token {
            headers.push(KeyValue::new("Authorization", format!("Bearer {token}")));
        }
        if let Some(cookie) = cookie {
            headers.push(KeyValue::new("Cookie", cookie));
        }
        let request = HttpRequest {
            method: method.into(),
            url: format!("{}{path}", self.url),
            headers,
            body: body.map_or_else(Body::default, |b| Body {
                kind: BodyKind::Json,
                text: b.to_string(),
                ..Body::default()
            }),
            settings: RequestSettings {
                timeout_ms: Some(60_000),
                ..global
            },
            ..HttpRequest::default()
        };
        let prepared =
            reqly_http::prepare::prepare(&request, &[]).map_err(|e| network(e.message))?;
        let options = ExecOptions {
            ca_pems: tls.ca_pems,
            ..ExecOptions::default()
        };
        let response = reqly_http::engine::execute(prepared, options, CancelToken::new(), None)
            .await
            .map_err(|e| network(e.message))?;
        let bytes = response.body.to_vec().map_err(|e| network(e.to_string()))?;
        Ok((response.status, response.headers, bytes))
    }

    /// Runs a command on the server.
    async fn call<O: DeserializeOwned>(
        &self,
        name: &str,
        input: Value,
        cookie: Option<&str>,
    ) -> CoreResult<O> {
        let (status, _, body) = self
            .send("POST", &format!("/api/v1/cmd/{name}"), Some(input), cookie)
            .await?;
        if !(200..300).contains(&status) {
            return Err(
                serde_json::from_slice::<CoreError>(&body).unwrap_or_else(|_| {
                    network(format!("the server answered {status}"))
                        .with_param("status", u64::from(status))
                }),
            );
        }
        serde_json::from_slice(&body)
            .map_err(|e| CoreError::new("sync.invalid", format!("unexpected answer: {e}")))
    }
}

#[async_trait]
impl Remote for HttpServer<'_> {
    async fn pull(&self, input: PullInput) -> CoreResult<PullOutput> {
        self.call("sync.pull", json!(input), None).await
    }

    async fn push(&self, input: PushInput) -> CoreResult<PushOutput> {
        self.call("sync.push", json!(input), None).await
    }
}

fn client<'a>(ctx: &'a AppContext, c: &Connection) -> SyncClient<'a> {
    SyncClient::new(&ctx.storage, ctx.clock.as_ref(), c.secrets)
}

pub async fn status(ctx: &AppContext) -> CoreResult<SyncStatus> {
    let connection = connection(ctx).await?;
    let (workspaces, pending, conflicts) = match &connection {
        Some(c) => {
            let client = client(ctx, c);
            let workspaces = client
                .cursors()
                .await?
                .into_iter()
                .map(|c| c.workspace_id)
                .collect();
            let conflicts = client.conflicts().await?;
            let waiting: HashSet<(Kind, Id)> = conflicts
                .iter()
                .map(|c| (c.local.kind, c.local.id))
                .collect();
            let pending = client
                .changed()
                .await?
                .iter()
                .filter_map(|e| e.current())
                .filter(|r| !waiting.contains(&(r.kind, r.id)))
                .count();
            (workspaces, pending as u32, conflicts.len() as u32)
        }
        None => (Vec::new(), 0, 0),
    };
    let live = ctx
        .sync
        .live
        .lock()
        .map_err(|_| CoreError::internal("sync state poisoned"))?;
    Ok(SyncStatus {
        url: connection.as_ref().map(|c| c.url.clone()),
        user: connection.as_ref().and_then(|c| c.user.clone()),
        secrets: connection.as_ref().is_some_and(|c| c.secrets),
        workspaces,
        syncing: live.syncing,
        last_sync_at: live.last_sync_at,
        last_error: live.last_error.clone(),
        pending,
        conflicts,
    })
}

async fn publish_status(ctx: &AppContext) {
    if let Ok(status) = status(ctx).await {
        ctx.events.publish(Event::new("sync.status", status));
    }
}

fn set_live(ctx: &AppContext, f: impl FnOnce(&mut Live)) {
    if let Ok(mut live) = ctx.sync.live.lock() {
        f(&mut live);
    }
}

/// Runs one sync round now, if connected; rounds never overlap.
pub async fn sync_now(ctx: &AppContext) -> CoreResult<Option<Report>> {
    let _round = ctx.sync.round.lock().await;
    let Some(c) = connection(ctx).await? else {
        return Ok(None);
    };
    set_live(ctx, |l| l.syncing = true);
    publish_status(ctx).await;
    let result = async {
        let server = HttpServer::connected(ctx, &c).await?;
        reqly_sync::round(&client(ctx, &c), &server, c.secrets).await
    }
    .await;
    let now = ctx.clock.now();
    let entry = match &result {
        Ok(r) => SyncLogEntry {
            at: now,
            pulled: r.pulled,
            pushed: r.pushed,
            conflicts: r.conflicts,
            rejected: r.rejected.len() as u32,
            error: None,
        },
        Err(e) => SyncLogEntry {
            at: now,
            pulled: 0,
            pushed: 0,
            conflicts: 0,
            rejected: 0,
            error: Some(e.clone()),
        },
    };
    if let Ok(report) = &result {
        publish_changes(ctx, &report.written).await;
        // Workspaces gone on the server (deleted, access removed) stop syncing.
        let client = client(ctx, &c);
        for ws in &report.unavailable {
            client.unfollow(*ws).await?;
        }
    }
    set_live(ctx, |l| {
        l.syncing = false;
        l.last_error = entry.error.clone();
        if entry.error.is_none() {
            l.last_sync_at = Some(now);
        }
        // Quiet rounds are not worth a line.
        let quiet = entry.error.is_none()
            && entry.pulled + entry.pushed + entry.conflicts + entry.rejected == 0;
        if !quiet {
            l.log.push_front(entry.clone());
            l.log.truncate(LOG_SIZE);
        }
    });
    publish_status(ctx).await;
    result.map(Some)
}

/// Asks the background loop to sync soon.
pub fn wake(ctx: &AppContext) {
    ctx.sync.wake.notify_one();
}

/// Syncs in the background while connected: every 30 seconds, shortly
/// after a local change and when woken ([`wake`]).
pub fn spawn_loop(ctx: Arc<AppContext>) {
    let mut events = ctx.events.subscribe();
    tokio::spawn(async move {
        loop {
            let changed = tokio::select! {
                _ = tokio::time::sleep(INTERVAL) => false,
                _ = ctx.sync.wake.notified() => false,
                event = events.recv() => match event {
                    Ok(e) => matches!(
                        e.topic.as_str(),
                        "tree.changed" | "collections.changed" | "environments.changed" | "workspaces.changed"
                    ),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(_) => true,
                },
            };
            let syncing = ctx.sync.live.lock().map(|l| l.syncing).unwrap_or(false);
            if changed {
                if syncing {
                    // Our own writes; the round in progress covers them.
                    continue;
                }
                tokio::time::sleep(DEBOUNCE).await;
                // Drop what queued up meanwhile.
                while events.try_recv().is_ok() {}
            }
            if let Err(e) = sync_now(&ctx).await {
                tracing::debug!(error = %e, "sync round failed");
            }
            // Changes the round itself announced are not new local work.
            while events.try_recv().is_ok() {}
        }
    });
}

// ----- commands -------------------------------------------------------------

/// Connection state, pending changes and conflicts.
pub struct SyncStatusCommand;

impl Command<AppContext> for SyncStatusCommand {
    const NAME: &'static str = "sync.status";
    type Input = Empty;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<SyncStatus> {
        status(ctx).await
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncConnectInput {
    pub url: String,
    /// The server's access token, or a personal API token.
    #[ts(optional)]
    pub token: Option<String>,
    /// Sign in to a multi-user server; a personal API token is made for
    /// this app.
    #[ts(optional)]
    pub username: Option<String>,
    #[ts(optional)]
    pub password: Option<String>,
}

#[derive(Deserialize)]
struct AuthStatus {
    mode: String,
}

#[derive(Deserialize)]
struct CreatedToken {
    secret: String,
}

#[derive(Deserialize)]
struct Me {
    user: Option<MeUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeUser {
    display_name: String,
    username: String,
}

/// Connects to a server: checks it, signs in if needed and keeps the
/// token. A different server than before starts sync over.
pub struct SyncConnect;

impl Command<AppContext> for SyncConnect {
    const NAME: &'static str = "sync.connect";
    type Input = SyncConnectInput;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, input: SyncConnectInput) -> CoreResult<SyncStatus> {
        let url = input.url.trim().trim_end_matches('/').to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(CoreError::new(
                "sync.url",
                "the server URL must start with http:// or https://",
            ));
        }
        let anonymous = HttpServer::new(ctx, &url, None);
        let (code, _, body) = anonymous
            .send("GET", "/api/v1/auth/status", None, None)
            .await?;
        let mode = serde_json::from_slice::<AuthStatus>(&body)
            .ok()
            .filter(|_| code == 200)
            .ok_or_else(|| CoreError::new("sync.not_reqly", "this is not a Reqly server"))?
            .mode;
        let given = input
            .token
            .filter(|t| !t.trim().is_empty())
            .map(|t| t.trim().to_string());
        let token = match (mode.as_str(), given, input.username) {
            ("none", _, _) => None,
            (_, Some(token), _) => Some(token),
            ("multi", None, Some(username)) => {
                let password = input.password.unwrap_or_default();
                let (code, headers, body) = anonymous
                    .send(
                        "POST",
                        "/api/v1/auth/login",
                        Some(json!({ "username": username, "password": password })),
                        None,
                    )
                    .await?;
                if code != 200 {
                    return Err(serde_json::from_slice::<CoreError>(&body)
                        .unwrap_or_else(|_| CoreError::new("auth.invalid", "sign-in failed")));
                }
                let cookie = headers
                    .iter()
                    .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
                    .filter_map(|(_, v)| v.split(';').next())
                    .find(|c| c.trim_start().starts_with("reqly_session="))
                    .map(str::to_string)
                    .ok_or_else(|| CoreError::new("auth.invalid", "sign-in failed"))?;
                let host = hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "desktop".into());
                let created: CreatedToken = anonymous
                    .call(
                        "account.createToken",
                        json!({ "name": format!("Reqly sync ({host})") }),
                        Some(&cookie),
                    )
                    .await?;
                let _ = anonymous
                    .send("POST", "/api/v1/auth/logout", None, Some(&cookie))
                    .await;
                Some(created.secret)
            }
            _ => {
                return Err(CoreError::new(
                    "sync.credentials",
                    "the server needs a token or a user name and password",
                ));
            }
        };
        // The token must work for commands.
        let server = HttpServer::new(ctx, &url, token.clone());
        let _: Vec<Workspace> = server.call("workspace.list", json!({}), None).await?;
        let user = if mode == "multi" {
            server
                .call::<Me>("account.me", json!({}), None)
                .await?
                .user
                .map(|u| format!("{} ({})", u.display_name, u.username))
        } else {
            None
        };

        let previous = connection(ctx).await?;
        if previous.as_ref().is_some_and(|p| p.url != url) {
            SyncClient::new(&ctx.storage, ctx.clock.as_ref(), false)
                .unfollow_all()
                .await?;
        }
        let c = Connection {
            url,
            token: token.map(|t| ctx.storage.secrets().encrypt(&t)),
            user,
            secrets: previous.is_some_and(|p| p.secrets),
        };
        save_connection(ctx, Some(&c)).await?;
        set_live(ctx, |l| *l = Live::default());
        wake(ctx);
        publish_status(ctx).await;
        status(ctx).await
    }
}

/// Forgets the server; local data stays.
pub struct SyncDisconnect;

impl Command<AppContext> for SyncDisconnect {
    const NAME: &'static str = "sync.disconnect";
    type Input = Empty;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<SyncStatus> {
        let _round = ctx.sync.round.lock().await;
        SyncClient::new(&ctx.storage, ctx.clock.as_ref(), false)
            .unfollow_all()
            .await?;
        save_connection(ctx, None).await?;
        set_live(ctx, |l| *l = Live::default());
        drop(_round);
        publish_status(ctx).await;
        status(ctx).await
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncWorkspace {
    pub id: Id,
    pub name: String,
    /// In this app.
    pub local: bool,
    /// On the server.
    pub remote: bool,
    pub synced: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncWorkspacesOutput {
    pub workspaces: Vec<SyncWorkspace>,
    /// Why the server's list is missing.
    pub error: Option<CoreError>,
}

/// Local and server workspaces and which of them sync.
pub struct SyncWorkspaces;

impl Command<AppContext> for SyncWorkspaces {
    const NAME: &'static str = "sync.workspaces";
    type Input = Empty;
    type Output = SyncWorkspacesOutput;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<SyncWorkspacesOutput> {
        let local = TreeRepo::new(&ctx.storage, ctx.clock.as_ref())
            .list_workspaces()
            .await?;
        let c = connection(ctx).await?;
        let (remote, error) = match &c {
            Some(c) => match HttpServer::connected(ctx, c)
                .await?
                .call::<Vec<Workspace>>("workspace.list", json!({}), None)
                .await
            {
                Ok(list) => (list, None),
                Err(e) => (Vec::new(), Some(e)),
            },
            None => (Vec::new(), None),
        };
        let synced: HashSet<Id> = match &c {
            Some(c) => client(ctx, c)
                .cursors()
                .await?
                .into_iter()
                .map(|c| c.workspace_id)
                .collect(),
            None => HashSet::new(),
        };
        let mut out: Vec<SyncWorkspace> = local
            .iter()
            .map(|w| SyncWorkspace {
                id: w.meta.id,
                name: w.name.clone(),
                local: true,
                remote: remote.iter().any(|r| r.meta.id == w.meta.id),
                synced: synced.contains(&w.meta.id),
            })
            .collect();
        for w in remote {
            if !out.iter().any(|o| o.id == w.meta.id) {
                out.push(SyncWorkspace {
                    id: w.meta.id,
                    name: w.name,
                    local: false,
                    remote: true,
                    synced: synced.contains(&w.meta.id),
                });
            }
        }
        Ok(SyncWorkspacesOutput {
            workspaces: out,
            error,
        })
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncWorkspaceInput {
    pub workspace_id: Id,
}

fn not_connected() -> CoreError {
    CoreError::new("sync.not_connected", "not connected to a server")
}

/// Starts syncing a workspace: a local one is uploaded, a server one
/// downloaded.
pub struct SyncFollow;

impl Command<AppContext> for SyncFollow {
    const NAME: &'static str = "sync.follow";
    type Input = SyncWorkspaceInput;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, input: SyncWorkspaceInput) -> CoreResult<SyncStatus> {
        let c = connection(ctx).await?.ok_or_else(not_connected)?;
        client(ctx, &c).follow(input.workspace_id).await?;
        wake(ctx);
        status(ctx).await
    }
}

/// Stops syncing a workspace; it stays in the app.
pub struct SyncUnfollow;

impl Command<AppContext> for SyncUnfollow {
    const NAME: &'static str = "sync.unfollow";
    type Input = SyncWorkspaceInput;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, input: SyncWorkspaceInput) -> CoreResult<SyncStatus> {
        let _round = ctx.sync.round.lock().await;
        let c = connection(ctx).await?.ok_or_else(not_connected)?;
        client(ctx, &c).unfollow(input.workspace_id).await?;
        drop(_round);
        publish_status(ctx).await;
        status(ctx).await
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncSettingsInput {
    /// Sync the values of secret environment variables.
    pub secrets: bool,
}

/// Sync options of the connection.
pub struct SyncSettingsCommand;

impl Command<AppContext> for SyncSettingsCommand {
    const NAME: &'static str = "sync.settings";
    type Input = SyncSettingsInput;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, input: SyncSettingsInput) -> CoreResult<SyncStatus> {
        let mut c = connection(ctx).await?.ok_or_else(not_connected)?;
        c.secrets = input.secrets;
        save_connection(ctx, Some(&c)).await?;
        wake(ctx);
        status(ctx).await
    }
}

/// Syncs now and returns the status ("Sync now").
pub struct SyncNow;

impl Command<AppContext> for SyncNow {
    const NAME: &'static str = "sync.now";
    type Input = Empty;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<SyncStatus> {
        if connection(ctx).await?.is_none() {
            return Err(not_connected());
        }
        sync_now(ctx).await?;
        status(ctx).await
    }
}

/// Conflicts waiting for the user.
pub struct SyncConflicts;

impl Command<AppContext> for SyncConflicts {
    const NAME: &'static str = "sync.conflicts";
    type Input = Empty;
    type Output = Vec<Conflict>;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<Vec<Conflict>> {
        match connection(ctx).await? {
            Some(c) => client(ctx, &c).conflicts().await,
            None => Ok(Vec::new()),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncResolveInput {
    pub kind: Kind,
    pub id: Id,
    pub resolution: Resolution,
}

/// Settles a conflict: keep the local version or take the server's.
pub struct SyncResolve;

impl Command<AppContext> for SyncResolve {
    const NAME: &'static str = "sync.resolve";
    type Input = SyncResolveInput;
    type Output = SyncStatus;

    async fn run(ctx: &AppContext, input: SyncResolveInput) -> CoreResult<SyncStatus> {
        let c = connection(ctx).await?.ok_or_else(not_connected)?;
        let report = {
            let _round = ctx.sync.round.lock().await;
            let server = HttpServer::connected(ctx, &c).await?;
            reqly_sync::resolve(
                &client(ctx, &c),
                &server,
                input.kind,
                input.id,
                input.resolution,
                c.secrets,
            )
            .await?
        };
        publish_changes(ctx, &report.written).await;
        publish_status(ctx).await;
        status(ctx).await
    }
}

/// The last sync rounds that did something, newest first.
pub struct SyncLog;

impl Command<AppContext> for SyncLog {
    const NAME: &'static str = "sync.log";
    type Input = Empty;
    type Output = Vec<SyncLogEntry>;

    async fn run(ctx: &AppContext, _input: Empty) -> CoreResult<Vec<SyncLogEntry>> {
        let live = ctx
            .sync
            .live
            .lock()
            .map_err(|_| CoreError::internal("sync state poisoned"))?;
        Ok(live.log.iter().cloned().collect())
    }
}
