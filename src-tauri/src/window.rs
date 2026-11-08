use std::sync::atomic::Ordering;

use tauri::{
    include_image, AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;

use crate::config::{Config, ConfigState, WindowState};

/// The web app to load. Override with the `STOAT_APP_URL` environment variable
/// to point the shell at a different instance.
pub fn app_url() -> String {
    std::env::var("STOAT_APP_URL").unwrap_or_else(|_| "https://stoat.chat/app".to_string())
}

pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok()
}

pub fn focus_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn create(app: &AppHandle, config: &Config, init_script: String) -> tauri::Result<WebviewWindow> {
    let is_macos = cfg!(target_os = "macos");
    let start_hidden = config.start_minimised_to_tray;
    let app_handle = app.clone();

    let window = WebviewWindowBuilder::new(
        app,
        "main",
        WebviewUrl::External(app_url().parse().expect("valid URL")),
    )
    .title("Stoat")
    .inner_size(1280.0, 720.0)
    .min_inner_size(300.0, 300.0)
    // Electron uses `frame: !customFrame`: when the web app draws its own
    // title bar (customFrame), the window must be frameless; otherwise you get
    // a double title bar.
    .decorations(if is_macos { true } else { !config.custom_frame })
    .visible(!start_hidden)
    .icon(include_image!("icons/icon.png"))?
    .initialization_script(&init_script)
    // Open links that request a new window (target=_blank / window.open) in the
    // default browser instead of spawning a webview popup, matching Electron.
    .on_new_window(move |url, _features| {
        if matches!(url.scheme(), "http" | "https" | "mailto") {
            let _ = app_handle.opener().open_url(url.as_str(), None::<&str>);
        }
        tauri::webview::NewWindowResponse::Deny
    })
    // Grant media permissions so voice channels (microphone), camera and screen
    // sharing (getDisplayMedia) work in WebKitGTK.
    .on_permission_request(|_webview, kind| match kind {
        tauri::webview::PermissionKind::Microphone
        | tauri::webview::PermissionKind::Camera
        | tauri::webview::PermissionKind::DisplayCapture
        | tauri::webview::PermissionKind::Notifications => {
            tauri::webview::PermissionResponse::Allow
        }
        _ => tauri::webview::PermissionResponse::Default,
    })
    .on_page_load(|window, _payload| {
        let app_handle = window.app_handle();
        let config = app_handle.state::<ConfigState>().get();
        let _ = app_handle.emit("config", &config);
    })
    .build()?;

    // Restore previous geometry.
    let ws = &config.window_state;
    if ws.width > 0 && ws.height > 0 {
        let _ = window.set_size(LogicalSize::new(ws.width as f64, ws.height as f64));
    }
    if ws.x > 0 || ws.y > 0 {
        let _ = window.set_position(LogicalPosition::new(ws.x as f64, ws.y as f64));
    }
    if ws.is_maximised && !start_hidden {
        let _ = window.maximize();
    }

    attach_window_events(&window);

    // Enable WebRTC + MediaStream in WebKitGTK (both are disabled by default
    // there; LiveKit voice needs WebRTC for RTCPeerConnection and MediaStream
    // for navigator.mediaDevices/getUserMedia).
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::{SettingsExt, WebViewExt};
        window.with_webview(|webview| {
            let webview = webview.inner();
            if let Some(settings) = webview.settings() {
                settings.set_enable_webrtc(true);
                settings.set_enable_media_stream(true);
            }
        })?;
    }

    Ok(window)
}

fn attach_window_events(window: &WebviewWindow) {
    let app_handle = window.app_handle().clone();

    window.on_window_event(move |event| {
        use tauri::WindowEvent;
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                let state = app_handle.state::<ConfigState>();
                if !state.should_quit.load(Ordering::SeqCst)
                    && state.get().minimise_to_tray
                {
                    api.prevent_close();
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            }
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let pos = w.outer_position().unwrap_or_default();
                    let size = w.outer_size().unwrap_or_default();
                    app_handle.state::<ConfigState>().set_window_state(WindowState {
                        x: pos.x,
                        y: pos.y,
                        width: size.width,
                        height: size.height,
                        is_maximised: w.is_maximized().unwrap_or(false),
                    });
                }
            }
            _ => {}
        }
    });
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    node: String,
    chrome: String,
    electron: String,
    desktop: String,
}

#[tauri::command]
pub fn get_versions(app: AppHandle) -> Versions {
    Versions {
        node: String::new(),
        chrome: String::new(),
        electron: "tauri".to_string(),
        desktop: app.package_info().version.to_string(),
    }
}

#[tauri::command]
pub fn minimise(window: WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn maximise(window: WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
pub fn close(window: WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
pub fn set_badge_count(window: WebviewWindow, count: i32) {
    #[cfg(target_os = "macos")]
    {
        if count == -1 {
            let _ = window.set_badge_label(Some("\u{2022}".to_string()));
        } else if count == 0 {
            let _ = window.set_badge_label(None);
            let _ = window.set_badge_count(None);
        } else {
            let _ = window.set_badge_count(Some(count as i64));
        }
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        // Linux: numeric badge via libunity (GNOME/Unity), no-op elsewhere.
        if count <= 0 {
            let _ = window.set_badge_count(None);
        } else {
            let _ = window.set_badge_count(Some(count as i64));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if count == 0 {
            let _ = window.set_overlay_icon(None);
        } else {
            let key = if count == -1 { -1 } else { count.clamp(1, 10) };
            if let Some(icon) = badge_icon(key) {
                let _ = window.set_overlay_icon(Some(icon));
            }
        }
    }
}

/// Returns the overlay icon for the given badge count on Windows.
#[cfg(target_os = "windows")]
fn badge_icon(count: i32) -> Option<tauri::image::Image<'static>> {
    Some(match count {
        -1 => tauri::include_image!("assets/badges/-1.ico"),
        1 => tauri::include_image!("assets/badges/1.ico"),
        2 => tauri::include_image!("assets/badges/2.ico"),
        3 => tauri::include_image!("assets/badges/3.ico"),
        4 => tauri::include_image!("assets/badges/4.ico"),
        5 => tauri::include_image!("assets/badges/5.ico"),
        6 => tauri::include_image!("assets/badges/6.ico"),
        7 => tauri::include_image!("assets/badges/7.ico"),
        8 => tauri::include_image!("assets/badges/8.ico"),
        9 => tauri::include_image!("assets/badges/9.ico"),
        10 => tauri::include_image!("assets/badges/10.ico"),
        _ => return None,
    })
}
