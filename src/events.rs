//! Reusable Tauri event subscription for the Leptos frontend.
//!
//! Wraps `window.__TAURI__.event.listen` so components can subscribe to
//! backend-pushed events instead of polling commands on a timer. Subscriptions
//! are torn down automatically via [`leptos::prelude::on_cleanup`] when the
//! calling reactive owner (component / effect) is disposed.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Subscribes to Tauri events via `window.__TAURI__.event.listen`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

/// Returns `true` when the Tauri event API is available on `window`.
///
/// Guards against running in browser dev mode where `__TAURI__` is absent.
pub fn is_tauri_event_available() -> bool {
    match web_sys::window() {
        Some(window) => {
            let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"));
            if let Ok(tauri_obj) = tauri
                && !tauri_obj.is_undefined()
                && !tauri_obj.is_null()
            {
                let event_api = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("event"));
                return event_api.is_ok() && !event_api.unwrap().is_undefined();
            }
            false
        }
        None => false,
    }
}

/// Extracts the inner `payload` from a Tauri v2 event object `{ event, id, payload }`.
pub fn event_payload(event: &JsValue) -> JsValue {
    js_sys::Reflect::get(event, &JsValue::from_str("payload")).unwrap_or(JsValue::UNDEFINED)
}

/// Subscribes to a Tauri `event`, passing each raw event object to `handler`.
///
/// `handler` receives the full Tauri v2 event object `{ event, id, payload }`;
/// use [`event_payload`] to reach the inner payload. The listener is registered
/// asynchronously and unsubscribed automatically when the current reactive owner
/// is disposed. No-op when the Tauri event API is unavailable.
pub fn on_tauri_event_raw<F>(event: &'static str, handler: F)
where
    F: Fn(JsValue) + 'static,
{
    if !is_tauri_event_available() {
        return;
    }

    // Arc holds the unlisten fn returned by `listen`, populated once the async
    // registration resolves and called on cleanup.
    let unlisten_fn: Arc<Mutex<Option<js_sys::Function>>> = Arc::new(Mutex::new(None));
    let unlisten_for_cleanup = Arc::clone(&unlisten_fn);

    leptos::task::spawn_local(async move {
        let event_closure = Closure::wrap(Box::new(handler) as Box<dyn Fn(JsValue)>);
        if let Ok(unlisten_val) = listen(event, event_closure.as_ref().unchecked_ref()).await
            && let Ok(function) = unlisten_val.dyn_into::<js_sys::Function>()
        {
            *unlisten_fn.lock().unwrap_or_else(|e| e.into_inner()) = Some(function);
        }
        event_closure.forget();
    });

    on_cleanup(move || {
        if let Ok(mut guard) = unlisten_for_cleanup.lock()
            && let Some(function) = guard.take()
        {
            let _ = function.call0(&JsValue::undefined());
        }
    });
}

/// Subscribes to a Tauri `event`, deserialising each payload into `T`.
///
/// Built on [`on_tauri_event_raw`]; payloads that fail to deserialise into `T`
/// are silently dropped. The subscription is torn down automatically on cleanup.
pub fn on_tauri_event<T, F>(event: &'static str, handler: F)
where
    T: DeserializeOwned + 'static,
    F: Fn(T) + 'static,
{
    on_tauri_event_raw(event, move |raw| {
        let payload = event_payload(&raw);
        if let Ok(value) = serde_wasm_bindgen::from_value::<T>(payload) {
            handler(value);
        }
    });
}
