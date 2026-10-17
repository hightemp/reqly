// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_webkit_workarounds();
    reqly_desktop_lib::run();
}

/// Hosts that must never go through a proxy: the dev server (`localhost:5173`)
/// and other local services.
const LOCAL_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// `existing` (a `NO_PROXY` value) with the local hosts added.
fn with_local_hosts(existing: Option<&str>) -> String {
    let mut hosts: Vec<String> = existing
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .collect();
    for host in LOCAL_HOSTS {
        if !hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
            hosts.push(host.to_string());
        }
    }
    hosts.join(",")
}

/// Linux webview fixes, applied before any thread starts.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn linux_webkit_workarounds() {
    // WebKitGTK's DMA-BUF renderer aborts with `EGL_NOT_INITIALIZED` on many
    // NVIDIA setups. Disable it unless the user decided otherwise.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called first thing in `main`, before any other thread exists.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    // WebKitGTK resolves proxies from `http_proxy` & co. Without this the
    // webview sends `http://localhost:5173` (dev) to the system proxy.
    // Reqly's own requests set their proxy explicitly and are not affected.
    for var in ["NO_PROXY", "no_proxy"] {
        let value = with_local_hosts(std::env::var(var).ok().as_deref());
        // SAFETY: as above, no other thread exists yet.
        unsafe { std::env::set_var(var, value) };
    }
}

#[cfg(test)]
mod tests {
    use super::with_local_hosts;

    #[test]
    fn local_hosts_are_added_once() {
        assert_eq!(with_local_hosts(None), "localhost,127.0.0.1,::1");
        assert_eq!(
            with_local_hosts(Some("example.com, LOCALHOST")),
            "example.com,LOCALHOST,127.0.0.1,::1"
        );
        assert_eq!(with_local_hosts(Some("*")), "*,localhost,127.0.0.1,::1");
    }
}
