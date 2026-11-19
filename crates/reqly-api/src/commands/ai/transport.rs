//! [`reqly_ai::Transport`] on the `reqly-http` engine: provider calls use
//! the network settings (proxy, TLS, CAs, client certificates) and the
//! server's network policy, like requests do.

use std::sync::Arc;

use reqly_ai::{BoxFuture, Chunk, ChunkSink, HttpCall, Transport};
use reqly_core::model::RequestSettings;
use reqly_core::{CoreError, CoreResult};
use reqly_http::engine::{BodyEvent, CancelToken, ExecOptions};
use reqly_http::prepare::{PreparedBody, PreparedRequest};

use super::super::AppContext;
use super::super::http::{
    HTTP_DEFAULTS_KEY, HTTP_TLS_KEY, TlsSettings, client_identity, execute, setting,
};

pub struct HttpTransport<'a> {
    ctx: &'a AppContext,
    cancel: CancelToken,
}

impl<'a> HttpTransport<'a> {
    /// Calls stop with `ai.cancelled` once `cancel` fires.
    pub fn new(ctx: &'a AppContext, cancel: CancelToken) -> Self {
        Self { ctx, cancel }
    }
}

/// The global network settings that apply to provider calls; the rest
/// (cookies, timeouts, per-request tweaks) does not.
fn settings(global: &RequestSettings) -> RequestSettings {
    RequestSettings {
        proxy: global.proxy.clone(),
        verify_tls: global.verify_tls,
        connect_timeout_ms: global.connect_timeout_ms,
        tls_min: global.tls_min,
        tls_max: global.tls_max,
        // An answer streams for as long as the model writes; Stop ends it.
        timeout_ms: Some(0),
        send_cookies: Some(false),
        store_cookies: Some(false),
        // Decompression would hold back the stream.
        compression: Some(false),
        ..RequestSettings::default()
    }
    .merge(&RequestSettings::defaults())
}

fn status_of(lines: &[String]) -> u16 {
    lines
        .first()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

impl Transport for HttpTransport<'_> {
    fn send(&self, call: HttpCall, sink: ChunkSink) -> BoxFuture<'_, CoreResult<()>> {
        Box::pin(async move {
            let global: RequestSettings = setting(self.ctx, HTTP_DEFAULTS_KEY).await?;
            let tls: TlsSettings = setting(self.ctx, HTTP_TLS_KEY).await?;
            let prepared = PreparedRequest {
                method: call.method.to_string(),
                url: call.url.clone(),
                headers: call.headers,
                body: call.body.map(PreparedBody::Bytes).unwrap_or_default(),
                settings: settings(&global),
                auth: None,
                engine_auth: None,
            };
            let options = ExecOptions {
                ca_pems: tls.ca_pems.clone(),
                client_identity: client_identity(&tls, &call.url)?,
                body_sink: Some(Arc::new(move |event: BodyEvent<'_>| match event {
                    BodyEvent::Head(lines) => sink(Chunk::Status(status_of(lines))),
                    BodyEvent::Data(bytes) => sink(Chunk::Data(bytes)),
                })),
                ..ExecOptions::default()
            };
            match execute(self.ctx, prepared, options, self.cancel.clone()).await {
                Ok(_) => Ok(()),
                Err(_) if self.cancel.is_cancelled() => {
                    Err(CoreError::new("ai.cancelled", "the answer was stopped"))
                }
                Err(e) => Err(e),
            }
        })
    }
}
