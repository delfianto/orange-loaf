// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        use std::os::unix::process::CommandExt;

        let executable = std::env::current_exe().expect("failed to resolve the current executable");
        let error = std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
            .exec();
        panic!("failed to restart with the WebKitGTK DMABuf renderer disabled: {error}");
    }

    orange_loaf_lib::run();
}
