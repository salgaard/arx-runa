//! Thin wasm-bindgen extern for the `tauri-plugin-dialog` JS surface.
//!
//! In Tauri 2.x, plugins are dynamically loaded and expose `window.__TAURI_PLUGIN_DIALOG__`.
//! This module wraps those async functions for use in WASM.
//!
//! Note: These APIs are only available in the Tauri desktop app context. In browser
//! development, the Tauri plugin bridge is unavailable, and these functions return None.

use serde_json::json;
use wasm_bindgen::prelude::*;

/// Checks if the Tauri dialog plugin is available (i.e., we're in a Tauri context, not browser dev).
fn is_tauri_context() -> bool {
    use wasm_bindgen::JsValue;
    match web_sys::window() {
        Some(window) => {
            let dialog_plugin =
                js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI_PLUGIN_DIALOG__"));
            if let Ok(plugin_obj) = dialog_plugin
                && !plugin_obj.is_undefined()
                && !plugin_obj.is_null()
            {
                return true;
            }
            web_sys::console::warn_1(&JsValue::from_str(
                "Tauri dialog plugin not available — running in browser or plugin not initialized",
            ));
            false
        }
        None => false,
    }
}

/// Opens a native directory picker dialog.
///
/// Returns `Some(path)` when the user selects a directory, `None` when cancelled.
/// Returns `None` if running in browser dev mode (Tauri plugin unavailable).
pub async fn open_file_dialog() -> Option<String> {
    if !is_tauri_context() {
        return None;
    }

    let window = web_sys::window()?;
    let dialog_plugin = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI_PLUGIN_DIALOG__"))
        .ok()?;
    let open_fn = js_sys::Reflect::get(&dialog_plugin, &JsValue::from_str("open")).ok()?;

    let opts = serde_wasm_bindgen::to_value(&json!({
        "multiple": false,
        "directory": false,
    }))
    .ok()?;

    let promise = open_fn.dyn_into::<js_sys::Function>().ok()?.call1(&dialog_plugin, &opts).ok()?;
    let result = js_sys::Promise::resolve(&promise);
    let result_val = wasm_bindgen_futures::JsFuture::from(result).await.ok()?;

    if result_val.is_null() || result_val.is_undefined() {
        return None;
    }
    result_val.as_string()
}

/// Opens a native directory picker dialog.
///
/// Returns `Some(path)` when the user selects a directory, `None` when cancelled.
/// Returns `None` if running in browser dev mode (Tauri plugin unavailable).
pub async fn open_directory_dialog() -> Option<String> {
    if !is_tauri_context() {
        return None;
    }

    let window = web_sys::window()?;
    let dialog_plugin = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI_PLUGIN_DIALOG__"))
        .ok()?;
    let open_fn = js_sys::Reflect::get(&dialog_plugin, &JsValue::from_str("open")).ok()?;

    let opts = serde_wasm_bindgen::to_value(&json!({
        "multiple": false,
        "directory": true,
    }))
    .ok()?;

    let promise = open_fn.dyn_into::<js_sys::Function>().ok()?.call1(&dialog_plugin, &opts).ok()?;
    let result = js_sys::Promise::resolve(&promise);
    let result_val = wasm_bindgen_futures::JsFuture::from(result).await.ok()?;

    if result_val.is_null() || result_val.is_undefined() {
        return None;
    }
    result_val.as_string()
}

/// Opens a native single-file save dialog.
///
/// Returns `Some(path)` when the user selects a destination, `None` when cancelled.
/// Returns `None` if running in browser dev mode (Tauri plugin unavailable).
pub async fn open_save_dialog(default_name: Option<&str>) -> Option<String> {
    if !is_tauri_context() {
        return None;
    }

    let window = web_sys::window()?;
    let dialog_plugin = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI_PLUGIN_DIALOG__"))
        .ok()?;
    let save_fn = js_sys::Reflect::get(&dialog_plugin, &JsValue::from_str("save")).ok()?;

    let mut opts_obj = json!({
        "directory": false,
    });
    if let Some(name) = default_name {
        opts_obj["defaultPath"] = json!(name);
    }
    let opts = serde_wasm_bindgen::to_value(&opts_obj).ok()?;

    let promise = save_fn.dyn_into::<js_sys::Function>().ok()?.call1(&dialog_plugin, &opts).ok()?;
    let result = js_sys::Promise::resolve(&promise);
    let result_val = wasm_bindgen_futures::JsFuture::from(result).await.ok()?;

    if result_val.is_null() || result_val.is_undefined() {
        return None;
    }
    result_val.as_string()
}
