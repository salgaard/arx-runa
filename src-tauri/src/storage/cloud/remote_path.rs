//! Remote path validation for rclone cloud operations.
//!
//! Anchored to `design.md#remote-path-sanitisation`.

use std::sync::OnceLock;

use regex::Regex;

use super::CloudTransportError;

const REMOTE_PATH_REGEX: &str = r"^[a-zA-Z0-9._/-]+$";

fn allowlist_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(REMOTE_PATH_REGEX).expect("remote path regex must compile"))
}

fn remote_name_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._-]*$").expect("remote name regex must compile")
    })
}

/// Validates a cloud-relative path used for upload/download/delete/list operations.
pub fn validate_remote_path(remote_path: &str) -> Result<&str, CloudTransportError> {
    if remote_path.is_empty() {
        return Err(CloudTransportError::Other(
            "remote path rejected: path is empty".to_owned(),
        ));
    }
    if remote_path.starts_with('/') {
        return Err(CloudTransportError::Other(
            "remote path rejected: absolute paths are not allowed".to_owned(),
        ));
    }
    if remote_path.contains("..") {
        return Err(CloudTransportError::Other(
            "remote path rejected: parent traversal is not allowed".to_owned(),
        ));
    }
    if remote_path.chars().any(char::is_control) {
        return Err(CloudTransportError::Other(
            "remote path rejected: control characters are not allowed".to_owned(),
        ));
    }
    if !allowlist_regex().is_match(remote_path) {
        return Err(CloudTransportError::Other(
            "remote path rejected: only [a-zA-Z0-9._/-] is allowed".to_owned(),
        ));
    }
    Ok(remote_path)
}

/// Validates a remote prefix. Empty prefix is accepted for list-all operations.
pub fn validate_remote_prefix(remote_prefix: &str) -> Result<&str, CloudTransportError> {
    if remote_prefix.is_empty() {
        return Ok(remote_prefix);
    }
    validate_remote_path(remote_prefix)
}

/// Validates remote root components and composes a canonical `remote:root` target.
pub fn compose_remote_root(
    remote_name: &str,
    bucket: &str,
    path_prefix: &str,
) -> Result<String, CloudTransportError> {
    let remote_name = validate_remote_name(remote_name)?;
    let bucket = validate_remote_root_component("bucket", bucket, true)?;
    let path_prefix = validate_remote_root_component("path_prefix", path_prefix, true)?;

    let mut root_path = String::new();
    if !bucket.is_empty() {
        root_path.push_str(bucket);
    }
    if !path_prefix.is_empty() {
        if !root_path.is_empty() {
            root_path.push('/');
        }
        root_path.push_str(path_prefix);
    }

    if root_path.is_empty() {
        Ok(format!("{remote_name}:"))
    } else {
        Ok(format!("{remote_name}:{root_path}"))
    }
}

pub(crate) fn validate_remote_name(remote_name: &str) -> Result<&str, CloudTransportError> {
    if remote_name.is_empty() {
        return Err(CloudTransportError::Other(
            "remote root rejected: remote_name is empty".to_owned(),
        ));
    }
    if remote_name.chars().any(char::is_control) {
        return Err(CloudTransportError::Other(
            "remote root rejected: remote_name contains control characters".to_owned(),
        ));
    }
    if !remote_name_regex().is_match(remote_name) {
        return Err(CloudTransportError::Other(
            "remote root rejected: remote_name must match [a-zA-Z0-9][a-zA-Z0-9._-]*".to_owned(),
        ));
    }
    Ok(remote_name)
}

pub(crate) fn validate_remote_root_component<'a>(
    label: &str,
    value: &'a str,
    allow_empty: bool,
) -> Result<&'a str, CloudTransportError> {
    if value.is_empty() && allow_empty {
        return Ok(value);
    }
    if value.is_empty() {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} is empty"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} contains control characters"
        )));
    }
    if value.starts_with('/') || value.ends_with('/') {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} must not start or end with '/'"
        )));
    }
    if value.contains("//") {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} must not contain empty path segments"
        )));
    }
    if value.contains("..") {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} must not contain parent traversal"
        )));
    }
    if value.contains(':') {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} must not contain ':'"
        )));
    }
    if value.contains('\\') {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} must not contain '\\\\'"
        )));
    }
    if !allowlist_regex().is_match(value) {
        return Err(CloudTransportError::Other(format!(
            "remote root rejected: {label} contains disallowed characters"
        )));
    }
    Ok(value)
}

