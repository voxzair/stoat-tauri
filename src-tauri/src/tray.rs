use tauri::{
    include_image,
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let version = app.package_info().version.to_string();

    let title = MenuItem::with_id(app, "title", "Stoat for Desktop", false, None::<&str>)?;
    let version_item =
        MenuItem::with_id(app, "version", version.as_str(), false, None::<&str>)?;
    let version_submenu = Submenu::with_items(app, "Version", true, &[&version_item])?;
    let separator = PredefinedMenuItem::separator(app)?;
    let toggle = MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit App", true, None::<&str>)?;

    let items: Vec<&dyn IsMenuItem<Wry>> = vec![&title, &version_submenu, &separator, &toggle, &quit];
    let menu = Menu::with_items(app, &items)?;

    let icon = include_image!("icons/icon.png");

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Stoat for Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "toggle" => toggle_window(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
