//! Thin wasm-bindgen extern for the Tauri webview drag-drop event API.
//!
//! The `onDragDropEvent` handler is subscribed on mount via `Effect::new`
//! and unsubscribed via `on_cleanup`. Handles only the "drop" variant;
//! ignores "enter", "over", and "leave".

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    /// Returns the current `WebviewWindow` from `window.__TAURI__.webviewWindow`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "webviewWindow"])]
    fn getCurrentWebviewWindow() -> JsValue;
}

// js_sys::Function is !Send, so the cleanup closure cannot hold it directly
// (on_cleanup requires Send). Store resolved unlisten fns in a thread-local
// keyed by a u64 — only the key is captured in the Send cleanup closure.
thread_local! {
    static UNLISTEN_REGISTRY: RefCell<HashMap<u64, js_sys::Function>> = RefCell::new(HashMap::new());
}
static NEXT_LISTENER_ID: AtomicU64 = AtomicU64::new(0);

/// Subscribes to `onDragDropEvent` on the current Tauri webview window.
///
/// Calls `handler` with the list of dropped file paths on a successful drop.
/// "Enter", "over", and "leave" variants are silently ignored.
///
/// Returns an unsubscribe closure that should be called in `on_cleanup`.
pub fn on_file_drop<F: Fn(Vec<String>) + 'static>(handler: F) -> impl FnOnce() + Send {
    let window = getCurrentWebviewWindow();
    let handler = std::rc::Rc::new(handler);
    let id = NEXT_LISTENER_ID.fetch_add(1, Ordering::Relaxed);

    let cb = Closure::wrap(Box::new(move |event: JsValue| {
        // onDragDropEvent delivers Event<DragDropPayload>:
        // { id, event, payload: { type: "drop"|"enter"|..., paths: [...], ... } }
        let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
            .unwrap_or(JsValue::undefined());

        let event_type = js_sys::Reflect::get(&payload, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string());

        if event_type.as_deref() != Some("drop") {
            return;
        }

        let paths_val = js_sys::Reflect::get(&payload, &JsValue::from_str("paths"))
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

    // onDragDropEvent returns Promise<UnlistenFn>. Await it and store the
    // resolved function in UNLISTEN_REGISTRY so cleanup can call it.
    // Without awaiting, cleanup gets the raw Promise (not callable) and the
    // listener leaks — causing N duplicate upload_file calls after N mounts.
    if let Ok(func) = on_drag_drop.dyn_into::<js_sys::Function>() {
        if let Ok(promise_val) = func.call1(&window, cb.as_ref().unchecked_ref()) {
            let promise = js_sys::Promise::from(promise_val);
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(unlisten_fn) = JsFuture::from(promise).await {
                    if let Ok(f) = unlisten_fn.dyn_into::<js_sys::Function>() {
                        UNLISTEN_REGISTRY.with(|reg| reg.borrow_mut().insert(id, f));
                    }
                }
            });
        }
    }

    cb.forget();

    move || {
        UNLISTEN_REGISTRY.with(|reg| {
            if let Some(f) = reg.borrow_mut().remove(&id) {
                let _ = f.call0(&JsValue::undefined());
            }
        });
    }
}
