// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Entrypoint for the native Tauri process.
fn main() {
    arx_runa_tauri_lib::run()
}
