//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub mod storage;
pub mod sync;
pub mod ui;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
/// Returns a greeting message from the backend.
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Arx Runa Tauri runtime.
pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
    }
}
