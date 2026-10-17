//! Long-lived connections: WebSocket, server-sent events and GraphQL
//! subscriptions. One connection per request; messages are kept in a
//! bounded log and published as `stream.message` events.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqly_core::model::{BodyKind, MessageFormat, NodeData};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::{BodyEvent, CancelToken};
use reqly_http::prepare::PreparedRequest;
use reqly_protocols::graphql_ws::{self, GqlEvent};
use reqly_protocols::sse::SseParser;
use reqly_protocols::ws::{self, WsConfig, WsEvent, WsFrame, WsSender};
use reqly_storage::tree::TreeRepo;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::events::{Event, EventBus};
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<StreamConnect>()
        .register::<StreamSend>()
        .register::<StreamDisconnect>()
        .register::<StreamList>()
        .register::<StreamLog>()
        .register::<StreamClearLog>()
        .register::<StreamEnd>();
}

/// Messages kept per connection (older ones are dropped).
const LOG_LIMIT: usize = 5000;
/// Default SSE reconnection delay (the spec leaves it to the client).
const SSE_RETRY_MS: u64 = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StreamKind {
    WebSocket,
    Sse,
    GraphqlSubscription,
    Grpc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StreamStatus {
    Connecting,
    Open,
    Reconnecting,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Direction {
    Sent,
    Received,
    /// Connection lifecycle (opened, closed, errors, reconnects).
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreamMessage {
    pub seq: u64,
    /// Unix milliseconds.
    pub time: i64,
    pub direction: Direction,
    /// `text`, `binary` (base64 in `data`), `ping`, `pong`, `event` (SSE),
    /// `data` (GraphQL result), `open`, `close`, `error`, `reconnect`.
    pub kind: String,
    pub data: String,
    /// SSE event type.
    #[ts(optional)]
    pub event: Option<String>,
    /// SSE event id.
    #[ts(optional)]
    pub event_id: Option<String>,
    /// Payload size in bytes.
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreamInfo {
    pub connection_id: String,
    pub request_id: Id,
    pub kind: StreamKind,
    pub status: StreamStatus,
    /// Handshake response headers (WebSocket) or response headers (SSE).
    pub headers: Vec<(String, String)>,
    /// Negotiated subprotocol (WebSocket).
    #[ts(optional)]
    pub protocol: Option<String>,
    pub url: String,
    pub opened_at: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreamMessageEvent {
    pub connection_id: String,
    pub request_id: Id,
    pub message: StreamMessage,
}

/// A live connection.
pub struct Connection {
    info: Mutex<StreamInfo>,
    log: Mutex<VecDeque<StreamMessage>>,
    seq: AtomicU64,
    /// Set when the user disconnects: no reconnects after that.
    stopped: CancelToken,
    stop_notify: Notify,
    ws: Mutex<Option<WsSender>>,
    gql: Mutex<Option<graphql_ws::Subscription>>,
    /// Client side of a gRPC stream: messages to send and their type.
    grpc_tx: Mutex<Option<tokio::sync::mpsc::UnboundedSender<prost_reflect::DynamicMessage>>>,
    grpc_input: Mutex<Option<prost_reflect::MessageDescriptor>>,
    events: EventBus,
}

#[derive(Default)]
pub struct Streams {
    by_id: Mutex<HashMap<String, Arc<Connection>>>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Connection {
    fn info(&self) -> StreamInfo {
        self.info.lock().map(|i| i.clone()).expect("info lock")
    }

    fn set_status(&self, status: StreamStatus) {
        if let Ok(mut info) = self.info.lock() {
            info.status = status;
            self.events
                .publish(Event::new("stream.status", info.clone()));
        }
    }

    fn push(
        &self,
        direction: Direction,
        kind: &str,
        data: String,
        size: u64,
        event: Option<(String, String)>,
    ) {
        let (event, event_id) = match event {
            Some((e, id)) => (Some(e), Some(id)),
            None => (None, None),
        };
        let message = StreamMessage {
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            time: now_ms(),
            direction,
            kind: kind.into(),
            data,
            event,
            event_id,
            size,
        };
        if let Ok(mut log) = self.log.lock() {
            if log.len() >= LOG_LIMIT {
                log.pop_front();
            }
            log.push_back(message.clone());
        }
        let info = self.info();
        self.events.publish(Event::new(
            "stream.message",
            StreamMessageEvent {
                connection_id: info.connection_id,
                request_id: info.request_id,
                message,
            },
        ));
    }

    fn info_message(&self, kind: &str, text: impl Into<String>) {
        let text = text.into();
        let size = text.len() as u64;
        self.push(Direction::Info, kind, text, size, None);
    }

    fn frame(&self, direction: Direction, frame: &WsFrame) {
        match frame {
            WsFrame::Text(t) => self.push(direction, "text", t.clone(), t.len() as u64, None),
            WsFrame::Binary(b) => self.push(
                direction,
                "binary",
                STANDARD.encode(b),
                b.len() as u64,
                None,
            ),
            WsFrame::Ping(b) => self.push(
                direction,
                "ping",
                String::from_utf8_lossy(b).into_owned(),
                b.len() as u64,
                None,
            ),
            WsFrame::Pong(b) => self.push(
                direction,
                "pong",
                String::from_utf8_lossy(b).into_owned(),
                b.len() as u64,
                None,
            ),
        }
    }

    async fn wait_or_stopped(&self, delay: Duration) -> bool {
        if self.stopped.is_cancelled() {
            return true;
        }
        tokio::select! {
            _ = tokio::time::sleep(delay) => self.stopped.is_cancelled(),
            _ = self.stop_notify.notified() => true,
        }
    }

    fn stop(&self) {
        self.stopped.cancel();
        self.stop_notify.notify_waiters();
        if let Some(ws) = self.ws.lock().ok().and_then(|mut w| w.take()) {
            ws.close(1000, "");
        }
        if let Some(sub) = self.gql.lock().ok().and_then(|mut g| g.take()) {
            sub.stop();
        }
        if let Ok(mut tx) = self.grpc_tx.lock() {
            tx.take();
        }
    }
}

fn not_found(id: &str) -> CoreError {
    CoreError::new("stream.not_found", "the connection is closed").with_param("connectionId", id)
}

impl Streams {
    fn get(&self, id: &str) -> CoreResult<Arc<Connection>> {
        self.by_id
            .lock()
            .ok()
            .and_then(|m| m.get(id).cloned())
            .ok_or_else(|| not_found(id))
    }

    fn by_request(&self, request_id: Id) -> Option<Arc<Connection>> {
        self.by_id
            .lock()
            .ok()?
            .values()
            .find(|c| c.info().request_id == request_id)
            .cloned()
    }

    fn insert(&self, conn: Arc<Connection>) {
        if let Ok(mut m) = self.by_id.lock() {
            m.insert(conn.info().connection_id, conn);
        }
    }

    fn remove(&self, id: &str) -> Option<Arc<Connection>> {
        self.by_id.lock().ok()?.remove(id)
    }

    /// Stops every connection (shutdown).
    pub fn stop_all(&self) {
        if let Ok(mut m) = self.by_id.lock() {
            for c in m.values() {
                c.stop();
            }
            m.clear();
        }
    }
}

/// Whether a GraphQL document's first operation is a subscription.
fn is_subscription(query: &str, operation_name: &str) -> bool {
    // Strip comments and strings well enough to find operation keywords.
    let mut clean = String::new();
    for line in query.lines() {
        clean.push_str(line.split('#').next().unwrap_or(""));
        clean.push('\n');
    }
    let tokens: Vec<&str> = clean
        .split(|c: char| c.is_whitespace() || c == '{' || c == '(')
        .filter(|t| !t.is_empty())
        .collect();
    for (i, t) in tokens.iter().enumerate() {
        if matches!(*t, "query" | "mutation" | "subscription") {
            let name = tokens.get(i + 1).copied().unwrap_or("");
            if operation_name.trim().is_empty() || name == operation_name.trim() {
                return *t == "subscription";
            }
        }
    }
    false
}

/// `ws(s)://` → `http(s)://` for the network policy.
fn http_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        url.to_string()
    }
}

async fn ws_config(
    ctx: &AppContext,
    prepared: &PreparedRequest,
    subprotocols: Vec<String>,
) -> CoreResult<WsConfig> {
    let url = ws::normalize_url(&prepared.url);
    let connect_addr = match &ctx.network_policy {
        Some(policy) => policy.check_target(&http_url(&url)).await?,
        None => None,
    };
    Ok(WsConfig {
        url: url.clone(),
        // The library sets Host and the WebSocket headers itself.
        headers: prepared
            .headers
            .iter()
            .filter(|(k, _)| {
                !k.eq_ignore_ascii_case("host")
                    && !k.eq_ignore_ascii_case("content-type")
                    && !k.to_ascii_lowercase().starts_with("sec-websocket-")
            })
            .cloned()
            .collect(),
        subprotocols,
        tls: super::http::stream_tls(ctx, &http_url(&url), &prepared.settings).await?,
        connect_timeout: Duration::from_millis(
            prepared
                .settings
                .connect_timeout_ms
                .unwrap_or(30_000)
                .max(1),
        ),
        connect_addr,
        ..WsConfig::default()
    })
}

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
pub struct StreamConnectInput {
    pub request_id: Id,
    /// Values for `{% prompt %}` tags, by key.
    #[serde(default)]
    #[ts(optional)]
    pub prompts: Option<HashMap<String, String>>,
}

command!(
    StreamConnect,
    "stream.connect",
    StreamConnectInput,
    StreamInfo,
    |ctx, input| {
        // One connection per request: a new connect replaces the old one.
        if let Some(old) = ctx.streams.by_request(input.request_id) {
            old.stop();
            ctx.streams.remove(&old.info().connection_id);
        }
        let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
        let node = tree.get_node(input.request_id).await?;
        let prompts = input.prompts.unwrap_or_default();
        let (prepared, workspace_id) =
            super::http::prepare_stored(ctx, input.request_id, &prompts, &[]).await?;
        let (kind, subprotocols, auto_reconnect, reconnect_delay) = match &node.data {
            NodeData::WebSocket(w) => (
                StreamKind::WebSocket,
                w.subprotocols.clone(),
                w.auto_reconnect,
                w.reconnect_delay_ms,
            ),
            NodeData::Request(r) if r.body.kind == BodyKind::Graphql => {
                if !is_subscription(&r.body.graphql.query, &r.body.graphql.operation_name) {
                    return Err(CoreError::new(
                        "stream.not_streaming",
                        "only GraphQL subscriptions are streamed; send queries and mutations",
                    ));
                }
                (StreamKind::GraphqlSubscription, Vec::new(), false, 0)
            }
            NodeData::Request(r) if r.event_stream => {
                (StreamKind::Sse, Vec::new(), true, SSE_RETRY_MS)
            }
            NodeData::Grpc(_) => (StreamKind::Grpc, Vec::new(), false, 0),
            _ => {
                return Err(CoreError::new(
                    "stream.not_streaming",
                    "this request is not a stream; send it instead",
                ));
            }
        };
        let conn = Arc::new(Connection {
            info: Mutex::new(StreamInfo {
                connection_id: Id::new().to_string(),
                request_id: input.request_id,
                kind,
                status: StreamStatus::Connecting,
                headers: Vec::new(),
                protocol: None,
                url: prepared.url.clone(),
                opened_at: now_ms(),
            }),
            log: Mutex::new(VecDeque::new()),
            seq: AtomicU64::new(0),
            stopped: CancelToken::new(),
            stop_notify: Notify::new(),
            ws: Mutex::new(None),
            gql: Mutex::new(None),
            grpc_tx: Mutex::new(None),
            grpc_input: Mutex::new(None),
            events: ctx.events.clone(),
        });
        ctx.streams.insert(conn.clone());
        conn.set_status(StreamStatus::Connecting);
        let result = match kind {
            StreamKind::WebSocket => {
                let config = ws_config(ctx, &prepared, subprotocols).await;
                match config {
                    Ok(config) => {
                        start_ws(conn.clone(), config, auto_reconnect, reconnect_delay).await
                    }
                    Err(e) => Err(e),
                }
            }
            StreamKind::Sse => start_sse(ctx, conn.clone(), prepared, workspace_id).await,
            StreamKind::GraphqlSubscription => start_graphql(ctx, conn.clone(), prepared).await,
            StreamKind::Grpc => start_grpc(ctx, conn.clone(), input.request_id).await,
        };
        if let Err(e) = result {
            conn.info_message("error", e.message.clone());
            conn.set_status(StreamStatus::Closed);
            ctx.streams.remove(&conn.info().connection_id);
            return Err(e);
        }
        Ok(conn.info())
    }
);

/// Connects; reconnects in the background when enabled and the server
/// drops the connection.
async fn start_ws(
    conn: Arc<Connection>,
    config: WsConfig,
    auto_reconnect: bool,
    delay_ms: u64,
) -> CoreResult<()> {
    let (handshake, sender, events) = ws::connect(config.clone()).await?;
    on_ws_open(&conn, &handshake, sender);
    tokio::spawn(async move {
        let mut events = events;
        loop {
            let clean = pump_ws(&conn, &mut events).await;
            if conn.stopped.is_cancelled() || !auto_reconnect || clean {
                break;
            }
            // Reconnect until it works or the user disconnects.
            let mut attempt = 0u32;
            let reconnected = loop {
                attempt += 1;
                conn.set_status(StreamStatus::Reconnecting);
                conn.info_message("reconnect", format!("reconnecting (attempt {attempt})"));
                if conn
                    .wait_or_stopped(Duration::from_millis(delay_ms.max(100)))
                    .await
                {
                    break None;
                }
                match ws::connect(config.clone()).await {
                    Ok((handshake, sender, ev)) => {
                        on_ws_open(&conn, &handshake, sender);
                        break Some(ev);
                    }
                    Err(e) => conn.info_message("error", e.message),
                }
            };
            match reconnected {
                Some(ev) => events = ev,
                None => break,
            }
        }
        conn.set_status(StreamStatus::Closed);
    });
    Ok(())
}

fn on_ws_open(conn: &Connection, handshake: &ws::WsHandshake, sender: WsSender) {
    if let Ok(mut info) = conn.info.lock() {
        info.headers = handshake.headers.clone();
        info.protocol = handshake.protocol.clone();
        info.url = handshake.url.clone();
    }
    if let Ok(mut ws) = conn.ws.lock() {
        *ws = Some(sender);
    }
    let text = match &handshake.protocol {
        Some(p) => format!("connected to {} ({p})", handshake.url),
        None => format!("connected to {}", handshake.url),
    };
    conn.info_message("open", text);
    conn.set_status(StreamStatus::Open);
}

/// Logs messages until the connection ends; true for a normal close.
async fn pump_ws(
    conn: &Connection,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<WsEvent>,
) -> bool {
    while let Some(event) = events.recv().await {
        match event {
            WsEvent::Message(frame) => conn.frame(Direction::Received, &frame),
            WsEvent::Closed {
                code,
                reason,
                error,
            } => {
                if let Ok(mut ws) = conn.ws.lock() {
                    *ws = None;
                }
                let mut text = match code {
                    Some(c) => format!("closed with code {c}"),
                    None => "connection lost".to_string(),
                };
                if !reason.is_empty() {
                    text.push_str(&format!(": {reason}"));
                }
                if let Some(e) = &error {
                    text.push_str(&format!(" ({e})"));
                }
                conn.info_message("close", text);
                return matches!(code, Some(1000 | 1001)) && error.is_none();
            }
        }
    }
    false
}

async fn start_sse(
    ctx: &AppContext,
    conn: Arc<Connection>,
    mut prepared: PreparedRequest,
    workspace_id: Id,
) -> CoreResult<()> {
    // A stream has no total timeout.
    prepared.settings.timeout_ms = Some(0);
    if !prepared
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("accept"))
    {
        prepared
            .headers
            .push(("Accept".into(), "text/event-stream".into()));
    }
    prepared
        .headers
        .push(("Cache-Control".into(), "no-cache".into()));
    let options = super::http::exec_options(ctx, workspace_id, &prepared.url).await?;
    let policy = ctx.network_policy.clone();
    tokio::spawn(async move {
        let parser = Arc::new(Mutex::new(SseParser::new()));
        let mut attempt = 0u32;
        loop {
            let mut request = prepared.clone();
            let last_id = parser
                .lock()
                .map(|p| p.last_event_id.clone())
                .unwrap_or_default();
            if !last_id.is_empty() {
                request
                    .headers
                    .retain(|(k, _)| !k.eq_ignore_ascii_case("last-event-id"));
                request.headers.push(("Last-Event-ID".into(), last_id));
            }
            // Status and type are checked when the head arrives.
            let verdict: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
            let sink_conn = conn.clone();
            let sink_parser = parser.clone();
            let sink_verdict = verdict.clone();
            let mut options = options.clone();
            let cancel = CancelToken::new();
            let sink_cancel = cancel.clone();
            options.body_sink = Some(Arc::new(move |event: BodyEvent<'_>| match event {
                BodyEvent::Head(lines) => {
                    let status: u16 = lines
                        .first()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let content_type = lines
                        .iter()
                        .filter_map(|l| l.split_once(':'))
                        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-type"))
                        .map(|(_, v)| v.trim().to_ascii_lowercase())
                        .unwrap_or_default();
                    let ok = status == 200 && content_type.starts_with("text/event-stream");
                    if let Ok(mut v) = sink_verdict.lock() {
                        *v = Some(if ok {
                            Ok(())
                        } else {
                            Err(format!("the server answered {status} {content_type}"))
                        });
                    }
                    if ok {
                        if let Ok(mut info) = sink_conn.info.lock() {
                            info.headers = lines
                                .iter()
                                .skip(1)
                                .filter_map(|l| l.split_once(':'))
                                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                                .collect();
                        }
                        sink_conn.info_message("open", format!("connected ({status})"));
                        sink_conn.set_status(StreamStatus::Open);
                    } else {
                        sink_cancel.cancel();
                    }
                }
                BodyEvent::Data(bytes) => {
                    let events = sink_parser
                        .lock()
                        .map(|mut p| p.feed(bytes))
                        .unwrap_or_default();
                    for e in events {
                        let size = e.data.len() as u64;
                        sink_conn.push(
                            Direction::Received,
                            "event",
                            e.data,
                            size,
                            Some((e.event, e.id)),
                        );
                    }
                }
            }));
            // Stop the transfer when the user disconnects.
            let watcher_conn = conn.clone();
            let watcher_cancel = cancel.clone();
            let watcher = tokio::spawn(async move {
                watcher_conn.stop_notify.notified().await;
                watcher_cancel.cancel();
            });
            if conn.stopped.is_cancelled() {
                cancel.cancel();
            }
            let result = match &policy {
                Some(p) => {
                    reqly_http::policy::send_checked(request, options, cancel, None, p).await
                }
                None => reqly_http::engine::execute(request, options, cancel, None).await,
            };
            watcher.abort();
            if let Ok(mut p) = parser.lock() {
                p.finish();
            }
            if conn.stopped.is_cancelled() {
                conn.info_message("close", "disconnected");
                break;
            }
            let verdict = verdict.lock().ok().and_then(|v| v.clone());
            match (&result, verdict) {
                // 204 or a wrong type ends the stream for good (WHATWG).
                (_, Some(Err(why))) => {
                    conn.info_message("error", why);
                    break;
                }
                (Ok(res), None) if res.status == 204 => {
                    conn.info_message("close", "the server ended the stream (204)");
                    break;
                }
                (Ok(res), None) => {
                    conn.info_message(
                        "error",
                        format!("the server answered {} without an event stream", res.status),
                    );
                    break;
                }
                (Err(e), None) => conn.info_message("error", e.message.clone()),
                (Ok(_), Some(Ok(()))) => conn.info_message("close", "the server closed the stream"),
                (Err(e), Some(Ok(()))) => {
                    conn.info_message("close", format!("the stream broke: {}", e.message))
                }
            }
            attempt += 1;
            let retry = parser
                .lock()
                .ok()
                .and_then(|p| p.retry)
                .unwrap_or(SSE_RETRY_MS);
            conn.set_status(StreamStatus::Reconnecting);
            conn.info_message(
                "reconnect",
                format!("reconnecting in {retry} ms (attempt {attempt})"),
            );
            if conn.wait_or_stopped(Duration::from_millis(retry)).await {
                conn.info_message("close", "disconnected");
                break;
            }
            conn.set_status(StreamStatus::Connecting);
        }
        conn.set_status(StreamStatus::Closed);
    });
    Ok(())
}

async fn start_graphql(
    ctx: &AppContext,
    conn: Arc<Connection>,
    prepared: PreparedRequest,
) -> CoreResult<()> {
    let payload: serde_json::Value = match &prepared.body {
        reqly_http::prepare::PreparedBody::Bytes(b) => {
            serde_json::from_slice(b).unwrap_or_default()
        }
        _ => {
            // GET-style GraphQL: rebuild from the query string.
            let url = url::Url::parse(&prepared.url)
                .map_err(|e| CoreError::new("http.invalid_url", e.to_string()))?;
            let mut p = serde_json::Map::new();
            for (k, v) in url.query_pairs() {
                let value = if k == "variables" {
                    serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v.to_string()))
                } else {
                    serde_json::Value::String(v.to_string())
                };
                p.insert(k.into_owned(), value);
            }
            serde_json::Value::Object(p)
        }
    };
    let mut ws_prepared = prepared.clone();
    if let Ok(mut url) = url::Url::parse(&ws_prepared.url) {
        url.set_query(None);
        ws_prepared.url = url.to_string();
    }
    let config = ws_config(ctx, &ws_prepared, Vec::new()).await?;
    // Headers also go into `connection_init`, where many servers read auth.
    let init: serde_json::Map<String, serde_json::Value> = config
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let init = (!init.is_empty()).then_some(serde_json::Value::Object(init));
    let size = payload.to_string().len() as u64;
    let (subscription, mut events) = graphql_ws::subscribe(config, init, payload.clone()).await?;
    if let Ok(mut g) = conn.gql.lock() {
        *g = Some(subscription);
    }
    conn.push(
        Direction::Sent,
        "data",
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
        size,
        None,
    );
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                GqlEvent::Started { protocol } => {
                    if let Ok(mut info) = conn.info.lock() {
                        info.protocol = Some(protocol.clone());
                    }
                    conn.info_message("open", format!("subscribed ({protocol})"));
                    conn.set_status(StreamStatus::Open);
                }
                GqlEvent::Next(v) => {
                    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                    let size = text.len() as u64;
                    conn.push(Direction::Received, "data", text, size, None);
                }
                GqlEvent::Error(v) => conn.info_message("error", v.to_string()),
                GqlEvent::Complete => conn.info_message("close", "the subscription completed"),
                GqlEvent::Closed {
                    code,
                    reason,
                    error,
                } => {
                    let mut text = format!(
                        "closed{}",
                        code.map(|c| format!(" with code {c}")).unwrap_or_default()
                    );
                    if !reason.is_empty() {
                        text.push_str(&format!(": {reason}"));
                    }
                    if let Some(e) = error {
                        text.push_str(&format!(" ({e})"));
                    }
                    conn.info_message("close", text);
                    break;
                }
            }
        }
        conn.set_status(StreamStatus::Closed);
    });
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreamSendInput {
    pub connection_id: String,
    pub format: MessageFormat,
    /// Templates are rendered; binary messages are base64.
    pub body: String,
}

