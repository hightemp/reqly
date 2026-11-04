//! In-place updates with the Tauri updater (docs/updates.md). The core
//! finds the release (`update.check`); these commands install it from the
//! release's `latest.json` after checking the signature.

use reqly_core::CoreError;
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_updater::UpdaterExt;

use crate::Core;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// The build has the public key that update signatures are checked with.
pub fn has_pubkey(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(Value::as_str)
        .is_some_and(|k| !k.trim().is_empty())
}

/// Whether the updater can replace this build: on Linux only an AppImage
/// (deb and rpm installs belong to the package manager).
pub fn supported(has_pubkey: bool, os: &str, appimage: bool) -> bool {
    has_pubkey && (os != "linux" || appimage)
}

/// The manual proxy of the HTTP settings (`http.defaults`), with its
/// credentials; other modes leave the updater to the system settings.
pub fn proxy_url(http_defaults: &Value) -> Option<tauri::Url> {
    let proxy = http_defaults.get("proxy")?;
    if proxy.get("mode")?.as_str()? != "manual" {
        return None;
    }
    let raw = proxy.get("url")?.as_str()?.trim();
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let mut url = tauri::Url::parse(&with_scheme).ok()?;
    let field = |k: &str| proxy.get(k).and_then(Value::as_str).unwrap_or("");
    if !field("username").is_empty() {
        url.set_username(field("username")).ok()?;
        url.set_password(Some(field("password")).filter(|p| !p.is_empty()))
            .ok()?;
    }
    Some(url)
}

fn install_error(message: impl ToString) -> CoreError {
    let message = message.to_string();
    CoreError::new("update.install", message.clone()).with_param("detail", message)
}

#[tauri::command]
pub fn update_installable<R: Runtime>(app: AppHandle<R>) -> bool {
    supported(
        has_pubkey(app.config()),
        std::env::consts::OS,
        std::env::var_os("APPIMAGE").is_some(),
    )
}

/// Downloads and installs the update described by the manifest at `url`.
#[tauri::command]
pub async fn update_install<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    url: String,
    on_progress: Channel<Progress>,
) -> Result<(), CoreError> {
    if !update_installable(app.clone()) {
        return Err(install_error("this build cannot update itself"));
    }
    let endpoint = tauri::Url::parse(&url).map_err(install_error)?;
    let defaults = core
        .registry
        .dispatch(
            &core.ctx,
            "settings.get",
            serde_json::json!({ "key": "http.defaults" }),
        )
        .await?;
    let mut builder = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(install_error)?;
    if let Some(proxy) = proxy_url(&defaults["value"]) {
        builder = builder.proxy(proxy);
    }
    let update = builder
        .build()
        .map_err(install_error)?
        .check()
        .await
        .map_err(install_error)?
        .ok_or_else(|| install_error("the manifest has no newer version"))?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(install_error)
}

#[tauri::command]
pub fn update_restart<R: Runtime>(app: AppHandle<R>) {
    app.restart()
}

/// Registers the updater plugin when the build can check signatures.
pub fn init<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    if has_pubkey(app.config()) {
        app.handle()
            .plugin(tauri_plugin_updater::Builder::new().build())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn deb_and_rpm_installs_are_not_updated() {
        assert!(supported(true, "linux", true));
        assert!(!supported(true, "linux", false));
        assert!(supported(true, "windows", false));
        assert!(supported(true, "macos", false));
        assert!(!supported(false, "windows", false));
    }

    #[test]
    fn pubkey_from_the_config() {
        let mut config = tauri::Config::default();
        assert!(!has_pubkey(&config));
        config
            .plugins
            .0
            .insert("updater".into(), json!({ "pubkey": "  " }));
        assert!(!has_pubkey(&config));
        config
            .plugins
            .0
            .insert("updater".into(), json!({ "pubkey": "dW50cnVzdGVk" }));
        assert!(has_pubkey(&config));
    }

    #[test]
    fn manual_proxy_with_credentials() {
        let proxy = |p: Value| proxy_url(&json!({ "proxy": p })).map(|u| u.to_string());
        assert_eq!(
            proxy(
                json!({ "mode": "manual", "url": "proxy.local:3128", "username": "", "password": "" })
            ),
            Some("http://proxy.local:3128/".into())
        );
        assert_eq!(
            proxy(
                json!({ "mode": "manual", "url": "http://p:8080", "username": "u@x", "password": "s:e" })
            ),
            Some("http://u%40x:s%3Ae@p:8080/".into())
        );
        assert_eq!(
            proxy(json!({ "mode": "system", "url": "http://p:8080" })),
            None
        );
        assert_eq!(proxy(json!({ "mode": "manual", "url": "" })), None);
        assert_eq!(proxy_url(&Value::Null), None);
    }
}
