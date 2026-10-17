//! OAuth 2.0: tokens per node (the request or folder that defines the auth),
//! automatic grants (client credentials, password, refresh) and interactive
//! flows (authorization code with PKCE, implicit, device code).

use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use reqly_core::model::{
    Auth, Body, BodyKind, HttpRequest, KeyValue, OAuth2ClientAuth, OAuth2Config, OAuth2Grant,
    RequestSettings,
};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::{CancelToken, ExecOptions};
use reqly_storage::oauth2::{OAuth2Repo, StoredToken};
use reqly_storage::tree::TreeRepo;
use reqly_template::{RenderOptions, Renderer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::Digest;
use ts_rs::TS;

use super::template::{Scope, SendHost};
use super::{AppContext, Empty};
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<OAuth2Status>()
        .register::<OAuth2Fetch>()
        .register::<OAuth2Clear>()
        .register::<OAuth2Authorize>()
        .register::<OAuth2Complete>()
        .register::<OAuth2DeviceStart>()
        .register::<OAuth2DevicePoll>();
}

/// Used when the config has no redirect URL: the desktop window catches it
/// before it loads; the web UI passes its own callback page.
pub const DEFAULT_REDIRECT: &str = "http://127.0.0.1/oauth2/callback";
/// Tokens this close to expiry are refreshed first.
const EXPIRY_MARGIN_MS: i64 = 30_000;

/// An interactive flow waiting for the user.
#[derive(Debug, Clone)]
pub struct PendingFlow {
    pub node_id: Id,
    pub config: OAuth2Config,
    pub state: String,
    pub verifier: String,
    pub redirect_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub created_ms: i64,
}

fn oauth_error(code: &str, message: impl Into<String>) -> CoreError {
    CoreError::new(code, message.into())
}

fn random(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS randomness");
    URL_SAFE_NO_PAD.encode(buf)
}

/// Settings that identify a token; a change makes the stored token stale.
fn config_hash(c: &OAuth2Config) -> String {
    let key = format!(
        "{:?}|{}|{}|{}|{}|{}|{}|{}",
        c.grant, c.auth_url, c.token_url, c.client_id, c.scope, c.audience, c.resource, c.username
    );
    let digest = sha2::Sha256::digest(key.as_bytes());
    digest.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// The rendered OAuth 2.0 config of a request or folder.
async fn node_config(ctx: &AppContext, node_id: Id) -> CoreResult<OAuth2Config> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(node_id).await?;
    let auth = node.data.auth().clone();
    let Auth::OAuth2(config) = auth else {
        return Err(oauth_error(
            "auth.oauth2_not_configured",
            "this item does not use OAuth 2.0",
        ));
    };
    let scope = Scope::load(ctx, node.collection_id, Some(node_id)).await?;
    let prompts = HashMap::new();
    let host = SendHost {
        ctx,
        scope: &scope,
        prompt_values: &prompts,
        chain: &[],
    };
    let options = RenderOptions {
        locale: super::template::faker_locale(ctx, node.collection_id).await,
        ..RenderOptions::default()
    };
    let renderer = Renderer::new(&scope.vars, &host, options);
    let mut value =
        serde_json::to_value(&config).map_err(|e| CoreError::internal(e.to_string()))?;
    if let Value::Object(map) = &mut value {
        for v in map.values_mut() {
            if let Value::String(s) = v
                && reqly_template::has_template(s)
            {
                *s = renderer.render(s).await?;
            }
        }
    }
    serde_json::from_value(value).map_err(|e| CoreError::internal(e.to_string()))
}

