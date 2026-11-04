//! Update checks against the GitHub releases of Reqly (docs/updates.md).
//!
//! The check sends one anonymous `GET` of the release list, through the
//! proxy and CA certificates of the HTTP settings; installing an update is
//! the shell's job (the Tauri updater on desktop).

use reqly_core::model::HttpRequest;
use reqly_core::model::{KeyValue, RequestSettings};
use reqly_core::{CoreError, CoreResult, Timestamp};
use reqly_http::engine::{CancelToken, ExecOptions};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AppContext;
use super::http::{HTTP_DEFAULTS_KEY, HTTP_TLS_KEY, TlsSettings, setting};
use crate::registry::{Command, Registry};

pub const UPDATE_SETTINGS_KEY: &str = "updates";
const UPDATE_STATE_KEY: &str = "updates.state";
pub const DEFAULT_SOURCE: &str = "https://api.github.com/repos/hightemp/reqly/releases?per_page=30";

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<UpdateCheck>();
}

/// Where updates come from; set by the shell.
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    /// The release list (GitHub API format); `None` turns checks off.
    pub url: Option<String>,
    /// The running version.
    pub current: String,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            url: Some(DEFAULT_SOURCE.into()),
            current: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

impl UpdateConfig {
    /// The default source, or `REQLY_UPDATE_URL`; `REQLY_UPDATE_CHECK=false`
    /// turns checks off (closed networks).
    pub fn from_env() -> Self {
        let off = std::env::var("REQLY_UPDATE_CHECK").is_ok_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        });
        let url = std::env::var("REQLY_UPDATE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SOURCE.into());
        Self {
            url: (!off).then_some(url),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateChannel {
    #[default]
    Stable,
    /// Stable releases and pre-releases (`-beta.1`, `-rc.1`…).
    Prerelease,
}

/// Update preferences (setting `updates`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UpdateSettings {
    /// Check at start and every `interval_hours`.
    pub auto_check: bool,
    /// Download and install without asking (desktop).
    pub auto_install: bool,
    pub channel: UpdateChannel,
    pub interval_hours: u32,
    /// A version the user does not want to hear about again.
    pub skipped_version: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            auto_install: false,
            channel: UpdateChannel::Stable,
            interval_hours: 24,
            skipped_version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateRelease {
    /// Semver without the `v`.
    pub version: String,
    pub tag: String,
    pub name: String,
    /// Release notes (Markdown).
    pub notes: String,
    /// The release page.
    pub url: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}

/// The last successful check (setting `updates.state`).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct UpdateState {
    checked_at: Option<Timestamp>,
    /// The version the releases were compared with.
    current: String,
    /// Releases newer than `current`, newest first.
    newer: Vec<UpdateRelease>,
}

#[derive(Debug, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UpdateCheckInput {
    /// "Check for updates": always asks the source and shows skipped versions.
    pub manual: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateCheckOutput {
    /// False when checks are turned off for this build or server.
    pub enabled: bool,
    pub current: String,
    /// When the source last answered.
    pub checked_at: Option<Timestamp>,
    /// The update to offer, if any.
    pub release: Option<UpdateRelease>,
}

/// Looks for a newer release. Automatic checks (`manual: false`) ask the
/// source at most once per interval and answer from the last result
/// otherwise; errors leave that result in place.
pub struct UpdateCheck;

impl Command<AppContext> for UpdateCheck {
    const NAME: &'static str = "update.check";
    type Input = UpdateCheckInput;
    type Output = UpdateCheckOutput;

    async fn run(ctx: &AppContext, input: UpdateCheckInput) -> CoreResult<UpdateCheckOutput> {
        let current = ctx.updates.current.clone();
        let Some(url) = ctx.updates.url.clone() else {
            return Ok(UpdateCheckOutput {
                enabled: false,
                current,
                checked_at: None,
                release: None,
            });
        };
        let settings: UpdateSettings = setting(ctx, UPDATE_SETTINGS_KEY).await?;
        let mut state: UpdateState = setting(ctx, UPDATE_STATE_KEY).await?;
        if state.current != current {
            state = UpdateState::default();
        }
        let interval = i64::from(settings.interval_hours.clamp(1, 24 * 30)) * 3_600_000;
        let due = state
            .checked_at
            .is_none_or(|at| ctx.clock.now().millis() - at.millis() >= interval);
        if input.manual || (settings.auto_check && due) {
            let releases = fetch(ctx, &url).await?;
            state = UpdateState {
                checked_at: Some(ctx.clock.now()),
                newer: newer_than(&current, releases),
                current: current.clone(),
            };
            let value =
                serde_json::to_value(&state).map_err(|e| CoreError::internal(e.to_string()))?;
            reqly_storage::settings::SettingsRepo::new(&ctx.storage)
                .set(
                    &crate::access::settings_key(UPDATE_STATE_KEY),
                    &value,
                    ctx.clock.as_ref(),
                )
                .await?;
        }
        let skipped = (!input.manual)
            .then_some(settings.skipped_version.as_deref())
            .flatten();
        let release = state
            .newer
            .iter()
            .find(|r| {
                (settings.channel == UpdateChannel::Prerelease || !r.prerelease)
                    && Some(r.version.as_str()) != skipped
            })
            .cloned();
        Ok(UpdateCheckOutput {
            enabled: true,
            current,
            checked_at: state.checked_at,
            release,
        })
    }
}

fn network(message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new("update.network", message.clone()).with_param("detail", message)
}

async fn fetch(ctx: &AppContext, url: &str) -> CoreResult<Vec<GithubRelease>> {
    let global: RequestSettings = setting(ctx, HTTP_DEFAULTS_KEY).await?;
    let tls: TlsSettings = setting(ctx, HTTP_TLS_KEY).await?;
    let request = HttpRequest {
        method: "GET".into(),
        url: url.into(),
        headers: vec![
            KeyValue::new("Accept", "application/vnd.github+json"),
            KeyValue::new("User-Agent", format!("Reqly/{}", env!("CARGO_PKG_VERSION"))),
        ],
        settings: RequestSettings {
            timeout_ms: Some(15_000),
            ..global
        },
        ..HttpRequest::default()
    };
    let prepared = reqly_http::prepare::prepare(&request, &[]).map_err(|e| network(e.message))?;
    let options = ExecOptions {
        ca_pems: tls.ca_pems,
        ..ExecOptions::default()
    };
    let response = match &ctx.network_policy {
        Some(policy) => {
            reqly_http::policy::send_checked(prepared, options, CancelToken::new(), None, policy)
                .await
        }
        None => reqly_http::engine::execute(prepared, options, CancelToken::new(), None).await,
    }
    .map_err(|e| network(e.message))?;
    if !(200..300).contains(&response.status) {
        return Err(
            network(format!("the update source answered {}", response.status))
                .with_param("status", u64::from(response.status)),
        );
    }
    let bytes = response.body.to_vec().map_err(|e| network(e.to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::new("update.invalid", format!("not a release list: {e}")))
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Published releases with a semver tag newer than `current`, newest first.
fn newer_than(current: &str, releases: Vec<GithubRelease>) -> Vec<UpdateRelease> {
    let Ok(current) = semver::Version::parse(current) else {
        return Vec::new();
    };
    let mut newer: Vec<(semver::Version, UpdateRelease)> = releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = semver::Version::parse(r.tag_name.trim_start_matches('v')).ok()?;
            (version > current).then(|| {
                let release = UpdateRelease {
                    version: version.to_string(),
                    name: r
                        .name
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| r.tag_name.clone()),
                    tag: r.tag_name,
                    notes: r.body.unwrap_or_default(),
                    url: r.html_url,
                    published_at: r.published_at,
                    prerelease: r.prerelease || !version.pre.is_empty(),
                    assets: r
                        .assets
                        .into_iter()
                        .map(|a| ReleaseAsset {
                            name: a.name,
                            url: a.browser_download_url,
                            size: a.size,
                        })
                        .collect(),
                };
                (version, release)
            })
        })
        .collect();
    newer.sort_by(|a, b| b.0.cmp(&a.0));
    newer.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(tag: &str, prerelease: bool) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            name: None,
            body: None,
            draft: false,
            prerelease,
            published_at: None,
            html_url: String::new(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn semver_ordering() {
        let versions = |current, tags: &[&str]| {
            newer_than(current, tags.iter().map(|t| gh(t, false)).collect())
                .into_iter()
                .map(|r| (r.version, r.prerelease))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            versions(
                "0.9.0",
                &["v0.9.1", "v0.10.0", "0.9.0", "v0.10.0-rc.1", "x"]
            ),
            vec![
                ("0.10.0".into(), false),
                ("0.10.0-rc.1".into(), true),
                ("0.9.1".into(), false)
            ]
        );
        assert_eq!(versions("0.10.0-rc.1", &["v0.10.0"]).len(), 1);
        assert!(versions("not a version", &["v1.0.0"]).is_empty());
    }

    #[test]
    fn settings_defaults() {
        let s: UpdateSettings = serde_json::from_str("{}").unwrap();
        assert!(s.auto_check);
        assert!(!s.auto_install);
        assert_eq!(s.interval_hours, 24);
        assert_eq!(s.channel, UpdateChannel::Stable);
    }
}
