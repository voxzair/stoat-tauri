use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_maximised: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            is_maximised: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub first_launch: bool,
    pub custom_frame: bool,
    pub minimise_to_tray: bool,
    pub start_minimised_to_tray: bool,
    pub spellchecker: bool,
    pub hardware_acceleration: bool,
    pub discord_rpc: bool,
    pub window_state: WindowState,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            first_launch: true,
            custom_frame: true,
            minimise_to_tray: true,
            start_minimised_to_tray: false,
            spellchecker: true,
            hardware_acceleration: true,
            discord_rpc: true,
            window_state: WindowState::default(),
        }
    }
}

/// Partial update mirroring the Electron preload behaviour, where the renderer
/// sends a (possibly partial) config object to be merged in.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigPatch {
    pub first_launch: Option<bool>,
    pub custom_frame: Option<bool>,
    pub minimise_to_tray: Option<bool>,
    pub start_minimised_to_tray: Option<bool>,
    pub spellchecker: Option<bool>,
    pub hardware_acceleration: Option<bool>,
    pub discord_rpc: Option<bool>,
    pub window_state: Option<WindowState>,
}

pub struct ConfigState {
    path: PathBuf,
    config: Mutex<Config>,
    pub should_quit: AtomicBool,
}

impl ConfigState {
    fn save(&self, config: &Config) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(config) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    pub fn get(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    pub fn apply(&self, patch: ConfigPatch) -> Config {
        let mut config = self.config.lock().unwrap();

        if let Some(v) = patch.first_launch {
            config.first_launch = v;
        }
        if let Some(v) = patch.custom_frame {
            config.custom_frame = v;
        }
        if let Some(v) = patch.minimise_to_tray {
            config.minimise_to_tray = v;
        }
        if let Some(v) = patch.start_minimised_to_tray {
            config.start_minimised_to_tray = v;
        }
        if let Some(v) = patch.spellchecker {
            config.spellchecker = v;
        }
        if let Some(v) = patch.hardware_acceleration {
            config.hardware_acceleration = v;
        }
        if let Some(v) = patch.discord_rpc {
            config.discord_rpc = v;
        }
        if let Some(v) = patch.window_state {
            config.window_state = v;
        }

        self.save(&config);
        config.clone()
    }

    pub fn set_window_state(&self, window_state: WindowState) {
        let mut config = self.config.lock().unwrap();
        config.window_state = window_state;
        self.save(&config);
    }
}

pub fn load(app: &AppHandle) -> ConfigState {
    let dir = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = dir.join("config.json");

    let config = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .unwrap_or_default();

    ConfigState {
        path,
        config: Mutex::new(config),
        should_quit: AtomicBool::new(false),
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, ConfigState>) -> Config {
    state.get()
}

#[tauri::command]
pub fn set_config(
    app: AppHandle,
    state: State<'_, ConfigState>,
    config: ConfigPatch,
) -> Config {
    let before = state.get();
    let new = state.apply(config);

    // React to Discord RPC toggling.
    if before.discord_rpc != new.discord_rpc {
        if new.discord_rpc {
            crate::rpc::init();
        } else {
            crate::rpc::destroy();
        }
    }

    let _ = app.emit("config", &new);
    new
}