/// POSTs a form to a token endpoint and parses the token response.
async fn token_request(
    ctx: &AppContext,
    config: &OAuth2Config,
    url: &str,
    mut form: Vec<(String, String)>,
) -> CoreResult<Value> {
    if url.trim().is_empty() {
        return Err(oauth_error("auth.oauth2_config", "the token URL is empty"));
    }
    let mut headers = vec![KeyValue::new("Accept", "application/json")];
    let public = config.client_secret.is_empty();
    match config.client_auth {
        OAuth2ClientAuth::Basic if !public => {
            let enc =
                |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
            headers.push(KeyValue::new(
                "Authorization",
                format!(
                    "Basic {}",
                    STANDARD.encode(format!(
                        "{}:{}",
                        enc(&config.client_id),
                        enc(&config.client_secret)
                    ))
                ),
            ));
        }
        _ => {
            form.push(("client_id".into(), config.client_id.clone()));
            if !public {
                form.push(("client_secret".into(), config.client_secret.clone()));
            }
        }
    }
    let request = HttpRequest {
        method: "POST".into(),
        url: url.trim().into(),
        headers,
        body: Body {
            kind: BodyKind::FormUrlencoded,
            fields: form.iter().map(|(k, v)| KeyValue::new(k, v)).collect(),
            ..Body::default()
        },
        settings: RequestSettings {
            timeout_ms: Some(30_000),
            ..RequestSettings::default()
        },
        ..HttpRequest::default()
    };
    let prepared = reqly_http::prepare::prepare(&request, &[])?;
    let response = match &ctx.network_policy {
        Some(policy) => {
            reqly_http::policy::send_checked(
                prepared,
                ExecOptions::default(),
                CancelToken::new(),
                None,
                policy,
            )
            .await?
        }
        None => {
            reqly_http::engine::execute(prepared, ExecOptions::default(), CancelToken::new(), None)
                .await?
        }
    };
    let bytes = response
        .body
        .to_vec()
        .map_err(|e| CoreError::new("http.io", e.to_string()))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // Most providers answer JSON; some (GitHub) a form-encoded body.
    let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
        Value::Object(
            text.split('&')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_string(), Value::String(percent_decode(v))))
                .collect(),
        )
    });
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let description = value
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(
            oauth_error("auth.oauth2_error", format!("{error}: {description}"))
                .with_param("error", error.to_string())
                .with_param("description", description.to_string())
                .with_param("status", u64::from(response.status)),
        );
    }
    if !(200..300).contains(&response.status) || value.get("access_token").is_none() {
        return Err(oauth_error(
            "auth.oauth2_error",
            format!("the token endpoint answered {}", response.status),
        )
        .with_param("error", "invalid_response")
        .with_param("description", text.chars().take(300).collect::<String>())
        .with_param("status", u64::from(response.status)));
    }
    Ok(value)
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

fn stored_from(
    value: Value,
    config: &OAuth2Config,
    now_ms: i64,
    previous_refresh: &str,
) -> StoredToken {
    let get = |k: &str| match value.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let expires_at = value
        .get("expires_in")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .map(|secs| now_ms + secs * 1000);
    let refresh = get("refresh_token");
    StoredToken {
        access_token: get("access_token"),
        refresh_token: if refresh.is_empty() {
            previous_refresh.to_string()
        } else {
            refresh
        },
        id_token: get("id_token"),
        token_type: if get("token_type").is_empty() {
            "Bearer".into()
        } else {
            get("token_type")
        },
        scope: if get("scope").is_empty() {
            config.scope.clone()
        } else {
            get("scope")
        },
        expires_at,
        created_at: now_ms,
        config_hash: config_hash(config),
        raw: value,
    }
}

fn scope_params(config: &OAuth2Config, form: &mut Vec<(String, String)>) {
    for (k, v) in [
        ("scope", &config.scope),
        ("audience", &config.audience),
        ("resource", &config.resource),
    ] {
        if !v.trim().is_empty() {
            form.push((k.into(), v.trim().to_string()));
        }
    }
}

async fn save(
    ctx: &AppContext,
    node_id: Id,
    value: Value,
    config: &OAuth2Config,
    previous_refresh: &str,
) -> CoreResult<StoredToken> {
    let now = ctx.clock.now().0;
    OAuth2Repo::new(&ctx.storage, ctx.clock.as_ref())
        .put(node_id, stored_from(value, config, now, previous_refresh))
        .await
}