command!(
    StreamSend,
    "stream.send",
    StreamSendInput,
    StreamMessage,
    |ctx, input| {
        let conn = ctx.streams.get(&input.connection_id)?;
        let request_id = conn.info().request_id;
        if conn.info().kind == StreamKind::Grpc {
            return send_grpc(ctx, &conn, request_id, &input.body).await;
        }
        let sender = conn
            .ws
            .lock()
            .ok()
            .and_then(|w| w.clone())
            .ok_or_else(|| CoreError::new("stream.not_open", "the connection is not open"))?;
        let body = super::http::render_in_request(ctx, request_id, &input.body).await?;
        let frame = match input.format {
            MessageFormat::Text => WsFrame::Text(body),
            MessageFormat::Json => {
                let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
                    CoreError::new("stream.invalid_json", e.to_string())
                        .with_param("message", e.to_string())
                })?;
                WsFrame::Text(value.to_string())
            }
            MessageFormat::Binary => {
                WsFrame::Binary(STANDARD.decode(body.trim()).map_err(|e| {
                    CoreError::new("stream.invalid_base64", e.to_string())
                        .with_param("message", e.to_string())
                })?)
            }
        };
        sender.send(frame.clone())?;
        conn.frame(Direction::Sent, &frame);
        conn.log
            .lock()
            .ok()
            .and_then(|l| l.back().cloned())
            .ok_or_else(|| CoreError::internal("log"))
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreamIdInput {
    pub connection_id: String,
}

