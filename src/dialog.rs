//! Thin wasm-bindgen extern for the `tauri-plugin-dialog` JS surface.
//!
//! Exposes `open_file_dialog` (single-file open picker) and `open_save_dialog`
//! (single-file save picker). The plugin's `window.__TAURI__.plugin.dialog` functions
//! are called with the fully qualified namespace; do not shorten them.
//!
//! Note: These APIs are only available in the Tauri desktop app context. In browser
//! development, the Tauri plugin bridge is unavailable, and these functions return None.

use serde_json::json;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Returns `true` if we're running inside the Tauri app (not a browser).
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__"], js_name = "__TAURI_INTERNALS__")]
    type TauriInternals;

    /// Binds `window.__TAURI__.plugin.dialog.open`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "plugin", "dialog"], catch)]
    async fn open(options: JsValue) -> Result<JsValue, JsValue>;

    /// Binds `window.__TAURI__.plugin.dialog.save`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "plugin", "dialog"], catch)]
    async fn save(options: JsValue) -> Result<JsValue, JsValue>;
}

/// Checks if the Tauri dialog plugin is available (i.e., we're in a Tauri context, not browser dev).
fn is_tauri_context() -> bool {
    use wasm_bindgen::JsValue;
    match web_sys::window() {
        Some(window) => {
            let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"));
            if let Ok(tauri_obj) = tauri {
                if !tauri_obj.is_undefined() && !tauri_obj.is_null() {
                    let plugin = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("plugin"));
                    if let Ok(plugin_obj) = plugin {
                        if !plugin_obj.is_undefined() && !plugin_obj.is_null() {
                            let dialog = js_sys::Reflect::get(&plugin_obj, &JsValue::from_str("dialog"));
                            if dialog.is_ok() && !dialog.unwrap().is_undefined() {
                                return true;
                            }
                        }
                    }
                }
            }
            web_sys::console::warn_1(&JsValue::from_str("Tauri dialog plugin not available"));
            false
        }
        None => false,
    }
}

/// Opens a native single-file open dialog.
///
/// Returns `Some(path)` when the user selects a file, `None` when cancelled.
/// Returns `None` if running in browser dev mode (Tauri plugin unavailable).
pub async fn open_file_dialog() -> Option<String> {
    if !is_tauri_context() {
        return None;
    }
    let opts = serde_wasm_bindgen::to_value(&json!({
        "multiple": false,
        "directory": false,
    }))
    .ok()?;
    let result = open(opts).await.ok()?;
    if result.is_null() || result.is_undefined() {
        return None;
    }
    result.as_string()
}

/// Opens a native single-file save dialog.
///
/// Returns `Some(path)` when the user selects a destination, `None` when cancelled.
/// Returns `None` if running in browser dev mode (Tauri plugin unavailable).
pub async fn open_save_dialog(default_name: Option<&str>) -> Option<String> {
    if !is_tauri_context() {
        return None;
    }
    let mut opts_obj = json!({
        "directory": false,
    });
    if let Some(name) = default_name {
        opts_obj["defaultPath"] = json!(name);
    }
    let opts = serde_wasm_bindgen::to_value(&opts_obj).ok()?;
    let result = save(opts).await.ok()?;
    if result.is_null() || result.is_undefined() {
        return None;
    }
    result.as_string()
}