/// Runs a grant that needs no user interaction.
async fn automatic_grant(
    ctx: &AppContext,
    node_id: Id,
    config: &OAuth2Config,
) -> CoreResult<StoredToken> {
    let mut form = Vec::new();
    match config.grant {
        OAuth2Grant::ClientCredentials => {
            form.push(("grant_type".into(), "client_credentials".into()))
        }
        OAuth2Grant::Password => {
            form.push(("grant_type".into(), "password".into()));
            form.push(("username".into(), config.username.clone()));
            form.push(("password".into(), config.password.clone()));
        }
        _ => {
            return Err(oauth_error(
                "auth.oauth2_authorize",
                "sign in to get a token (Get token in the Auth tab)",
            )
            .with_param("grant", format!("{:?}", config.grant)));
        }
    }
    scope_params(config, &mut form);
    let value = token_request(ctx, config, &config.token_url, form).await?;
    save(ctx, node_id, value, config, "").await
}

async fn refresh(
    ctx: &AppContext,
    node_id: Id,
    config: &OAuth2Config,
    token: &StoredToken,
) -> CoreResult<StoredToken> {
    let mut form = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("refresh_token".to_string(), token.refresh_token.clone()),
    ];
    if !config.scope.trim().is_empty() {
        form.push(("scope".into(), config.scope.trim().into()));
    }
    let value = token_request(ctx, config, &config.token_url, form).await?;
    save(ctx, node_id, value, config, &token.refresh_token).await
}

/// A valid token for `node_id`: stored, refreshed or fetched.
async fn valid_token(
    ctx: &AppContext,
    node_id: Id,
    config: &OAuth2Config,
) -> CoreResult<StoredToken> {
    let repo = OAuth2Repo::new(&ctx.storage, ctx.clock.as_ref());
    if let Some(token) = repo
        .get(node_id)
        .await?
        .filter(|t| t.config_hash == config_hash(config))
    {
        if !repo.is_expired(&token, EXPIRY_MARGIN_MS) {
            return Ok(token);
        }
        if config.auto_refresh && !token.refresh_token.is_empty() {
            match refresh(ctx, node_id, config, &token).await {
                Ok(t) => return Ok(t),
                Err(e) => tracing::warn!(error = %e, "OAuth 2.0 refresh failed"),
            }
        }
    }
    automatic_grant(ctx, node_id, config).await
}

/// The value to send for a request whose auth `config` belongs to `owner`.
pub async fn access_token(
    ctx: &AppContext,
    owner: Id,
    config: &OAuth2Config,
) -> CoreResult<String> {
    let token = valid_token(ctx, owner, config).await?;
    Ok(if config.use_id_token && !token.id_token.is_empty() {
        token.id_token
    } else {
        token.access_token
    })
}

