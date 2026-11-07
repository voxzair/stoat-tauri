#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

mod autostart;
mod bridge;
mod config;
mod rpc;
mod tray;
#[cfg(target_os = "linux")]
mod virtual_mic;
mod window;

fn main() {
    // WebKitGTK on NVIDIA + native Wayland crashes its DMA-BUF renderer with a
    // Wayland protocol error ("Gdk-Message: Error 71 dispatching to Wayland
    // display"). Disable the DMA-BUF renderer so WebKitGTK falls back to a safe
    // rendering path. Must be set before the webview is created.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        // NVIDIA's hardware video decoder reports invalid GStreamer int ranges
        // on some setups; force software decoding.
        std::env::set_var("GST_PLUGIN_FEATURE_RANK", "nvh264dec:0,nvh265dec:0,nvvp8dec:0,nvvp9dec:0");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let handle = app.handle();

            let config_state = config::load(handle);
            let config = config_state.get();
            let desktop_version = handle.package_info().version.to_string();
            let wayland = window::is_wayland();
            let script = bridge::init_script(&desktop_version, wayland);

            // mark first launch as consumed
            if config.first_launch {
                config_state.apply(config::ConfigPatch {
                    first_launch: Some(false),
                    ..Default::default()
                });
            }

            app.manage(config_state);

            window::create(handle, &config, script)?;
            tray::init(handle)?;

            if config.discord_rpc {
                rpc::init();
            }

            // Virtual sink/source for screen-share audio (Linux Wayland only).
            #[cfg(target_os = "linux")]
            virtual_mic::init(handle.package_info().name.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::set_config,
            autostart::get_autostart,
            autostart::set_autostart,
            window::get_versions,
            window::minimise,
            window::maximise,
            window::close,
            window::set_badge_count,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, _event| {});
}
