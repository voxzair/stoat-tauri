use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, state: bool) -> Result<bool, String> {
    let autolaunch = app.autolaunch();
    if state {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    autolaunch.is_enabled().map_err(|e| e.to_string())
}
