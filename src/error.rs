use serde::Deserialize;

/// Frontend representation of backend-sanitised IPC errors.
///
/// Deserialised from the JSON shape `{"kind": "<camelCase>", "message": "..."}`
/// produced by `src-tauri/src/ui/error.rs::IpcError`.
#[derive(Debug, Clone, Deserialize)]
pub struct IpcError {
    /// Machine-readable discriminator: `"vaultLocked"`, `"authenticationFailed"`,
    /// `"notFound"`, `"alreadyExists"`, `"cloudError"`, `"invalidInput"`,
    /// `"internalError"`.
    pub kind: String,
    /// User-safe, displayable message (no paths, keys, or internals).
    pub message: String,
}

impl IpcError {
    /// Build a synthetic error for client-side serialisation / parse failures
    /// where no backend error was produced.
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: "internalError".into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for IpcError {
    /// Formats the error using the user-safe `message` field.
    ///
    /// Intentionally does NOT include `kind` to avoid leaking machine-readable
    /// discriminators into UI error text; Leptos `ErrorBoundary` uses this impl.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_error_deserialises_from_backend_json_shape() {
        let json = r#"{"kind":"notFound","message":"File or directory not found"}"#;
        let err: IpcError = serde_json::from_str(json).unwrap();
        assert_eq!(err.kind, "notFound");
        assert_eq!(err.message, "File or directory not found");
    }

    #[test]
    fn test_ipc_error_internal_constructor_produces_internal_error_kind() {
        let err = IpcError::internal("something went wrong");
        assert_eq!(err.kind, "internalError");
        assert_eq!(err.message, "something went wrong");
    }

    #[test]
    fn test_ipc_error_display_delegates_to_message_not_debug() {
        let err = IpcError {
            kind: "vaultLocked".into(),
            message: "Vault is locked".into(),
        };
        assert_eq!(err.to_string(), "Vault is locked");
        assert!(!err.to_string().contains("vaultLocked"));
    }

    // --- Edge-case / forward-compat coverage ---

    /// An unrecognised `kind` value must still deserialise successfully so the
    /// frontend remains forward-compatible when the backend adds new discriminators.
    #[test]
    fn test_ipc_error_deserialises_unknown_kind_is_forward_compatible() {
        let json = r#"{"kind":"newFutureKind","message":"Something new happened"}"#;
        let err: IpcError = serde_json::from_str(json).unwrap();
        assert_eq!(err.kind, "newFutureKind");
        assert_eq!(err.message, "Something new happened");
    }

    /// `IpcError::internal("")` must produce an `internalError` kind with an
    /// empty (not null / panicking) message — callers may pass empty strings.
    #[test]
    fn test_ipc_error_internal_with_empty_string_produces_empty_message() {
        let err = IpcError::internal("");
        assert_eq!(err.kind, "internalError");
        assert_eq!(err.message, "");
    }
}
