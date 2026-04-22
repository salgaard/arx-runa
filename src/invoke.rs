use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;

use crate::error::IpcError;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Checks if Tauri IPC is available (i.e., we're in a Tauri context, not browser dev).
fn is_tauri_ipc_available() -> bool {
    use wasm_bindgen::JsValue;
    match web_sys::window() {
        Some(window) => {
            let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"));
            if let Ok(tauri_obj) = tauri {
                if !tauri_obj.is_undefined() && !tauri_obj.is_null() {
                    let core = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("core"));
                    if let Ok(core_obj) = core {
                        if !core_obj.is_undefined() && !core_obj.is_null() {
                            let invoke_fn =
                                js_sys::Reflect::get(&core_obj, &JsValue::from_str("invoke"));
                            return invoke_fn.is_ok() && !invoke_fn.unwrap().is_undefined();
                        }
                    }
                }
            }
            false
        }
        None => false,
    }
}

/// Type-safe wrapper around `window.__TAURI__.core.invoke`.
///
/// Serialises `args` via `serde_wasm_bindgen`, invokes the Tauri command,
/// and deserialises either the success payload into `R` or the rejected
/// `IpcError` JSON payload.
///
/// Returns an error if running in browser dev mode without Tauri context.
pub async fn invoke_command<A, R>(cmd: &str, args: &A) -> Result<R, IpcError>
where
    A: Serialize,
    R: DeserializeOwned,
{
    if !is_tauri_ipc_available() {
        return Err(IpcError::internal(
            "Tauri IPC unavailable — running in browser dev mode, not Tauri desktop app",
        ));
    }

    let args_js = serde_wasm_bindgen::to_value(args)
        .map_err(|_| IpcError::internal("Failed to serialise command arguments"))?;

    match invoke(cmd, args_js).await {
        Ok(result_js) => serde_wasm_bindgen::from_value(result_js)
            .map_err(|_| IpcError::internal("Failed to deserialise command response")),
        Err(error_js) => Err(serde_wasm_bindgen::from_value::<IpcError>(error_js)
            .unwrap_or_else(|_| IpcError::internal("Unknown error"))),
    }
}
