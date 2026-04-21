/// Zero-Trace security audit tests.
///
/// These tests scan the source tree at build/test time to verify that
/// Zero-Trace invariants are enforced across the codebase. They do not
/// exercise runtime behaviour — they assert structural properties of the
/// source code itself.
///
/// Run with: `cargo test ui::security`

#[cfg(test)]
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
        let target = concat!("Zeroizing::new(pass", "word");

        let files = collect_rs_source(&ui_src);
        assert!(
            !files.is_empty(),
            "No .rs files found under {ui_src:?} — check CARGO_MANIFEST_DIR",
        );

        let found = files.iter().any(|(_, content)| content.contains(target));

        assert!(
            found,
            "Zero-Trace invariant broken: no IPC handler in `src/ui/` contains `{}`. \
             Password strings must be wrapped with Zeroizing::new() before dispatch.",
            target,
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

        // Tauri v2 accepts CSP as either a structured JSON object or a plain string;
        // both forms are valid and non-null. Reject only null/absent values.
        assert!(
            csp.is_object() || csp.is_string(),
            "CS-001 violation: `app.security.csp` in tauri.conf.json must be a non-null \
             string or object. Found: {csp:?}",
        );
        if let Some(s) = csp.as_str() {
            assert!(
                !s.trim().is_empty(),
                "CS-001 violation: `app.security.csp` in tauri.conf.json is an empty string",
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

    /// Verifies that the Zero-Trace state-clearing hook is wired in `src/app.rs`.
    ///
    /// On the `is_unlocked: true → false` transition the router's `Effect::new`
    /// block must call both `vault_actions.clear()` and `sync_actions.clear()`.
    /// Omitting either call would leave decrypted vault or sync state alive in
    /// memory after the session locks, violating Zero-Trace.
    #[test]
    fn test_state_clearing_wired_on_lock_transition_in_router() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let app_rs_path = Path::new(manifest_dir).join("../src/app.rs");
        let vault_clear = concat!("vault_actions", ".clear()");
        let sync_clear = concat!("sync_actions", ".clear()");
        let effect_new = "Effect::new";

        let content = fs::read_to_string(&app_rs_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", app_rs_path.display()));

        // Verify all three tokens appear within a 30-line window so that a
        // refactor moving either clear() call out of the Effect::new block is
        // caught, not just their individual presence anywhere in the file.
        let lines: Vec<&str> = content.lines().collect();
        let window = 30_usize;
        let found = lines.windows(window).any(|block| {
            let joined = block.join("\n");
            joined.contains(effect_new)
                && joined.contains(vault_clear)
                && joined.contains(sync_clear)
        });

        assert!(
            found,
            "Zero-Trace violation: `{vault_clear}`, `{sync_clear}`, and `{effect_new}` \
             must all appear within {window} lines of each other in src/app.rs. \
             Both clear() calls must be inside the Effect::new lock-transition block.",
        );
    }
}