/// Validates a local `path_prefix` before embedding it in an rclone local-remote root.
///
/// Accepts absolute paths used for `LocalPath` and `ExternalDrive` destinations, including
/// Windows paths with a drive-letter prefix (`D:/vault`).  Backslashes must already be
/// normalized to forward slashes by the caller before this function is invoked.
///
/// Rejects:
/// - Empty string
/// - Control characters
/// - `..` path traversal segments
/// - Consecutive slashes (`//`)
/// - Colons anywhere other than position 1 of a Windows drive prefix (`X:`)
pub(crate) fn validate_local_path_prefix(path: &str) -> Result<(), CloudTransportError> {
    if path.is_empty() {
        return Err(CloudTransportError::Other(
            "local path_prefix rejected: path is empty".to_owned(),
        ));
    }
    if path.chars().any(char::is_control) {
        return Err(CloudTransportError::Other(
            "local path_prefix rejected: path contains control characters".to_owned(),
        ));
    }
    if path.contains("..") {
        return Err(CloudTransportError::Other(
            "local path_prefix rejected: path must not contain parent traversal (..)".to_owned(),
        ));
    }
    if path.contains("//") {
        return Err(CloudTransportError::Other(
            "local path_prefix rejected: path must not contain empty path segments (//)".to_owned(),
        ));
    }
    // Allow exactly one colon at position 1 (Windows drive letter, e.g. "C:/vault").
    // Any colon at other positions is a shell-injection / rclone remote confusion risk.
    let path_after_drive = if path.len() >= 2
        && path.as_bytes()[1] == b':'
        && path.as_bytes()[0].is_ascii_alphabetic()
    {
        &path[2..]
    } else {
        path
    };
    if path_after_drive.contains(':') {
        return Err(CloudTransportError::Other(
            "local path_prefix rejected: path must not contain ':' except for a leading drive letter"
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        compose_remote_root, validate_local_path_prefix, validate_remote_name,
        validate_remote_path, validate_remote_prefix,
    };
    use crate::storage::cloud::CloudTransportError;

    #[test]
    fn test_validate_remote_path_accepts_common_safe_paths() {
        assert_eq!(
            validate_remote_path("vault-header.json").unwrap(),
            "vault-header.json"
        );
        assert_eq!(
            validate_remote_path("vault/uuid.blob").unwrap(),
            "vault/uuid.blob"
        );
    }

    #[test]
    fn test_validate_remote_path_rejects_parent_traversal() {
        let result = validate_remote_path("../escape");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_path_rejects_absolute_path() {
        let result = validate_remote_path("/abs");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_path_rejects_space() {
        let result = validate_remote_path("has space");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_path_rejects_nul() {
        let result = validate_remote_path("nul\0byte");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_prefix_accepts_empty() {
        assert_eq!(validate_remote_prefix("").unwrap(), "");
    }

    #[test]
    fn test_compose_remote_root_with_bucket_and_prefix() {
        let root = compose_remote_root("remote-a", "bucket", "vault").unwrap();
        assert_eq!(root, "remote-a:bucket/vault");
    }

    #[test]
    fn test_compose_remote_root_allows_prefix_without_bucket() {
        let root = compose_remote_root("remote-a", "", "vault").unwrap();
        assert_eq!(root, "remote-a:vault");
    }

    #[test]
    fn test_compose_remote_root_rejects_invalid_remote_name() {
        let result = compose_remote_root("bad:name", "bucket", "vault");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_compose_remote_root_rejects_bucket_with_slash_boundary() {
        let result = compose_remote_root("remote", "/bucket", "vault");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_name_rejects_closing_bracket() {
        let result = validate_remote_name("bad]name");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_name_rejects_newline() {
        let result = validate_remote_name("bad\nname");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_name_rejects_equals_sign() {
        let result = validate_remote_name("bad=name");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_remote_name_accepts_arx_uuid_prefix_format() {
        assert!(validate_remote_name("arx_550e8400").is_ok());
    }

    #[test]
    fn test_validate_local_path_prefix_accepts_unix_absolute_path() {
        assert!(validate_local_path_prefix("/home/user/vault").is_ok());
    }

    #[test]
    fn test_validate_local_path_prefix_accepts_windows_drive_path() {
        assert!(validate_local_path_prefix("D:/vault/arx").is_ok());
    }

    #[test]
    fn test_validate_local_path_prefix_accepts_relative_path() {
        assert!(validate_local_path_prefix("vault/data").is_ok());
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_empty() {
        let result = validate_local_path_prefix("");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_parent_traversal() {
        let result = validate_local_path_prefix("../../home/other");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_parent_traversal_in_middle() {
        let result = validate_local_path_prefix("/home/user/../other");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_control_character() {
        let result = validate_local_path_prefix("/home/user/\x00vault");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_double_slash() {
        let result = validate_local_path_prefix("/home//vault");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_colon_in_path_body() {
        let result = validate_local_path_prefix("/home/user/va:ult");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_validate_local_path_prefix_rejects_drive_letter_with_extra_colon() {
        let result = validate_local_path_prefix("D:/vault:bad");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }
}