// ----- commands ----------------------------------------------------------------------

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
pub struct OAuth2NodeInput {
    /// The request or folder that defines the OAuth 2.0 auth.
    pub node_id: Id,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2TokenInfo {
    /// `none`, `valid`, `expired` or `stale` (settings changed since).
    pub status: String,
    pub access_token: String,
    pub id_token: String,
    pub has_refresh_token: bool,
    pub token_type: String,
    pub scope: String,
    /// Unix milliseconds.
    pub expires_at: Option<i64>,
    pub obtained_at: Option<i64>,
}

async fn info(
    ctx: &AppContext,
    node_id: Id,
    config: Option<&OAuth2Config>,
) -> CoreResult<OAuth2TokenInfo> {
    let repo = OAuth2Repo::new(&ctx.storage, ctx.clock.as_ref());
    let Some(token) = repo.get(node_id).await? else {
        return Ok(OAuth2TokenInfo {
            status: "none".into(),
            access_token: String::new(),
            id_token: String::new(),
            has_refresh_token: false,
            token_type: String::new(),
            scope: String::new(),
            expires_at: None,
            obtained_at: None,
        });
    };
    let status = if config.is_some_and(|c| config_hash(c) != token.config_hash) {
        "stale"
    } else if repo.is_expired(&token, 0) {
        "expired"
    } else {
        "valid"
    };
    Ok(OAuth2TokenInfo {
        status: status.into(),
        has_refresh_token: !token.refresh_token.is_empty(),
        access_token: token.access_token,
        id_token: token.id_token,
        token_type: token.token_type,
        scope: token.scope,
        expires_at: token.expires_at,
        obtained_at: Some(token.created_at),
    })
}

command!(
    OAuth2Status,
    "oauth2.status",
    OAuth2NodeInput,
    OAuth2TokenInfo,
    |ctx, input| {
        let config = node_config(ctx, input.node_id).await.ok();
        info(ctx, input.node_id, config.as_ref()).await
    }
);

command!(
    OAuth2Fetch,
    "oauth2.fetch",
    OAuth2NodeInput,
    OAuth2TokenInfo,
    |ctx, input| {
        // Refresh when possible, otherwise run an automatic grant.
        let config = node_config(ctx, input.node_id).await?;
        let repo = OAuth2Repo::new(&ctx.storage, ctx.clock.as_ref());
        let stored = repo
            .get(input.node_id)
            .await?
            .filter(|t| t.config_hash == config_hash(&config));
        match stored {
            Some(token) if !token.refresh_token.is_empty() => {
                refresh(ctx, input.node_id, &config, &token).await?
            }
            _ => automatic_grant(ctx, input.node_id, &config).await?,
        };
        info(ctx, input.node_id, Some(&config)).await
    }
);

command!(
    OAuth2Clear,
    "oauth2.clear",
    OAuth2NodeInput,
    Empty,
    |ctx, input| {
        OAuth2Repo::new(&ctx.storage, ctx.clock.as_ref())
            .delete(input.node_id)
            .await?;
        Ok(Empty {})
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2AuthorizeInput {
    pub node_id: Id,
    /// Redirect URI when the config has none (the web UI's callback page).
    #[ts(optional)]
    pub default_redirect_uri: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2AuthorizeOutput {
    pub flow_id: String,
    /// Open this in a browser window.
    pub authorize_url: String,
    /// The flow is complete when the window reaches this URI.
    pub redirect_uri: String,
}

fn store_flow(ctx: &AppContext, flow: PendingFlow) -> String {
    let id = random(12);
    if let Ok(mut flows) = ctx.oauth2_flows.lock() {
        // Forget flows older than ten minutes.
        let now = flow.created_ms;
        flows.retain(|_, f| now - f.created_ms < 600_000);
        flows.insert(id.clone(), flow);
    }
    id
}

fn take_flow(ctx: &AppContext, flow_id: &str, remove: bool) -> CoreResult<PendingFlow> {
    let mut flows = ctx
        .oauth2_flows
        .lock()
        .map_err(|_| CoreError::internal("flows lock"))?;
    let flow = if remove {
        flows.remove(flow_id)
    } else {
        flows.get(flow_id).cloned()
    };
    flow.ok_or_else(|| oauth_error("auth.oauth2_flow", "the sign-in expired; start again"))
}

command!(
    OAuth2Authorize,
    "oauth2.authorize",
    OAuth2AuthorizeInput,
    OAuth2AuthorizeOutput,
    |ctx, input| {
        let config = node_config(ctx, input.node_id).await?;
        let response_type = match config.grant {
            OAuth2Grant::AuthorizationCode => "code",
            OAuth2Grant::Implicit => "token",
            _ => {
                return Err(oauth_error(
                    "auth.oauth2_config",
                    "this grant does not use a browser sign-in",
                ));
            }
        };
        if config.auth_url.trim().is_empty() {
            return Err(oauth_error(
                "auth.oauth2_config",
                "the authorization URL is empty",
            ));
        }
        let redirect_uri = [
            config.redirect_url.trim(),
            input.default_redirect_uri.as_deref().unwrap_or("").trim(),
            DEFAULT_REDIRECT,
        ]
        .into_iter()
        .find(|u| !u.is_empty())
        .unwrap_or(DEFAULT_REDIRECT)
        .to_string();
        let state = if config.state.trim().is_empty() {
            random(16)
        } else {
            config.state.trim().to_string()
        };
        let verifier = random(48);
        let mut url = url::Url::parse(config.auth_url.trim()).map_err(|e| {
            oauth_error(
                "auth.oauth2_config",
                format!("invalid authorization URL: {e}"),
            )
        })?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", response_type);
            q.append_pair("client_id", &config.client_id);
            q.append_pair("redirect_uri", &redirect_uri);
            q.append_pair("state", &state);
            for (k, v) in [
                ("scope", &config.scope),
                ("audience", &config.audience),
                ("resource", &config.resource),
            ] {
                if !v.trim().is_empty() {
                    q.append_pair(k, v.trim());
                }
            }
            if config.pkce && config.grant == OAuth2Grant::AuthorizationCode {
                q.append_pair(
                    "code_challenge",
                    &URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes())),
                );
                q.append_pair("code_challenge_method", "S256");
            }
        }
        let flow_id = store_flow(
            ctx,
            PendingFlow {
                node_id: input.node_id,
                config,
                state,
                verifier,
                redirect_uri: redirect_uri.clone(),
                device_code: String::new(),
                interval: 0,
                created_ms: ctx.clock.now().0,
            },
        );
        Ok(OAuth2AuthorizeOutput {
            flow_id,
            authorize_url: url.to_string(),
            redirect_uri,
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2CompleteInput {
    pub flow_id: String,
    /// The URL the provider redirected to (with `code` or a token fragment).
    pub callback_url: String,
}

command!(
    OAuth2Complete,
    "oauth2.complete",
    OAuth2CompleteInput,
    OAuth2TokenInfo,
    |ctx, input| {
        let flow = take_flow(ctx, &input.flow_id, true)?;
        let url = url::Url::parse(&input.callback_url)
            .map_err(|e| oauth_error("auth.oauth2_flow", format!("invalid callback URL: {e}")))?;
        // Implicit grants return the token in the fragment.
        let mut params: Map<String, Value> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect();
        if let Some(fragment) = url.fragment() {
            for (k, v) in url::form_urlencoded::parse(fragment.as_bytes()) {
                params.insert(k.into_owned(), Value::String(v.into_owned()));
            }
        }
        let get = |k: &str| {
            params
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        if !get("error").is_empty() {
            return Err(oauth_error(
                "auth.oauth2_error",
                format!("{}: {}", get("error"), get("error_description")),
            )
            .with_param("error", get("error"))
            .with_param("description", get("error_description")));
        }
        if get("state") != flow.state {
            return Err(oauth_error(
                "auth.oauth2_state",
                "the state did not match; the sign-in may have been tampered with",
            ));
        }
        let value = match flow.config.grant {
            OAuth2Grant::Implicit => Value::Object(params.clone()),
            _ => {
                let code = get("code");
                if code.is_empty() {
                    return Err(oauth_error(
                        "auth.oauth2_flow",
                        "the provider returned no code",
                    ));
                }
                let mut form = vec![
                    ("grant_type".to_string(), "authorization_code".to_string()),
                    ("code".to_string(), code),
                    ("redirect_uri".to_string(), flow.redirect_uri.clone()),
                ];
                if flow.config.pkce {
                    form.push(("code_verifier".into(), flow.verifier.clone()));
                }
                token_request(ctx, &flow.config, &flow.config.token_url, form).await?
            }
        };
        save(ctx, flow.node_id, value, &flow.config, "").await?;
        info(ctx, flow.node_id, Some(&flow.config)).await
    }
);

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2DeviceOutput {
    pub flow_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    /// Seconds between polls.
    pub interval: u64,
    pub expires_in: u64,
}

command!(
    OAuth2DeviceStart,
    "oauth2.deviceStart",
    OAuth2NodeInput,
    OAuth2DeviceOutput,
    |ctx, input| {
        let config = node_config(ctx, input.node_id).await?;
        if config.grant != OAuth2Grant::DeviceCode {
            return Err(oauth_error(
                "auth.oauth2_config",
                "this grant is not the device code grant",
            ));
        }
        if config.device_url.trim().is_empty() {
            return Err(oauth_error(
                "auth.oauth2_config",
                "the device authorization URL is empty",
            ));
        }
        let mut form = vec![("client_id".to_string(), config.client_id.clone())];
        if !config.scope.trim().is_empty() {
            form.push(("scope".into(), config.scope.trim().into()));
        }
        // The device endpoint is called like a token endpoint but without client auth.
        let public = OAuth2Config {
            client_secret: String::new(),
            client_auth: OAuth2ClientAuth::Body,
            ..config.clone()
        };
        let value = device_request(ctx, &public, &config.device_url, form).await?;
        let get = |k: &str| {
            value
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let num = |k: &str, d: u64| value.get(k).and_then(Value::as_u64).unwrap_or(d);
        let interval = num("interval", 5);
        let flow_id = store_flow(
            ctx,
            PendingFlow {
                node_id: input.node_id,
                config,
                state: String::new(),
                verifier: String::new(),
                redirect_uri: String::new(),
                device_code: get("device_code"),
                interval,
                created_ms: ctx.clock.now().0,
            },
        );
        Ok(OAuth2DeviceOutput {
            flow_id,
            user_code: get("user_code"),
            verification_uri: if get("verification_uri").is_empty() {
                get("verification_url")
            } else {
                get("verification_uri")
            },
            verification_uri_complete: get("verification_uri_complete"),
            interval,
            expires_in: num("expires_in", 600),
        })
    }
);

/// Like [`token_request`] but without requiring `access_token` in the answer.
async fn device_request(
    ctx: &AppContext,
    config: &OAuth2Config,
    url: &str,
    form: Vec<(String, String)>,
) -> CoreResult<Value> {
    let request = HttpRequest {
        method: "POST".into(),
        url: url.trim().into(),
        headers: vec![KeyValue::new("Accept", "application/json")],
        body: Body {
            kind: BodyKind::FormUrlencoded,
            fields: form.iter().map(|(k, v)| KeyValue::new(k, v)).collect(),
            ..Body::default()
        },
        ..HttpRequest::default()
    };
    let _ = config;
    let prepared = reqly_http::prepare::prepare(&request, &[])?;
    let response = match &ctx.network_policy {
        Some(policy) => {
            reqly_http::policy::send_checked(
                prepared,
                ExecOptions::default(),
                CancelToken::new(),
                None,
                policy,
            )
            .await?
        }
        None => {
            reqly_http::engine::execute(prepared, ExecOptions::default(), CancelToken::new(), None)
                .await?
        }
    };
    let bytes = response
        .body
        .to_vec()
        .map_err(|e| CoreError::new("http.io", e.to_string()))?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(oauth_error("auth.oauth2_error", error.to_string())
            .with_param("error", error.to_string()));
    }
    Ok(value)
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2DevicePollInput {
    pub flow_id: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OAuth2DevicePollOutput {
    /// `pending`, `slowDown` or `done`.
    pub status: String,
    #[ts(optional)]
    pub token: Option<OAuth2TokenInfo>,
}

command!(
    OAuth2DevicePoll,
    "oauth2.devicePoll",
    OAuth2DevicePollInput,
    OAuth2DevicePollOutput,
    |ctx, input| {
        let flow = take_flow(ctx, &input.flow_id, false)?;
        let form = vec![
            (
                "grant_type".to_string(),
                "urn:ietf:params:oauth:grant-type:device_code".to_string(),
            ),
            ("device_code".to_string(), flow.device_code.clone()),
        ];
        match token_request(ctx, &flow.config, &flow.config.token_url, form).await {
            Ok(value) => {
                take_flow(ctx, &input.flow_id, true)?;
                save(ctx, flow.node_id, value, &flow.config, "").await?;
                Ok(OAuth2DevicePollOutput {
                    status: "done".into(),
                    token: Some(info(ctx, flow.node_id, Some(&flow.config)).await?),
                })
            }
            Err(e)
                if e.params.get("error").and_then(Value::as_str)
                    == Some("authorization_pending") =>
            {
                Ok(OAuth2DevicePollOutput {
                    status: "pending".into(),
                    token: None,
                })
            }
            Err(e) if e.params.get("error").and_then(Value::as_str) == Some("slow_down") => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(OAuth2DevicePollOutput {
                    status: "slowDown".into(),
                    token: None,
                })
            }
            Err(e) => {
                take_flow(ctx, &input.flow_id, true).ok();
                Err(e)
            }
        }
    }
);
