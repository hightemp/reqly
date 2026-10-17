// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_webkit_workarounds();
    reqly_desktop_lib::run();
}

/// WebKitGTK's DMA-BUF renderer aborts with `EGL_NOT_INITIALIZED` on many
/// NVIDIA setups. Disable it unless the user decided otherwise.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn linux_webkit_workarounds() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called first thing in `main`, before any other thread exists.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}