command!(
    StreamDisconnect,
    "stream.disconnect",
    StreamIdInput,
    Empty,
    |ctx, input| {
        let conn = ctx.streams.get(&input.connection_id)?;
        conn.stop();
        // Keep the log readable until the next connect; the status says closed.
        if conn.info().kind == StreamKind::WebSocket {
            // The close handshake reports the final status.
        } else {
            conn.set_status(StreamStatus::Closed);
        }
        Ok(Empty {})
    }
);

command!(
    StreamList,
    "stream.list",
    Empty,
    Vec<StreamInfo>,
    |ctx, _input| {
        let mut list: Vec<StreamInfo> = ctx
            .streams
            .by_id
            .lock()
            .map(|m| m.values().map(|c| c.info()).collect())
            .unwrap_or_default();
        list.sort_by_key(|i| i.opened_at);
        Ok(list)
    }
);

command!(
    StreamLog,
    "stream.log",
    StreamIdInput,
    Vec<StreamMessage>,
    |ctx, input| {
        let conn = ctx.streams.get(&input.connection_id)?;
        Ok(conn
            .log
            .lock()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default())
    }
);

command!(
    StreamClearLog,
    "stream.clearLog",
    StreamIdInput,
    Empty,
    |ctx, input| {
        let conn = ctx.streams.get(&input.connection_id)?;
        if let Ok(mut l) = conn.log.lock() {
            l.clear();
        }
        Ok(Empty {})
    }
);

