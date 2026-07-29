mod commands;
mod config;
mod encoders;
mod filters;
mod job;
mod probe;
mod queue;
mod state;
mod thumbnail;
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
        .setup(|app| {
            // Resumes whatever the queue looked like when the app last
            // exited (see `Queue::load`'s recovery reset) rather than
            // leaving already-queued jobs sitting idle until some other
            // action happens to trigger dispatch.
            queue::dispatch_next(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_tools,
            commands::set_tool_path,
            commands::probe_video,
            commands::generate_thumbnail,
            commands::start_encode,
            commands::preview_command,
            queue::list_queue,
            queue::enqueue_job,
            queue::remove_from_queue,
            queue::move_job,
            queue::cancel_job,
            queue::pause_queue,
            queue::resume_queue,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
