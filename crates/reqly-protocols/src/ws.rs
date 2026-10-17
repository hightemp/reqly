//! WebSocket client. A connection runs in its own task; the caller sends
//! through [`WsSender`] and receives [`WsEvent`]s from a channel.

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use reqly_core::{CoreError, CoreResult};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};

use crate::tls::{TlsOptions, client_config};

#[derive(Debug, Clone)]
pub struct WsConfig {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub subprotocols: Vec<String>,
    pub tls: TlsOptions,
    pub connect_timeout: Duration,
    /// Largest accepted message in bytes.
    pub max_message_size: usize,
    /// Connect to this address instead of resolving the host (checked by
    /// the server's network policy).
    pub connect_addr: Option<std::net::SocketAddr>,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: Vec::new(),
            subprotocols: Vec::new(),
            tls: TlsOptions {
                verify: true,
                ..TlsOptions::default()
            },
            connect_timeout: Duration::from_secs(30),
            max_message_size: 64 << 20,
            connect_addr: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsHandshake {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Negotiated subprotocol.
    pub protocol: Option<String>,
    /// The URL actually connected to (`ws(s)://`).
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsFrame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsEvent {
    Message(WsFrame),
    /// The connection ended; `code` is `None` without a close frame.
    Closed {
        code: Option<u16>,
        reason: String,
        error: Option<String>,
    },
}

enum Outgoing {
    Frame(WsFrame),
    Close(u16, String),
}

#[derive(Clone)]
pub struct WsSender(mpsc::UnboundedSender<Outgoing>);

impl WsSender {
    pub fn send(&self, frame: WsFrame) -> CoreResult<()> {
        self.0
            .send(Outgoing::Frame(frame))
            .map_err(|_| CoreError::new("ws.closed", "the connection is closed"))
    }

    pub fn close(&self, code: u16, reason: impl Into<String>) {
        let _ = self.0.send(Outgoing::Close(code, reason.into()));
    }
}

/// `http(s)://` → `ws(s)://`; no scheme → `ws://`.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if url.starts_with("ws://") || url.starts_with("wss://") {
        url.to_string()
    } else {
        format!("ws://{url}")
    }
}

fn ws_error(code: &str, message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new(code, message.clone()).with_param("message", message)
}

fn map_error(err: tungstenite::Error) -> CoreError {
    match err {
        tungstenite::Error::Http(response) => {
            let status = response.status().as_u16();
            let body = response
                .body()
                .as_ref()
                .map(|b| {
                    String::from_utf8_lossy(b)
                        .chars()
                        .take(500)
                        .collect::<String>()
                })
                .unwrap_or_default();
            ws_error(
                "ws.handshake",
                format!("the server answered {status} instead of switching protocols"),
            )
            .with_param("status", u64::from(status))
            .with_param("body", body)
        }
        tungstenite::Error::Io(e) => {
            // rustls failures surface as I/O errors.
            let tls = e
                .get_ref()
                .is_some_and(|inner| inner.downcast_ref::<rustls::Error>().is_some());
            ws_error(if tls { "http.tls" } else { "http.connect" }, e.to_string())
        }
        tungstenite::Error::Tls(e) => ws_error("http.tls", e.to_string()),
        tungstenite::Error::Url(e) => {
            ws_error("http.invalid_url", e.to_string()).with_param("url", "")
        }
        other => ws_error("ws.error", other.to_string()),
    }
}

