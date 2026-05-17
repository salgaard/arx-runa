//! Input validators for Tauri IPC command parameters.
//!
//! Validators return `IpcError::InvalidInput` with user-safe messages.
//! No internal details are included in validation error messages.

use std::sync::LazyLock;

use regex::Regex;
use uuid::Uuid;

use crate::ui::error::IpcError;

/// Allowlist regex for vault-relative paths. Permits Unicode letters and
/// digits (`\p{L}\p{N}`) so filenames with Danish and other non-ASCII
/// characters are accepted.
const VAULT_PATH_ALLOWLIST: &str = r"^[\p{L}\p{N} ._/\-]*$";

/// Minimum chunk size in bytes (128 KiB).
const MIN_CHUNK_SIZE_BYTES: u64 = 131_072;

/// Maximum chunk size in bytes (64 MiB).
const MAX_CHUNK_SIZE_BYTES: u64 = 67_108_864;

/// Compiled vault path allowlist regex; initialised once on first access.
///
/// `LazyLock` guarantees the closure runs exactly once. The `expect` is
/// acceptable here because `VAULT_PATH_ALLOWLIST` is a compile-time
/// constant whose syntactic validity is verified by
/// `test_vault_path_regex_compiles_successfully`.
static VAULT_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(VAULT_PATH_ALLOWLIST).expect("vault path regex is a compile-time literal")
});

/// Returns the compiled vault path allowlist regex.
fn vault_path_regex() -> &'static Regex {
    &VAULT_PATH_REGEX
}

/// Validates a vault-relative path.
///
/// Accepts empty string (root directory equivalent). Rejects:
/// - Backslashes (`\`)
/// - Absolute paths (leading `/`)
/// - Path traversal sequences (`..`)
/// - Characters outside the allowlist `[\p{L}\p{N} ._/-]` (Unicode letters/digits allowed)
/// - ASCII control characters (U+0000–U+001F)
pub(crate) fn validate_vault_path(path: &str) -> Result<(), IpcError> {
    if path.contains('\\') {
        return Err(IpcError::InvalidInput(
            "Path must not contain backslashes".into(),
        ));
    }
    if path.starts_with('/') {
        return Err(IpcError::InvalidInput(
            "Path must be relative; absolute paths are not allowed".into(),
        ));
    }
    if path.split('/').any(|s| s == "..") {
        return Err(IpcError::InvalidInput(
            "Path must not contain traversal sequences".into(),
        ));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(IpcError::InvalidInput(
            "Path must not contain control characters".into(),
        ));
    }
    if !vault_path_regex().is_match(path) {
        return Err(IpcError::InvalidInput(
            "Path contains characters outside the allowed set".into(),
        ));
    }
    Ok(())
}

/// Validates a file identifier (must be a valid UUID v4).
pub(crate) fn validate_file_id(id: &str) -> Result<(), IpcError> {
    if id.is_empty() {
        return Err(IpcError::InvalidInput("File ID must not be empty".into()));
    }
    let parsed = Uuid::parse_str(id)
        .map_err(|_| IpcError::InvalidInput("File ID must be a valid UUID".into()))?;
    if parsed.get_version_num() != 4 {
        return Err(IpcError::InvalidInput(
            "File ID must be a version 4 UUID".into(),
        ));
    }
    Ok(())
}

/// Validates a password (must be non-empty).
pub(crate) fn validate_password(password: &str) -> Result<(), IpcError> {
    if password.is_empty() {
        return Err(IpcError::InvalidInput("Password must not be empty".into()));
    }
    Ok(())
}

/// Validates a chunk size in bytes.
///
/// Valid range: 131072 (128 KiB) to 67108864 (64 MiB) inclusive.
pub(crate) fn validate_chunk_size(chunk_size_bytes: u64) -> Result<(), IpcError> {
    if !(MIN_CHUNK_SIZE_BYTES..=MAX_CHUNK_SIZE_BYTES).contains(&chunk_size_bytes) {
        return Err(IpcError::InvalidInput(
            "chunk_size_bytes must be between 131072 (128 KiB) and 67108864 (64 MiB)".into(),
        ));
    }
    Ok(())
}

