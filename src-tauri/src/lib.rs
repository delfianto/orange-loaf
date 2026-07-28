mod commands;
mod config;
mod state;
mod tools;

use state::AppState;

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if the underlying webview/runtime fails to initialize (for example, no
/// supported system webview is installed).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::detect_tools,
            commands::set_tool_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