/// Connects; events arrive on the returned receiver until `Closed`.
pub async fn connect(
    config: WsConfig,
) -> CoreResult<(WsHandshake, WsSender, mpsc::UnboundedReceiver<WsEvent>)> {
    let url = normalize_url(&config.url);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| ws_error("http.invalid_url", e.to_string()).with_param("url", url.clone()))?;
    for (k, v) in &config.headers {
        let name = http::HeaderName::from_bytes(k.as_bytes())
            .map_err(|_| ws_error("ws.invalid_header", format!("invalid header name {k}")))?;
        let value = http::HeaderValue::from_str(v)
            .map_err(|_| ws_error("ws.invalid_header", format!("invalid value of {k}")))?;
        request.headers_mut().append(name, value);
    }
    if !config.subprotocols.is_empty() {
        let value = config.subprotocols.join(", ");
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            http::HeaderValue::from_str(&value)
                .map_err(|_| ws_error("ws.invalid_header", "invalid subprotocol"))?,
        );
    }
    let connector = if url.starts_with("wss://") {
        Some(tokio_tungstenite::Connector::Rustls(client_config(
            &config.tls,
        )?))
    } else {
        None
    };
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(config.max_message_size))
        .max_frame_size(Some(config.max_message_size));
    let connecting = async {
        match config.connect_addr {
            Some(addr) => {
                let tcp = tokio::net::TcpStream::connect(addr)
                    .await
                    .map_err(|e| ws_error("http.connect", e.to_string()))?;
                let _ = tcp.set_nodelay(true);
                tokio_tungstenite::client_async_tls_with_config(
                    request,
                    tcp,
                    Some(ws_config),
                    connector,
                )
                .await
                .map_err(map_error)
            }
            None => tokio_tungstenite::connect_async_tls_with_config(
                request,
                Some(ws_config),
                true,
                connector,
            )
            .await
            .map_err(map_error),
        }
    };
    let (stream, response) = tokio::time::timeout(config.connect_timeout, connecting)
        .await
        .map_err(|_| ws_error("http.timeout", "connecting timed out"))??;
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let protocol = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let handshake = WsHandshake {
        status: response.status().as_u16(),
        headers,
        protocol,
        url,
    };

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outgoing>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<WsEvent>();
    let (mut sink, mut source) = stream.split();
    tokio::spawn(async move {
        let closed = |code, reason, error| WsEvent::Closed {
            code,
            reason,
            error,
        };
        // Our close frame, reported if the server does not answer with its own.
        let mut sent_close: Option<(u16, String)> = None;
        loop {
            tokio::select! {
                outgoing = out_rx.recv() => {
                    let result = match outgoing {
                        Some(Outgoing::Frame(WsFrame::Text(t))) => sink.send(Message::Text(t.into())).await,
                        Some(Outgoing::Frame(WsFrame::Binary(b))) => sink.send(Message::Binary(b.into())).await,
                        Some(Outgoing::Frame(WsFrame::Ping(b))) => sink.send(Message::Ping(b.into())).await,
                        Some(Outgoing::Frame(WsFrame::Pong(b))) => sink.send(Message::Pong(b.into())).await,
                        Some(Outgoing::Close(code, reason)) => {
                            sent_close = Some((code, reason.clone()));
                            let frame = CloseFrame { code: code.into(), reason: reason.into() };
                            sink.send(Message::Close(Some(frame))).await
                        }
                        // Every sender dropped: close normally.
                        None => {
                            let _ = sink.send(Message::Close(None)).await;
                            let _ = ev_tx.send(closed(Some(1000), String::new(), None));
                            return;
                        }
                    };
                    if let Err(e) = result {
                        let _ = ev_tx.send(closed(None, String::new(), Some(e.to_string())));
                        return;
                    }
                }
                incoming = source.next() => {
                    let event = match incoming {
                        Some(Ok(Message::Text(t))) => WsEvent::Message(WsFrame::Text(t.to_string())),
                        Some(Ok(Message::Binary(b))) => WsEvent::Message(WsFrame::Binary(b.to_vec())),
                        Some(Ok(Message::Ping(b))) => WsEvent::Message(WsFrame::Ping(b.to_vec())),
                        Some(Ok(Message::Pong(b))) => WsEvent::Message(WsFrame::Pong(b.to_vec())),
                        Some(Ok(Message::Close(frame))) => {
                            // tungstenite answers the close; wait for the stream to end.
                            let (code, reason) = frame
                                .map(|f| (Some(u16::from(f.code)), f.reason.to_string()))
                                .or_else(|| sent_close.clone().map(|(c, r)| (Some(c), r)))
                                .unwrap_or((None, String::new()));
                            let _ = ev_tx.send(closed(code, reason, None));
                            while let Some(Ok(_)) = source.next().await {}
                            return;
                        }
                        Some(Ok(Message::Frame(_))) => continue,
                        Some(Err(_)) | None if sent_close.is_some() => {
                            let (code, reason) = sent_close.take().unwrap_or_default();
                            let _ = ev_tx.send(closed(Some(code), reason, None));
                            return;
                        }
                        Some(Err(e)) => {
                            let _ = ev_tx.send(closed(None, String::new(), Some(e.to_string())));
                            return;
                        }
                        None => {
                            let _ = ev_tx.send(closed(None, String::new(), Some("the connection was closed".into())));
                            return;
                        }
                    };
                    if ev_tx.send(event).is_err() {
                        return;
                    }
                }
            }
        }
    });
    Ok((handshake, WsSender(out_tx), ev_rx))
}