/// Normalises a vault-relative path by stripping any leading `'/'`.
///
/// The frontend represents the root as `"/"` and sub-directories as `"/docs"`,
/// `"/docs/reports"`, etc. This function converts those to the backend's relative
/// convention (`""`, `"docs"`, `"docs/reports"`).
///
/// **Must be called before [`validate_vault_path`]** for all user-supplied vault paths.
/// Calling `validate_vault_path` on a raw, un-normalised path causes it to be
/// rejected as an absolute path rather than accepted as a vault-relative path.
pub(crate) fn normalise_vault_path(path: &str) -> &str {
    path.trim_start_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- vault_path_regex ---

    #[test]
    fn test_vault_path_regex_compiles_successfully() {
        // Asserts that the LazyLock static initialisation does not panic.
        let _ = vault_path_regex();
    }

    // --- validate_vault_path ---

    #[test]
    fn test_validate_vault_path_empty_string_succeeds_as_root() {
        assert!(validate_vault_path("").is_ok());
    }

    #[test]
    fn test_validate_vault_path_simple_file_name_succeeds() {
        assert!(validate_vault_path("documents/report.pdf").is_ok());
    }

    #[test]
    fn test_validate_vault_path_hyphenated_name_succeeds() {
        assert!(validate_vault_path("my-file.txt").is_ok());
    }

    #[test]
    fn test_validate_vault_path_rejects_backslash() {
        assert!(validate_vault_path(r"path\with\backslash").is_err());
    }

    #[test]
    fn test_validate_vault_path_rejects_absolute_path() {
        assert!(validate_vault_path("/absolute/path").is_err());
    }

    #[test]
    fn test_validate_vault_path_rejects_traversal_sequence() {
        assert!(validate_vault_path("../escape").is_err());
    }

    #[test]
    fn test_validate_vault_path_accepts_filename_with_embedded_double_dot() {
        // "file..bak" contains ".." as a substring but not as a path segment;
        // the segment-based check must accept it.
        assert!(validate_vault_path("archive/file..bak").is_ok());
    }

    #[test]
    fn test_validate_vault_path_rejects_null_byte() {
        assert!(validate_vault_path("file\x00name").is_err());
    }

    #[test]
    fn test_validate_vault_path_rejects_control_character() {
        assert!(validate_vault_path("file\x1fname").is_err());
    }

    #[test]
    fn test_validate_vault_path_rejects_percent_encoded_traversal() {
        // '%' is not in the allowlist, so this is caught by the regex
        assert!(validate_vault_path("%2E%2E/foo").is_err());
    }

    #[test]
    fn test_validate_vault_path_accepts_danish_lowercase_letters() {
        assert!(validate_vault_path("filer/æresdoktor.pdf").is_ok());
    }

    #[test]
    fn test_validate_vault_path_accepts_danish_uppercase_letters() {
        assert!(validate_vault_path("Ø-rapport/Årsopgørelse.txt").is_ok());
    }

    #[test]
    fn test_validate_vault_path_accepts_mixed_unicode_filename() {
        assert!(validate_vault_path("dokumenter/mødereferat 2024.docx").is_ok());
    }

    // --- validate_file_id ---

    #[test]
    fn test_validate_file_id_valid_uuid_v4_succeeds() {
        // Generate a real v4 UUID
        let v4 = uuid::Uuid::new_v4().to_string();
        assert!(validate_file_id(&v4).is_ok());
    }

    #[test]
    fn test_validate_file_id_rejects_empty_string() {
        assert!(validate_file_id("").is_err());
    }

    #[test]
    fn test_validate_file_id_rejects_non_uuid_string() {
        assert!(validate_file_id("not-a-uuid").is_err());
    }

    #[test]
    fn test_validate_file_id_rejects_v1_uuid() {
        // f81d4fae-7dec-11d0-a765-00a0c91e6bf6 is a v1 UUID
        assert!(validate_file_id("f81d4fae-7dec-11d0-a765-00a0c91e6bf6").is_err());
    }

    // --- validate_password ---

    #[test]
    fn test_validate_password_rejects_empty_string() {
        assert!(validate_password("").is_err());
    }

    #[test]
    fn test_validate_password_accepts_non_empty_string() {
        assert!(validate_password("hunter2").is_ok());
    }

    // --- validate_chunk_size ---

    #[test]
    fn test_validate_chunk_size_rejects_zero() {
        assert!(validate_chunk_size(0).is_err());
    }

    #[test]
    fn test_validate_chunk_size_rejects_below_minimum() {
        assert!(validate_chunk_size(131_071).is_err());
    }

    #[test]
    fn test_validate_chunk_size_accepts_minimum() {
        assert!(validate_chunk_size(131_072).is_ok());
    }

    #[test]
    fn test_validate_chunk_size_accepts_default_4mib() {
        assert!(validate_chunk_size(4_194_304).is_ok());
    }

    #[test]
    fn test_validate_chunk_size_accepts_maximum() {
        assert!(validate_chunk_size(67_108_864).is_ok());
    }

    #[test]
    fn test_validate_chunk_size_rejects_above_maximum() {
        assert!(validate_chunk_size(67_108_865).is_err());
    }

    #[test]
    fn test_validate_chunk_size_rejects_u64_max() {
        assert!(validate_chunk_size(u64::MAX).is_err());
    }

    // --- normalise_vault_path ---

    #[test]
    fn test_normalise_vault_path_slash_becomes_empty_root() {
        assert_eq!(normalise_vault_path("/"), "");
    }

    #[test]
    fn test_normalise_vault_path_empty_string_unchanged() {
        assert_eq!(normalise_vault_path(""), "");
    }

    #[test]
    fn test_normalise_vault_path_non_root_path_unchanged() {
        assert_eq!(normalise_vault_path("documents"), "documents");
    }

    #[test]
    fn test_normalise_vault_path_absolute_sub_path_strips_leading_slash() {
        // Frontend breadcrumbs emit "/docs", "/docs/reports" — these must become relative.
        assert_eq!(normalise_vault_path("/docs"), "docs");
        assert_eq!(normalise_vault_path("/docs/reports"), "docs/reports");
    }

    // --- validate_vault_path additional boundary cases ---

    /// Verifies that an un-normalised bare `"/"` is rejected by the validator.
    ///
    /// Raw frontend input `"/"` must be passed through `normalise_vault_path` first;
    /// presenting it directly to the validator must fail.
    #[test]
    fn test_validate_vault_path_rejects_bare_slash() {
        assert!(validate_vault_path("/").is_err());
    }

    /// Verifies the canonical round-trip: frontend `"/"` → normalise → validate → ok.
    ///
    /// After normalisation the empty string (root) must pass validation.
    #[test]
    fn test_normalise_then_validate_bare_slash_succeeds_as_root() {
        let normalised = normalise_vault_path("/");
        assert!(validate_vault_path(normalised).is_ok());
    }

    /// Verifies that a multi-segment path with no traversal sequences is accepted.
    #[test]
    fn test_validate_vault_path_accepts_nested_path() {
        assert!(validate_vault_path("documents/reports/2024").is_ok());
    }

    /// Verifies that a traversal sequence embedded mid-path is rejected.
    ///
    /// Distinct from the embedded-dot test (`"file..bak"`): the segment `".."` between
    /// two real segments must still be caught by the segment-level check.
    #[test]
    fn test_validate_vault_path_rejects_double_dot_segment() {
        assert!(validate_vault_path("a/../b").is_err());
    }

    /// Verifies that a bare `".."` (single traversal segment) is rejected.
    #[test]
    fn test_validate_vault_path_rejects_only_double_dot() {
        assert!(validate_vault_path("..").is_err());
    }
}
