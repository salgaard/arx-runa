//! Thin wasm-bindgen extern for the Tauri webview drag-drop event API.
//!
//! The `onDragDropEvent` handler is subscribed on mount via `Effect::new`
//! and unsubscribed via `on_cleanup`. Handles only the "drop" variant;
//! ignores "enter", "over", and "leave".

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Returns the current `WebviewWindow` from `window.__TAURI__.webview`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "webview"])]
    fn getCurrentWebviewWindow() -> JsValue;
}

/// Subscribes to `onDragDropEvent` on the current Tauri webview window.
///
/// Calls `handler` with the list of dropped file paths on a successful drop.
/// "Enter", "over", and "leave" variants are silently ignored.
///
/// Returns an unsubscribe closure that should be called in `on_cleanup`.
pub fn on_file_drop<F: Fn(Vec<String>) + 'static>(handler: F) -> impl FnOnce() {
    let window = getCurrentWebviewWindow();
    let handler = std::rc::Rc::new(handler);

    let cb = Closure::wrap(Box::new(move |event: JsValue| {
        // The event payload is { type: "drop", paths: [...] } | { type: "enter"/"over"/"leave" }
        let event_type = js_sys::Reflect::get(&event, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string());

        if event_type.as_deref() != Some("drop") {
            return;
        }

        let paths_val = js_sys::Reflect::get(&event, &JsValue::from_str("paths"))
            .unwrap_or(JsValue::undefined());

        let paths = js_sys::Array::from(&paths_val)
            .iter()
            .filter_map(|v| v.as_string())
            .collect::<Vec<_>>();

        if !paths.is_empty() {
            handler(paths);
        }
    }) as Box<dyn Fn(JsValue)>);

    let on_drag_drop = js_sys::Reflect::get(&window, &JsValue::from_str("onDragDropEvent"))
        .unwrap_or(JsValue::undefined());

    let unsub = if let Ok(func) = on_drag_drop.dyn_into::<js_sys::Function>() {
        let result = func
            .call1(&window, cb.as_ref().unchecked_ref())
            .unwrap_or(JsValue::undefined());
        Some(result)
    } else {
        None
    };

    cb.forget();

    move || {
        if let Some(unsub_fn) = unsub
            && let Ok(f) = unsub_fn.dyn_into::<js_sys::Function>()
        {
            let _ = f.call0(&JsValue::undefined());
        }
    }
}
