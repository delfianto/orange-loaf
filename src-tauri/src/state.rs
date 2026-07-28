use std::sync::Mutex;

use crate::config::Config;

/// Shared, mutable application state, managed by Tauri and accessed from
/// commands via `tauri::State`.
pub struct AppState {
    pub config: Mutex<Config>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::load()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
