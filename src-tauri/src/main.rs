// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    glib::setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1", false)
        .expect("failed to configure the WebKitGTK DMABuf renderer");

    orange_loaf_lib::run();
}
