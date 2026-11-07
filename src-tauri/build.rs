fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_config",
                "set_config",
                "get_autostart",
                "set_autostart",
                "get_versions",
                "minimise",
                "maximise",
                "close",
                "set_badge_count",
            ]),
        ),
    )
    .expect("failed to run tauri-build");
}
