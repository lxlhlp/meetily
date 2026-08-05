#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    // RUST_LOG is read by tauri-plugin-log (registered in lib.rs), which
    // writes to both stdout and a rotating log file under the app log dir.
    // env_logger is no longer used - it cannot write to files.
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    // Async logger will be initialized lazily when first needed (after Tauri runtime starts)
    app_lib::run();
}
