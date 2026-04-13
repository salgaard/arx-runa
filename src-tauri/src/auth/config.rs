//! Session timeout configuration loaded from the platform local config file.
//!
//! Path: `dirs::config_dir()` joined with `arx-runa/config.json`.
//! Default if file is missing, unreadable, or malformed: 900 seconds.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

const CONFIG_SUBDIRECTORY: &str = "arx-runa";
const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_SESSION_TIMEOUT_SECONDS: u64 = 900;
const MINIMUM_SESSION_TIMEOUT_SECONDS: u64 = 60;
const MAXIMUM_SESSION_TIMEOUT_SECONDS: u64 = 86_400;
const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct SessionConfigFile {
    schema_version: u32,
    session_timeout_secs: u64,
}

/// Returns the session timeout duration clamped to
/// `[MINIMUM_SESSION_TIMEOUT_SECONDS, MAXIMUM_SESSION_TIMEOUT_SECONDS]`.
/// Reads `dirs::config_dir() / "arx-runa/config.json"`. On any failure
/// (missing file, I/O error, invalid JSON, unknown schema version) logs a
/// warning and returns the default of 900 seconds.
pub fn load_session_timeout() -> Duration {
    let path = match config_file_path() {
        Some(path) => path,
        None => {
            tracing::warn!("no platform config directory available; using default session timeout");
            return default_session_timeout();
        }
    };

    load_session_timeout_from_path(&path)
}

/// Loads and parses a session timeout from an explicit file path.
fn load_session_timeout_from_path(path: &Path) -> Duration {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return default_session_timeout();
        }
        Err(error) => {
            tracing::warn!(?error, "failed to read session config; using default");
            return default_session_timeout();
        }
    };

    parse_config_bytes(&raw)
}

/// Parses `raw` session config JSON into a clamped timeout duration.
fn parse_config_bytes(raw: &str) -> Duration {
    let parsed: SessionConfigFile = match serde_json::from_str(raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!(?error, "invalid session config JSON; using default");
            return default_session_timeout();
        }
    };

    if parsed.schema_version != CURRENT_SCHEMA_VERSION {
        tracing::warn!(
            version = parsed.schema_version,
            "unknown session config schema version; using default",
        );
        return default_session_timeout();
    }

    let clamped = parsed.session_timeout_secs.clamp(
        MINIMUM_SESSION_TIMEOUT_SECONDS,
        MAXIMUM_SESSION_TIMEOUT_SECONDS,
    );
    if clamped != parsed.session_timeout_secs {
        tracing::warn!(
            requested = parsed.session_timeout_secs,
            clamped,
            "session timeout outside allowed range; using clamped value",
        );
    }

    Duration::from_secs(clamped)
}

/// Resolves the platform config file path for `config.json`.
fn config_file_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push(CONFIG_SUBDIRECTORY);
    path.push(CONFIG_FILE_NAME);
    Some(path)
}

/// Returns the default session timeout duration.
fn default_session_timeout() -> Duration {
    Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SESSION_TIMEOUT_SECONDS, MAXIMUM_SESSION_TIMEOUT_SECONDS,
        MINIMUM_SESSION_TIMEOUT_SECONDS, load_session_timeout_from_path, parse_config_bytes,
    };

    #[test]
    fn test_load_session_timeout_returns_default_when_file_is_missing() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let missing_path = directory.path().join("missing-config.json");

        let timeout = load_session_timeout_from_path(&missing_path);

        assert_eq!(timeout.as_secs(), DEFAULT_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_returns_default_for_invalid_json() {
        let timeout = parse_config_bytes("{not valid json");

        assert_eq!(timeout.as_secs(), DEFAULT_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_returns_default_for_unknown_schema_version() {
        let timeout = parse_config_bytes(
            r#"{
                "schema_version": 2,
                "session_timeout_secs": 900
            }"#,
        );

        assert_eq!(timeout.as_secs(), DEFAULT_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_returns_default_for_invalid_schema_shape() {
        let timeout = parse_config_bytes(
            r#"{
                "schema_version": 1
            }"#,
        );

        assert_eq!(timeout.as_secs(), DEFAULT_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_clamps_below_minimum_to_60s() {
        let timeout = parse_config_bytes(
            r#"{
                "schema_version": 1,
                "session_timeout_secs": 15
            }"#,
        );

        assert_eq!(timeout.as_secs(), MINIMUM_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_clamps_above_maximum_to_86400s() {
        let timeout = parse_config_bytes(
            r#"{
                "schema_version": 1,
                "session_timeout_secs": 90000
            }"#,
        );

        assert_eq!(timeout.as_secs(), MAXIMUM_SESSION_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_parse_config_bytes_returns_exact_value_when_in_range() {
        let timeout = parse_config_bytes(
            r#"{
                "schema_version": 1,
                "session_timeout_secs": 1200
            }"#,
        );

        assert_eq!(timeout.as_secs(), 1200);
    }
}
