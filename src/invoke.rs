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
            if let Ok(tauri_obj) = tauri
                && !tauri_obj.is_undefined()
                && !tauri_obj.is_null()
            {
                let core = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("core"));
                if let Ok(core_obj) = core
                    && !core_obj.is_undefined()
                    && !core_obj.is_null()
                {
                    let invoke_fn = js_sys::Reflect::get(&core_obj, &JsValue::from_str("invoke"));
                    return invoke_fn.is_ok() && !invoke_fn.unwrap().is_undefined();
                }
            }
            false
        }
        None => false,
    }
}

/// Variant of `invoke_command` for commands that accept a Tauri `Channel<T>` argument.
///
/// Serialises `args` via serde, then injects `channel_value` at `channel_key` into the
/// resulting JS object before invoking. This bypasses serde for the channel, which is not
/// JSON-serialisable.
pub async fn invoke_command_with_channel<A, R>(
    cmd: &str,
    args: &A,
    channel_key: &str,
    channel_value: &JsValue,
) -> Result<R, IpcError>
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
    let _ = js_sys::Reflect::set(&args_js, &JsValue::from_str(channel_key), channel_value);

    match invoke(cmd, args_js).await {
        Ok(result_js) => serde_wasm_bindgen::from_value(result_js)
            .map_err(|_| IpcError::internal("Failed to deserialise command response")),
        Err(error_js) => {
            if let Ok(ipc_err) = serde_wasm_bindgen::from_value::<IpcError>(error_js.clone()) {
                return Err(ipc_err);
            }
            if let Some(message) = js_sys::Reflect::get(&error_js, &JsValue::from_str("message"))
                .ok()
                .and_then(|msg| msg.as_string())
            {
                return Err(IpcError::internal(message));
            }
            Err(IpcError::internal("Unknown error"))
        }
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
        Err(error_js) => {
            // Tauri wraps command errors; try to extract the inner IpcError.
            // First attempt: deserialise the error_js directly as IpcError.
            if let Ok(ipc_err) = serde_wasm_bindgen::from_value::<IpcError>(error_js.clone()) {
                return Err(ipc_err);
            }

            // Fallback: try to extract the error message from the JsValue if it's a JS error object.
            if let Some(message) =
                js_sys::Reflect::get(&error_js, &wasm_bindgen::JsValue::from_str("message"))
                    .ok()
                    .and_then(|msg| msg.as_string())
            {
                return Err(IpcError::internal(message));
            }

            Err(IpcError::internal("Unknown error"))
        }
    }
}
