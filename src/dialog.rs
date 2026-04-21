//! Thin wasm-bindgen extern for the `tauri-plugin-dialog` JS surface.
//!
//! Exposes only `open_file_dialog` (single-file, non-directory). The plugin's
//! `window.__TAURI__.plugin.dialog.open` function is called with the fully
//! qualified namespace; do not shorten it.

use serde_json::json;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Binds `window.__TAURI__.plugin.dialog.open`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "plugin", "dialog"], catch)]
    async fn open(options: JsValue) -> Result<JsValue, JsValue>;
}

/// Opens a native single-file open dialog.
///
/// Returns `Some(path)` when the user selects a file, `None` when cancelled.
pub async fn open_file_dialog() -> Option<String> {
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
