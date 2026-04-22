//! Typed wrapper around `window.__TAURI__.core.Channel` for streaming IPC progress updates.

use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Mirrors the `Channel` constructor from `window.__TAURI__.core`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    #[derive(Clone)]
    pub type Channel;

    /// Constructs a new `Channel` from the Tauri core namespace.
    #[wasm_bindgen(constructor, js_namespace = ["window", "__TAURI__", "core"])]
    pub fn new() -> Channel;

    /// Sets the `onmessage` callback on the channel.
    #[wasm_bindgen(method, setter, js_name = onmessage)]
    pub fn set_onmessage(this: &Channel, cb: &js_sys::Function);
}

/// Typed wrapper around `window.__TAURI__.core.Channel`.
///
/// Pass `inner()` to IPC requests that accept a progress channel argument.
/// Register `on_message` to receive deserialised `T` payloads.
#[derive(Clone)]
pub struct IpcChannel<T: DeserializeOwned + 'static> {
    inner: Channel,
    _marker: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned + 'static> IpcChannel<T> {
    /// Creates a new channel by calling the Tauri JS constructor.
    pub fn new() -> Self {
        Self {
            inner: Channel::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the inner JS value to pass as a request argument.
    pub fn inner(&self) -> &JsValue {
        self.inner.unchecked_ref()
    }

    /// Registers a handler that receives deserialised `T` payloads from the channel.
    ///
    /// The closure is leaked via `Closure::forget` to keep it alive for the channel's lifetime.
    pub fn on_message<F: Fn(T) + 'static>(&self, handler: F) {
        let cb = Closure::wrap(Box::new(move |msg: JsValue| {
            if let Ok(payload) = serde_wasm_bindgen::from_value::<T>(msg) {
                handler(payload);
            }
        }) as Box<dyn Fn(JsValue)>);
        self.inner.set_onmessage(cb.as_ref().unchecked_ref());
        cb.forget();
    }
}

impl<T: DeserializeOwned + 'static> Default for IpcChannel<T> {
    /// Creates a default channel by calling the Tauri JS constructor.
    fn default() -> Self {
        Self::new()
    }
}
