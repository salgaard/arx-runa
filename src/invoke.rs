use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;

use crate::error::IpcError;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Type-safe wrapper around `window.__TAURI__.core.invoke`.
///
/// Serialises `args` via `serde_wasm_bindgen`, invokes the Tauri command,
/// and deserialises either the success payload into `R` or the rejected
/// `IpcError` JSON payload.
pub async fn invoke_command<A, R>(cmd: &str, args: &A) -> Result<R, IpcError>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args_js = serde_wasm_bindgen::to_value(args)
        .map_err(|_| IpcError::internal("Failed to serialise command arguments"))?;

    match invoke(cmd, args_js).await {
        Ok(result_js) => serde_wasm_bindgen::from_value(result_js)
            .map_err(|_| IpcError::internal("Failed to deserialise command response")),
        Err(error_js) => Err(serde_wasm_bindgen::from_value::<IpcError>(error_js)
            .unwrap_or_else(|_| IpcError::internal("Unknown error"))),
    }
}
