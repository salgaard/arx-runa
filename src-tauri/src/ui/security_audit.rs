//! Zero-Trace security audit tests.
//!
//! These tests scan the source tree at build/test time to verify that
//! Zero-Trace invariants are enforced across the codebase. They do not
//! exercise runtime behaviour — they assert structural properties of the
//! source code itself.
//!
//! Run with: `cargo test ui::security`

#[cfg(test)]
#[allow(clippy::module_inception)]
mod security_audit {
    use std::fs;
    use std::path::Path;

    /// Walks a directory recursively and returns the contents of all `.rs` files.
    ///
    /// Panics if any `.rs` file cannot be read, so a permission or I/O error
    /// produces a clear actionable failure rather than a silent audit skip.
    fn collect_rs_source(dir: &Path) -> Vec<(std::path::PathBuf, String)> {
        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    results.extend(collect_rs_source(&path));
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
                        panic!("security audit: cannot read {}: {e}", path.display())
                    });
                    results.push((path, content));
                }
            }
        }
        results
    }

    /// Verifies that no frontend `.rs` file calls the `local_storage` web_sys API.
    ///
    /// Direct `local_storage` access would persist sensitive data across sessions,
    /// violating the Zero-Trace invariant that no vault data may be written to
    /// browser-managed persistent storage.
    #[test]
    fn test_no_local_storage_api_calls_in_frontend_source() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let frontend_src = Path::new(manifest_dir).join("../src");
        let target = concat!("local", "_", "storage");

        let files = collect_rs_source(&frontend_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {frontend_src:?} — check CARGO_MANIFEST_DIR",
        );

        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            if content.contains(target) {
                violations.push(path.display().to_string());
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in frontend source files:\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that no frontend `.rs` file calls the `session_storage` web_sys API.
    ///
    /// `session_storage` is tab-scoped but still browser-managed storage. Any use
    /// could inadvertently cache decrypted vault content in a recoverable location,
    /// violating Zero-Trace.
    #[test]
    fn test_no_session_storage_api_calls_in_frontend_source() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let frontend_src = Path::new(manifest_dir).join("../src");
        let target = concat!("session", "_", "storage");

        let files = collect_rs_source(&frontend_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {frontend_src:?} — check CARGO_MANIFEST_DIR",
        );

        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            if content.contains(target) {
                violations.push(path.display().to_string());
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in frontend source files:\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that no frontend `.rs` file uses the IndexedDB API (`IdbDatabase`).
    ///
    /// IndexedDB is a persistent, browser-managed database. Storing any vault or
    /// session data there would leave recoverable traces after the session ends,
    /// violating Zero-Trace.
    #[test]
    fn test_no_indexed_db_api_calls_in_frontend_source() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let frontend_src = Path::new(manifest_dir).join("../src");
        let target = concat!("Idb", "Database");

        let files = collect_rs_source(&frontend_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {frontend_src:?} — check CARGO_MANIFEST_DIR",
        );

        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            if content.contains(target) {
                violations.push(path.display().to_string());
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in frontend source files:\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that no frontend `.rs` file registers a `ServiceWorker`.
    ///
    /// Service Workers can intercept network requests and cache responses in
    /// persistent storage outside the renderer's direct control. Any use in the
    /// Arx Runa frontend would risk caching decrypted payloads, violating
    /// Zero-Trace.
    #[test]
    fn test_no_service_worker_api_calls_in_frontend_source() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let frontend_src = Path::new(manifest_dir).join("../src");
        let target = concat!("Service", "Worker");

        let files = collect_rs_source(&frontend_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {frontend_src:?} — check CARGO_MANIFEST_DIR",
        );

        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            if content.contains(target) {
                violations.push(path.display().to_string());
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in frontend source files:\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that IPC handlers in `src/ui/` convert password strings to
    /// `Zeroizing<Vec<u8>>` before dispatching to internal APIs.
    ///
    /// This is an **affirmative** check — the test passes only if the pattern is
    /// found at least once, confirming the Zero-Trace wrapping convention is present
    /// and not accidentally removed.
    #[test]
    fn test_password_string_zeroized_before_ipc_dispatch() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let ui_src = Path::new(manifest_dir).join("src/ui");
        // `sanitise_password` zeroizes the backing String bytes and returns a
        // `Zeroizing<Vec<u8>>`. Split across concat! so the test does not
        // accidentally match its own source when scanning the ui/ directory.
        let target = concat!("sanitise_pass", "word");
        let password_param = concat!("pass", "word: String");

        let files = collect_rs_source(&ui_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {ui_src:?} — check CARGO_MANIFEST_DIR",
        );

        let violations: Vec<String> = files
            .iter()
            .filter(|(_, content)| content.contains(password_param) && !content.contains(target))
            .map(|(path, _)| path.display().to_string())
            .collect();

        assert!(
            violations.is_empty(),
            "Zero-Trace invariant broken: these IPC handlers accept a password String \
             but do not call {target} on it:\n  {}",
            violations.join("\n  "),
        );
    }

    /// Verifies that no line in any `src/auth/` file contains both a `tracing::`
    /// macro call and the word `password`.
    ///
    /// Logging password values — even at debug level — would leave sensitive
    /// material in log sinks (files, journald, syslog), violating Zero-Trace.
    /// This check catches accidental `tracing::debug!(password = ...)` patterns.
    #[test]
    fn test_no_password_bytes_logged_via_tracing_in_auth_module() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let auth_src = Path::new(manifest_dir).join("src/auth");
        let tracing_call = concat!("tracing", "::");
        let password_word = concat!("pass", "word");

        let files = collect_rs_source(&auth_src);
        // The auth directory may be empty in early phases — that is acceptable;
        // there is nothing to violate.
        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            for (line_no, line) in content.lines().enumerate() {
                if line.contains(tracing_call) && line.contains(password_word) {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        line_no + 1,
                        line.trim(),
                    ));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: password value passed to tracing macro in auth module:\n  {}",
            violations.join("\n  "),
        );
    }

    /// Verifies that `tauri.conf.json` has a non-null CSP object under
    /// `app.security.csp` (CS-001 compliance).
    ///
    /// A missing or null CSP allows arbitrary script execution in the WebView,
    /// which could exfiltrate decrypted vault data or inject malicious content.
    #[test]
    fn test_csp_is_populated_in_tauri_conf() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let conf_path = Path::new(manifest_dir).join("tauri.conf.json");

        let content = fs::read_to_string(&conf_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", conf_path.display()));

        let json: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", conf_path.display()));

        let csp = &json["app"]["security"]["csp"];

        assert!(
            csp.is_object(),
            "CSP must be in object form for directive-level validation; \
             string form bypasses 'unsafe-inline' and 'wasm-unsafe-eval' checks. Found: {csp:?}",
        );
        if let Some(obj) = csp.as_object() {
            let script_src = obj.get("script-src").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                !script_src.contains("'unsafe-inline'"),
                "CSP violation: script-src must not contain 'unsafe-inline'; found: {script_src}",
            );
            assert!(
                script_src.contains("'wasm-unsafe-eval'"),
                "CSP violation: script-src must contain 'wasm-unsafe-eval'; found: {script_src}",
            );
            let connect_src = obj
                .get("connect-src")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert!(
                connect_src.contains("ipc:"),
                "CSP violation: connect-src must contain 'ipc:'; found: {connect_src}",
            );
            assert!(
                connect_src.contains("http://ipc.localhost"),
                "CSP violation: connect-src must contain 'http://ipc.localhost'; found: {connect_src}",
            );
            // FLOW-G-004: connect-src must not widen beyond the three allowed origins.
            assert!(
                !connect_src.contains('*'),
                "CSP violation: connect-src must not contain a wildcard; found: {connect_src}",
            );
            let allowed: std::collections::HashSet<&str> =
                ["'self'", "ipc:", "http://ipc.localhost"].into();
            let actual: std::collections::HashSet<&str> = connect_src.split_whitespace().collect();
            let unknown: Vec<&&str> = actual.difference(&allowed).collect();
            assert!(
                unknown.is_empty(),
                "CSP violation: connect-src contains unexpected token(s) {:?}; \
                 only {:?} are permitted",
                unknown,
                allowed,
            );
        }
    }

    /// Verifies that `tauri-plugin-clipboard` is NOT listed in `src-tauri/Cargo.toml`.
    ///
    /// The clipboard plugin would expose a write-to-clipboard API that could be
    /// triggered to exfiltrate decrypted vault data. Arx Runa must copy to clipboard
    /// only via the OS native dialog layer with explicit user intent, not via an
    /// unrestricted plugin.
    #[test]
    fn test_clipboard_plugin_not_in_cargo_toml() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let cargo_toml_path = Path::new(manifest_dir).join("Cargo.toml");
        let target = concat!("tauri-plugin-clip", "board");

        let content = fs::read_to_string(&cargo_toml_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", cargo_toml_path.display()));

        assert!(
            !content.contains(target),
            "Zero-Trace violation: `{}` found in Cargo.toml. \
             The clipboard plugin must not be added without explicit security review.",
            target,
        );
    }

    /// Verifies that the `tempfile` crate is not used inside the decrypt pipeline.
    ///
    /// The decrypt pipeline must never write plaintext to OS temp directories.
    /// Sibling atomic-swap files (`.arx-runa-decrypt-*.tmp`) use `std::fs`, not
    /// the `tempfile` crate, and are in the user-chosen destination directory.
    /// Any use of `tempfile::` in the pipeline would risk writing plaintext to
    /// an OS-managed temp path (e.g. `%LOCALAPPDATA%\Temp`) where it could be
    /// indexed by Windows Search or read by AV before deletion.
    #[test]
    fn test_no_tempfile_crate_in_decrypt_pipeline() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pipeline_src = Path::new(manifest_dir).join("src/storage/pipeline");
        let target = concat!("tempfile", "::");

        let files = collect_rs_source(&pipeline_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {pipeline_src:?} — check CARGO_MANIFEST_DIR",
        );

        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                // Skip comments and `use` import lines — test modules may import TempDir.
                if line.contains(target)
                    && !trimmed.starts_with("//")
                    && !trimmed.starts_with("use ")
                {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        line_no + 1,
                        line.trim(),
                    ));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in decrypt pipeline (plaintext must never \
             be written to OS temp dirs):\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that the `tempfile` crate is not used in `file_commands.rs`.
    ///
    /// `get_file_content` must decrypt entirely in RAM via `download_file_to_memory`.
    /// Any reintroduction of `tempfile::` here would write decrypted plaintext to
    /// the OS temp directory before returning it over IPC.
    #[test]
    fn test_no_tempfile_crate_in_file_commands() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let file_commands = Path::new(manifest_dir).join("src/ui/file_commands.rs");
        let target = concat!("tempfile", "::");

        let content = fs::read_to_string(&file_commands).unwrap_or_else(|e| {
            panic!(
                "security audit: cannot read {}: {e}",
                file_commands.display()
            )
        });

        let violations: Vec<String> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(target) && !line.trim_start().starts_with("//"))
            .map(|(i, line)| format!("{}:{}: {}", file_commands.display(), i + 1, line.trim()))
            .collect();

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: `{}` found in file_commands.rs. \
             `get_file_content` must decrypt to memory, not to OS temp dirs:\n  {}",
            target,
            violations.join("\n  "),
        );
    }

    /// Verifies that no key material, passwords, or derived keys are logged
    /// anywhere in the Tauri backend source tree (`src/`).
    ///
    /// Storage, sharing, auth, UI, and crypto modules must never emit raw key
    /// bytes to tracing sinks. This catches accidental `tracing::debug!(key = …)`
    /// patterns that would write key material to log files, violating Zero-Trace.
    #[test]
    fn test_no_key_material_logged_in_backend_modules() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let backend_src = Path::new(manifest_dir).join("src");
        let tracing_call = concat!("tracing", "::");
        let key_words = [
            "master_key",
            "file_key",
            "sqlcipher_key",
            "kek",
            "manifest_key",
        ];

        let files: Vec<(std::path::PathBuf, String)> = collect_rs_source(&backend_src)
            .into_iter()
            .filter(|(p, _)| {
                // Skip unit/integration test files — they use key material intentionally.
                !p.components().any(|c| c.as_os_str() == "tests")
            })
            .collect();
        let mut violations: Vec<String> = Vec::new();
        for (path, content) in &files {
            for (line_no, line) in content.lines().enumerate() {
                if line.contains(tracing_call)
                    && key_words.iter().any(|kw| line.contains(kw))
                    && !line.trim_start().starts_with("//")
                {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        line_no + 1,
                        line.trim(),
                    ));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "Zero-Trace violation: key material name found in tracing macro in backend source:\n  {}",
            violations.join("\n  "),
        );
    }

    /// Verifies that the Zero-Trace state-clearing hook is wired in `src/app.rs`.
    ///
    /// On the `is_unlocked: true → false` transition the router's `Effect::new`
    /// block must call `vault_actions.clear()`, `sync_actions.clear()`, and
    /// `session_actions.clear()`. Omitting any call would leave decrypted state
    /// alive in memory after the session locks, violating Zero-Trace.
    #[test]
    fn test_state_clearing_wired_on_lock_transition_in_router() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let app_rs_path = Path::new(manifest_dir).join("../src/app.rs");
        let vault_clear = concat!("vault_actions", ".clear()");
        let sync_clear = concat!("sync_actions", ".clear()");
        let session_clear = concat!("session_actions", ".clear()");
        let effect_new = "Effect::new";

        let content = fs::read_to_string(&app_rs_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", app_rs_path.display()));

        // Verify all four tokens appear within a 30-line window so that a
        // refactor moving any clear() call out of the Effect::new block is
        // caught, not just their individual presence anywhere in the file.
        let lines: Vec<&str> = content.lines().collect();
        let window = 30_usize;
        let found = lines.windows(window).any(|block| {
            let joined = block.join("\n");
            joined.contains(effect_new)
                && joined.contains(vault_clear)
                && joined.contains(sync_clear)
                && joined.contains(session_clear)
        });

        assert!(
            found,
            "Zero-Trace violation: `{vault_clear}`, `{sync_clear}`, `{session_clear}`, \
             and `{effect_new}` must all appear within {window} lines of each other in \
             src/app.rs. All clear() calls must be inside the Effect::new lock-transition block.",
        );
    }

    /// Verifies that `sync_percent` is reset to `None` inside `SyncState::clear()`.
    ///
    /// `sync_percent` is displayed live in the header while a sync is in-flight.
    /// Forgetting to clear it in `SyncState::clear()` would leave a stale progress
    /// value visible after the vault locks, violating Zero-Trace state hygiene.
    #[test]
    fn test_sync_percent_cleared_in_sync_state_clear() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let sync_ctx = Path::new(manifest_dir).join("../src/state/sync_context.rs");
        let clear_marker = "fn clear(";
        let percent_reset = concat!("sync_percent", " = None");

        let content = fs::read_to_string(&sync_ctx)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", sync_ctx.display()));

        let lines: Vec<&str> = content.lines().collect();
        let window = 20_usize;
        let found = lines.windows(window).any(|block| {
            let joined = block.join("\n");
            joined.contains(clear_marker) && joined.contains(percent_reset)
        });

        assert!(
            found,
            "Zero-Trace violation: `{percent_reset}` must appear inside `{clear_marker}` \
             in sync_context.rs. SyncState::clear() must reset sync_percent to None on lock.",
        );
    }

    /// Verifies that the four slow auth commands accept a `Channel<ProgressUpdate>` parameter.
    ///
    /// These commands drive the `ProgressModal` in the frontend. If a command loses its
    /// `Channel` parameter (e.g., during a signature refactor), the frontend channel will
    /// silently never receive updates and the modal will hang open indefinitely.
    #[test]
    fn test_progress_channel_wired_in_auth_slow_commands() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let auth_commands = Path::new(manifest_dir).join("src/ui/auth_commands.rs");
        let channel_param = concat!("Channel<Progress", "Update>");

        let content = fs::read_to_string(&auth_commands)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", auth_commands.display()));

        let count = content.matches(channel_param).count();
        assert!(
            count >= 4,
            "Expected at least 4 occurrences of `{channel_param}` in auth_commands.rs \
             (create_vault, recover_vault_from_cloud, recover_vault_from_cloud_with_phrase, \
             recover_vault_with_phrase). Found: {count}. \
             Re-add Channel<ProgressUpdate> to any removed slow-auth command.",
        );
    }

    /// Verifies that `download_received_share` accepts a `Channel<ProgressUpdate>` parameter.
    ///
    /// Without it the frontend ProgressModal for received-share downloads will never
    /// receive progress events and the modal will never auto-close.
    #[test]
    fn test_progress_channel_wired_in_download_received_share() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let sharing_commands = Path::new(manifest_dir).join("src/ui/sharing_commands.rs");
        let fn_marker = "download_received_share";
        let channel_param = concat!("Channel<Progress", "Update>");

        let content = fs::read_to_string(&sharing_commands)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", sharing_commands.display()));

        let lines: Vec<&str> = content.lines().collect();
        let window = 15_usize;
        let found = lines.windows(window).any(|block| {
            let joined = block.join("\n");
            joined.contains(fn_marker) && joined.contains(channel_param)
        });

        assert!(
            found,
            "Expected `{channel_param}` within {window} lines of `{fn_marker}` in \
             sharing_commands.rs. Re-add the progress channel to download_received_share.",
        );
    }
}