async fn start_grpc(ctx: &AppContext, conn: Arc<Connection>, request_id: Id) -> CoreResult<()> {
    use reqly_protocols::grpc;
    let call = super::grpc::prepare_call(ctx, request_id).await?;
    let channel = grpc::connect(&call.target, call.channel_options.clone()).await?;
    let pool = super::grpc::descriptor_pool(ctx, &call, Some(channel.clone())).await?;
    if call.request.service.is_empty() || call.request.method.is_empty() {
        return Err(CoreError::new(
            "grpc.no_method",
            "choose a service and a method",
        ));
    }
    let method = grpc::find_method(&pool, &call.request.service, &call.request.method)?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let client_streaming = method.is_client_streaming();
    if let Ok(mut info) = conn.info.lock() {
        info.url = format!(
            "{}://{}:{}/{}/{}",
            if call.target.tls { "grpcs" } else { "grpc" },
            call.target.host,
            call.target.port,
            call.request.service,
            call.request.method
        );
    }
    if client_streaming {
        // Messages come from stream.send; stream.end half-closes.
        if let Ok(mut g) = conn.grpc_tx.lock() {
            *g = Some(tx);
        }
        if let Ok(mut d) = conn.grpc_input.lock() {
            *d = Some(method.input());
        }
    } else {
        let message = grpc::from_json(&method.input(), &call.message)?;
        conn.push(
            Direction::Sent,
            "data",
            grpc::to_json(&message),
            message_size(&message),
            None,
        );
        let _ = tx.send(message);
        drop(tx);
    }
    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    // The call runs in the background: for client streaming the server
    // answers (even its headers) only after the client side ends.
    let metadata = call.metadata.clone();
    let deadline = call.deadline;
    conn.info_message("open", format!("calling {}", method.full_name()));
    conn.set_status(StreamStatus::Open);
    tokio::spawn(async move {
        let started = tokio::select! {
            r = grpc::call(channel, &method, &metadata, stream, deadline) => Some(r),
            _ = conn.stop_notify.notified() => None,
        };
        match started {
            None => conn.info_message("close", "cancelled"),
            Some(Err(status)) => grpc_status(&conn, &status),
            Some(Ok(mut response)) => {
                if let Ok(mut info) = conn.info.lock() {
                    info.headers = response.headers.clone();
                }
                conn.events
                    .publish(Event::new("stream.status", conn.info()));
                loop {
                    tokio::select! {
                        message = response.messages.message() => match message {
                            Ok(Some(m)) => {
                                let size = message_size(&m);
                                conn.push(Direction::Received, "data", grpc::to_json(&m), size, None);
                            }
                            Ok(None) => {
                                let trailers = response.messages.trailers().await.ok().flatten();
                                let pairs = trailers.as_ref().map(grpc::metadata_pairs).unwrap_or_default();
                                let extra: Vec<String> = pairs
                                    .iter()
                                    .filter(|(k, _)| !k.starts_with("grpc-"))
                                    .map(|(k, v)| format!("{k}: {v}"))
                                    .collect();
                                let mut text = "status OK (0)".to_string();
                                if !extra.is_empty() {
                                    text.push_str(&format!("; trailers: {}", extra.join(", ")));
                                }
                                conn.info_message("close", text);
                                break;
                            }
                            Err(status) => {
                                grpc_status(&conn, &status);
                                break;
                            }
                        },
                        _ = conn.stop_notify.notified() => {
                            conn.info_message("close", "cancelled");
                            break;
                        }
                    }
                }
            }
        }
        if let Ok(mut g) = conn.grpc_tx.lock() {
            g.take();
        }
        conn.set_status(StreamStatus::Closed);
    });
    Ok(())
}

