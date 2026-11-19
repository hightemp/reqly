//! AI settings (ADR-015): providers, their keys, the default model and the
//! tools the chat may use.

use std::collections::{BTreeMap, BTreeSet};

use reqly_core::{CoreError, CoreResult};
use reqly_storage::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::access::{self, Caller};
use crate::agent::{Group, catalog_tools};
use crate::commands::AppContext;
use crate::commands::mcp::CATALOG;

/// Settings keys; `settings.get`/`settings.set` refuse them.
pub const CONFIG_KEY: &str = "ai";
pub const KEYS_KEY: &str = "ai.keys";

/// Whether the generic settings commands must leave `key` alone.
pub fn reserved_key(key: &str) -> bool {
    key == CONFIG_KEY || key.starts_with("ai.")
}

/// Where a provider's defaults come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AiPreset {
    Openai,
    Openrouter,
    OllamaCloud,
    Ollama,
    Custom,
}

impl AiPreset {
    pub const ALL: [AiPreset; 5] = [
        AiPreset::Openai,
        AiPreset::Openrouter,
        AiPreset::OllamaCloud,
        AiPreset::Ollama,
        AiPreset::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AiPreset::Openai => "OpenAI",
            AiPreset::Openrouter => "OpenRouter",
            AiPreset::OllamaCloud => "Ollama Cloud",
            AiPreset::Ollama => "Ollama",
            AiPreset::Custom => "OpenAI-compatible",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            AiPreset::Openai => "https://api.openai.com/v1",
            AiPreset::Openrouter => "https://openrouter.ai/api/v1",
            AiPreset::OllamaCloud => "https://ollama.com/v1",
            AiPreset::Ollama => "http://localhost:11434/v1",
            AiPreset::Custom => "",
        }
    }

    pub fn key_required(self) -> bool {
        matches!(
            self,
            AiPreset::Openai | AiPreset::Openrouter | AiPreset::OllamaCloud
        )
    }

    /// Headers the preset always sends.
    fn headers(self) -> Vec<(String, String)> {
        match self {
            // https://openrouter.ai/docs/api-reference/overview#headers
            AiPreset::Openrouter => vec![
                (
                    "HTTP-Referer".into(),
                    "https://github.com/hightemp/reqly".into(),
                ),
                ("X-Title".into(), "Reqly".into()),
            ],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub preset: AiPreset,
    pub base_url: String,
    #[serde(default)]
    pub headers: Vec<AiHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiModelRef {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AiConfig {
    pub enabled: bool,
    pub providers: Vec<AiProvider>,
    pub default_model: Option<AiModelRef>,
    /// Explicit choices; a tool missing here is on.
    pub tools: BTreeMap<String, bool>,
    /// Groups whose changing tools run without asking.
    pub auto_approve: BTreeMap<Group, bool>,
}

impl AiConfig {
    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tools.get(name).copied().unwrap_or(true)
    }

    pub fn provider(&self, id: &str) -> Option<&AiProvider> {
        self.providers.iter().find(|p| p.id == id)
    }

    fn validate(&self) -> CoreResult<()> {
        let mut ids = BTreeSet::new();
        for p in &self.providers {
            if p.id.trim().is_empty() {
                return Err(invalid("provider id is empty"));
            }
            if !ids.insert(p.id.as_str()) {
                return Err(invalid("duplicate provider id").with_param("provider", p.id.as_str()));
            }
            if p.name.trim().is_empty() {
                return Err(invalid("provider name is empty").with_param("provider", p.id.as_str()));
            }
            let ok = url::Url::parse(p.base_url.trim())
                .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some());
            if !ok {
                return Err(invalid("base URL must be an http(s) URL")
                    .with_param("provider", p.id.as_str()));
            }
        }
        if let Some(m) = &self.default_model
            && self.provider(&m.provider_id).is_none()
        {
            return Err(invalid("default model of an unknown provider"));
        }
        if let Some(name) = self
            .tools
            .keys()
            .find(|n| !CATALOG.iter().any(|(c, _)| c == n))
        {
            return Err(invalid("unknown tool").with_param("tool", name.as_str()));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::new("ai.invalid_config", message)
}

pub async fn load_config(storage: &reqly_storage::Storage) -> CoreResult<AiConfig> {
    let value = SettingsRepo::new(storage).get(CONFIG_KEY).await?;
    Ok(value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

async fn load_keys(storage: &reqly_storage::Storage) -> CoreResult<BTreeMap<String, String>> {
    let value = SettingsRepo::new(storage).get(KEYS_KEY).await?;
    Ok(value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

async fn save_keys(ctx: &AppContext, keys: &BTreeMap<String, String>) -> CoreResult<()> {
    SettingsRepo::new(&ctx.storage)
        .set(
            KEYS_KEY,
            &serde_json::to_value(keys).map_err(|e| CoreError::internal(e.to_string()))?,
            ctx.clock.as_ref(),
        )
        .await
}

/// Whether the caller may change the AI settings.
pub fn can_edit(caller: Caller) -> bool {
    match caller {
        Caller::Local => true,
        Caller::User { admin, .. } => admin,
    }
}

/// The client settings of a configured provider, with its key.
pub(crate) async fn provider_client(
    ctx: &AppContext,
    config: &AiConfig,
    provider_id: &str,
) -> CoreResult<reqly_ai::Provider> {
    if !config.enabled {
        return Err(CoreError::new("ai.disabled", "the AI chat is turned off"));
    }
    let p = config
        .provider(provider_id)
        .ok_or_else(|| unknown(provider_id))?;
    let keys = load_keys(&ctx.storage).await?;
    let mut headers = p.preset.headers();
    headers.extend(
        p.headers
            .iter()
            .filter(|h| !h.name.trim().is_empty())
            .map(|h| (h.name.trim().to_string(), h.value.clone())),
    );
    Ok(reqly_ai::Provider {
        base_url: p.base_url.trim().to_string(),
        api_key: keys.get(provider_id).cloned(),
        headers,
    })
}

fn unknown(provider_id: &str) -> CoreError {
    CoreError::new("ai.unknown_provider", "no such provider").with_param("provider", provider_id)
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiPresetInfo {
    pub preset: AiPreset,
    pub name: String,
    pub base_url: String,
    pub key_required: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiProviderInfo {
    pub id: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiToolInfo {
    pub name: String,
    pub group: Group,
    pub enabled: bool,
    /// Reading tools never ask; others ask unless their group is
    /// auto-approved.
    pub needs_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiStatus {
    /// For users who may not edit, provider headers are left out.
    pub config: AiConfig,
    pub providers: Vec<AiProviderInfo>,
    pub tools: Vec<AiToolInfo>,
    pub presets: Vec<AiPresetInfo>,
    pub can_edit: bool,
}

/// Whether a tool runs only after the user allows it.
pub fn needs_confirmation(config: &AiConfig, group: Group, read_only: bool) -> bool {
    !(group == Group::Read
        || read_only
        || config.auto_approve.get(&group).copied().unwrap_or(false))
}

pub(crate) async fn status(ctx: &AppContext) -> CoreResult<AiStatus> {
    let mut config = load_config(&ctx.storage).await?;
    let keys = load_keys(&ctx.storage).await?;
    let can_edit = can_edit(access::current());
    let catalog = catalog_tools();
    let tools = catalog
        .iter()
        .map(|t| AiToolInfo {
            name: t.name.to_string(),
            group: t.group,
            enabled: config.tool_enabled(t.name),
            needs_confirmation: needs_confirmation(&config, t.group, t.annotations.read_only_hint),
        })
        .collect();
    let providers = config
        .providers
        .iter()
        .map(|p| AiProviderInfo {
            id: p.id.clone(),
            has_key: keys.get(&p.id).is_some_and(|k| !k.is_empty()),
        })
        .collect();
    if !can_edit {
        for p in &mut config.providers {
            p.headers.clear();
        }
    }
    Ok(AiStatus {
        config,
        providers,
        tools,
        presets: AiPreset::ALL
            .iter()
            .map(|p| AiPresetInfo {
                preset: *p,
                name: p.name().to_string(),
                base_url: p.base_url().to_string(),
                key_required: p.key_required(),
            })
            .collect(),
        can_edit,
    })
}

pub(crate) async fn set_config(ctx: &AppContext, input: AiConfig) -> CoreResult<AiStatus> {
    input.validate()?;
    let value = serde_json::to_value(&input).map_err(|e| CoreError::internal(e.to_string()))?;
    SettingsRepo::new(&ctx.storage)
        .set(CONFIG_KEY, &value, ctx.clock.as_ref())
        .await?;
    // Keys of removed providers go with them.
    let mut keys = load_keys(&ctx.storage).await?;
    let before = keys.len();
    keys.retain(|id, _| input.provider(id).is_some());
    if keys.len() != before {
        save_keys(ctx, &keys).await?;
    }
    ctx.events.publish(crate::Event::new(
        "ai.configChanged",
        serde_json::json!({ "enabled": input.enabled }),
    ));
    status(ctx).await
}

pub(crate) async fn set_key(
    ctx: &AppContext,
    provider_id: &str,
    key: Option<String>,
) -> CoreResult<()> {
    let config = load_config(&ctx.storage).await?;
    if config.provider(provider_id).is_none() {
        return Err(unknown(provider_id));
    }
    let mut keys = load_keys(&ctx.storage).await?;
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(k) => keys.insert(provider_id.to_string(), k),
        None => keys.remove(provider_id),
    };
    save_keys(ctx, &keys).await?;
    ctx.events.publish(crate::Event::new(
        "ai.configChanged",
        serde_json::json!({ "enabled": config.enabled }),
    ));
    Ok(())
}
