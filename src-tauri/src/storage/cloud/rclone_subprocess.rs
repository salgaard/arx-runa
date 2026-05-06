//! Rclone subprocess runner.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use super::CloudTransportError;
use super::stderr_sanitiser::sanitise_stderr;

/// Executes the rclone sidecar with typed arguments and timeout handling.
pub(crate) async fn run_rclone(
    binary_path: &Path,
    args: Vec<OsString>,
    timeout: Duration,
) -> Result<String, CloudTransportError> {
    let mut child = tokio::process::Command::new(binary_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| CloudTransportError::Other("failed to capture rclone stdout".to_owned()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| CloudTransportError::Other("failed to capture rclone stderr".to_owned()))?;

    let stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await?;
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(wait_result) => wait_result?,
        Err(_) => {
            if let Err(error) = child.start_kill() {
                tracing::warn!(error = %error, "failed to start-kill timed out rclone child");
            }
            if let Err(error) = tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                tracing::warn!(error = %error, "failed to reap timed out rclone child");
            }
            stdout_task.abort();
            stderr_task.abort();
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(CloudTransportError::Timeout);
        }
    };

    let stdout_bytes = stdout_task
        .await
        .map_err(|error| CloudTransportError::Other(error.to_string()))??;
    let stderr_bytes = stderr_task
        .await
        .map_err(|error| CloudTransportError::Other(error.to_string()))??;

    let stdout_text = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes).into_owned();

    let stderr_sanitised = sanitise_stderr(&stderr_text);
    match status.code() {
        Some(0) => Ok(stdout_text),
        Some(exit_code) => classify_non_zero_exit(exit_code, &stderr_text, &stderr_sanitised),
        None => Err(CloudTransportError::RcloneProcessFailed {
            exit_code: -1,
            stderr_sanitised,
        }),
    }
}

fn classify_non_zero_exit(
    exit_code: i32,
    stderr_raw: &str,
    stderr_sanitised: &str,
) -> Result<String, CloudTransportError> {
    if is_authentication_failure(stderr_raw) {
        tracing::warn!(
            exit_code = exit_code,
            stderr = %stderr_sanitised,
            "rclone authentication failure — check cloud credentials"
        );
        return Err(CloudTransportError::AuthenticationFailed);
    }
    if matches!(exit_code, 3 | 4) {
        return Err(CloudTransportError::NotFound);
    }
    tracing::warn!(
        exit_code = exit_code,
        stderr = %stderr_sanitised,
        "rclone process failed"
    );
    Err(CloudTransportError::RcloneProcessFailed {
        exit_code,
        stderr_sanitised: stderr_sanitised.to_owned(),
    })
}

fn is_authentication_failure(stderr_sanitised: &str) -> bool {
    let normalized = stderr_sanitised.to_ascii_lowercase();
    const AUTH_FAILURE_PATTERNS: [&str; 8] = [
        "authentication failed",
        "auth failed",
        "invalid credentials",
        "access denied",
        "unauthorized",
        "authorisation failed",
        "forbidden",
        "login failed",
    ];
    AUTH_FAILURE_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::run_rclone;
    use crate::storage::cloud::CloudTransportError;

    #[cfg(unix)]
    fn fixture_binary() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_rclone.sh");
        let metadata = std::fs::metadata(&path).expect("fixture metadata should be readable");
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("fixture should be executable");
        path
    }

    #[cfg(windows)]
    fn fixture_binary() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_rclone.cmd")
    }

    #[tokio::test]
    async fn test_run_rclone_exit_zero_returns_stdout() {
        let result = run_rclone(
            &fixture_binary(),
            vec![OsString::from("ok"), OsString::from("hello")],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(result.trim(), "hello");
    }

    #[tokio::test]
    async fn test_run_rclone_exit_three_maps_to_not_found() {
        let result = run_rclone(
            &fixture_binary(),
            vec![
                OsString::from("status"),
                OsString::from("3"),
                OsString::from("missing"),
            ],
            Duration::from_secs(2),
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_run_rclone_exit_four_maps_to_not_found() {
        let result = run_rclone(
            &fixture_binary(),
            vec![
                OsString::from("status"),
                OsString::from("4"),
                OsString::from("missing"),
            ],
            Duration::from_secs(2),
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_run_rclone_non_zero_maps_to_rclone_process_failed() {
        let result = run_rclone(
            &fixture_binary(),
            vec![
                OsString::from("status"),
                OsString::from("7"),
                OsString::from("plain failure"),
            ],
            Duration::from_secs(2),
        )
        .await;

        match result {
            Err(CloudTransportError::RcloneProcessFailed {
                exit_code,
                stderr_sanitised,
            }) => {
                assert_eq!(exit_code, 7);
                assert_eq!(stderr_sanitised, "plain failure");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_run_rclone_auth_failure_maps_before_generic_process_failure() {
        let result = run_rclone(
            &fixture_binary(),
            vec![
                OsString::from("status"),
                OsString::from("7"),
                OsString::from("Authentication failed: invalid credentials"),
            ],
            Duration::from_secs(2),
        )
        .await;
        assert!(matches!(
            result,
            Err(CloudTransportError::AuthenticationFailed)
        ));
    }

    #[tokio::test]
    async fn test_run_rclone_auth_failure_detected_from_raw_stderr_before_sanitise() {
        let result = run_rclone(
            &fixture_binary(),
            vec![
                OsString::from("status"),
                OsString::from("7"),
                OsString::from("AUTH failed token=abc"),
            ],
            Duration::from_secs(2),
        )
        .await;
        assert!(matches!(
            result,
            Err(CloudTransportError::AuthenticationFailed)
        ));
    }

    #[tokio::test]
    async fn test_run_rclone_timeout_maps_to_timeout() {
        let result = run_rclone(
            &fixture_binary(),
            vec![OsString::from("sleep"), OsString::from("10000")],
            Duration::from_millis(100),
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::Timeout)));
    }
}