fn message_size(m: &prost_reflect::DynamicMessage) -> u64 {
    prost::Message::encoded_len(m) as u64
}

fn grpc_status(conn: &Connection, status: &tonic::Status) {
    let code = status.code();
    conn.info_message(
        "error",
        format!(
            "status {} ({}): {}",
            reqly_protocols::grpc::code_name(code),
            code as i32,
            status.message()
        ),
    );
}

async fn send_grpc(
    ctx: &AppContext,
    conn: &Connection,
    request_id: Id,
    body: &str,
) -> CoreResult<StreamMessage> {
    let tx = conn
        .grpc_tx
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| {
            CoreError::new("stream.not_open", "this call does not take more messages")
        })?;
    let desc = conn
        .grpc_input
        .lock()
        .ok()
        .and_then(|d| d.clone())
        .ok_or_else(|| {
            CoreError::new("stream.not_open", "this call does not take more messages")
        })?;
    let body = super::http::render_in_request(ctx, request_id, body).await?;
    let message = reqly_protocols::grpc::from_json(&desc, &body)?;
    let size = message_size(&message);
    let json = reqly_protocols::grpc::to_json(&message);
    tx.send(message)
        .map_err(|_| CoreError::new("stream.not_open", "the call has ended"))?;
    conn.push(Direction::Sent, "data", json, size, None);
    conn.log
        .lock()
        .ok()
        .and_then(|l| l.back().cloned())
        .ok_or_else(|| CoreError::internal("log"))
}

pub struct StreamEnd;

impl Command<AppContext> for StreamEnd {
    /// Ends the client side of a gRPC stream (the server then answers).
    const NAME: &'static str = "stream.end";
    type Input = StreamIdInput;
    type Output = Empty;

    async fn run(ctx: &AppContext, input: StreamIdInput) -> CoreResult<Empty> {
        let conn = ctx.streams.get(&input.connection_id)?;
        let had = conn
            .grpc_tx
            .lock()
            .ok()
            .and_then(|mut g| g.take())
            .is_some();
        if had {
            conn.info_message("end", "sent all messages");
        }
        Ok(Empty {})
    }
}

#[cfg(test)]
mod tests {
    use super::is_subscription;

    #[test]
    fn detects_subscriptions() {
        assert!(is_subscription("subscription { counter }", ""));
        assert!(is_subscription("# comment query\nsubscription S { a }", ""));
        assert!(!is_subscription("query { a }", ""));
        assert!(!is_subscription("{ a }", ""));
        assert!(is_subscription("query A { a } subscription B { b }", "B"));
        assert!(!is_subscription("query A { a } subscription B { b }", "A"));
    }
}
