/// Builds the initialization script that shims the Electron preload globals
/// (`window.desktopConfig` and `window.native`) onto the Tauri IPC bridge, so
/// the remote Stoat web app can keep using the same API surface.
pub fn init_script(desktop_version: &str, is_wayland: bool) -> String {
    format!(
        r#"
(function () {{
  if (window.__stoatBridge) return;
  window.__stoatBridge = true;

  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
  let cachedConfig = null;

  const whenTauri = (cb) => {{
    if (window.__TAURI__) {{ cb(); return; }}
    let tries = 0;
    const timer = setInterval(() => {{
      if (window.__TAURI__) {{ clearInterval(timer); cb(); }}
      else if (++tries > 400) clearInterval(timer);
    }}, 25);
  }};

  whenTauri(() => window.__TAURI__.event.listen("config", (e) => {{ cachedConfig = e.payload; }}));

  window.desktopConfig = {{
    get: () => cachedConfig,
    set: (config) => invoke("set_config", {{ config }}),
    getAutostart: () => invoke("get_autostart"),
    setAutostart: (state) => invoke("set_autostart", {{ state }}),
  }};

  window.native = {{
    versions: {{
      node: () => "",
      chrome: () => "",
      electron: () => "tauri",
      desktop: () => "{desktop_version}",
    }},
    minimise: () => invoke("minimise"),
    maximise: () => invoke("maximise"),
    close: () => invoke("close"),
    setBadgeCount: (count) => invoke("set_badge_count", {{ count }}),
    onceScreenPicker: () => {{}},
    screenPickerCallback: () => {{}},
    isWayland: async () => {is_wayland},
  }};
}})();
"#
    )
}
