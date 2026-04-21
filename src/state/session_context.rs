use leptos::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::invoke::invoke_command;
use crate::ipc_types::SessionStatus;

/// Authentication and session state held in Leptos signals (RAM only).
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    /// Whether the vault session is currently active and unlocked.
    pub is_unlocked: bool,
    /// Identifier of the open vault, or `None` when locked.
    pub vault_id: Option<String>,
    /// Idle-timeout in seconds configured for this session, or `None` if no timeout is set.
    pub timeout_seconds: Option<u64>,
    /// `true` while an unlock/login IPC call is in-flight.
    /// UI-owned: `apply_status` does not overwrite this field.
    pub authenticating: bool,
    /// Last user-displayable authentication error, or `None` when clean.
    /// UI-owned: `apply_status` does not overwrite this field.
    pub error: Option<String>,
}

impl SessionState {
    /// Applies a polled `SessionStatus` without disturbing UI-owned fields.
    pub fn apply_status(&mut self, status: SessionStatus) {
        self.is_unlocked = status.is_unlocked;
        self.vault_id = status.vault_id;
        self.timeout_seconds = status.timeout_seconds;
    }

    /// Marks the beginning of an `authenticate` IPC round trip.
    pub fn begin_authenticating(&mut self) {
        self.authenticating = true;
        self.error = None;
    }

    /// Marks a successful unlock; sets `is_unlocked = true` and stores `vault_id`.
    pub fn complete_success(&mut self, vault_id: String) {
        self.authenticating = false;
        self.is_unlocked = true;
        self.vault_id = Some(vault_id);
        self.error = None;
    }

    /// Marks a failed unlock; records the sanitised error message.
    pub fn complete_failure(&mut self, message: String) {
        self.authenticating = false;
        self.is_unlocked = false;
        self.error = Some(message);
    }
}

/// Write-side handle to `SessionState` exposing intent-level operations.
#[derive(Clone, Copy)]
pub struct SessionActions {
    set_state: WriteSignal<SessionState>,
}

impl SessionActions {
    /// Delegates to [`SessionState::apply_status`] on the inner signal.
    pub fn apply_status(self, status: SessionStatus) {
        self.set_state.update(|s| s.apply_status(status));
    }

    /// Delegates to [`SessionState::begin_authenticating`] on the inner signal.
    pub fn begin_authenticating(self) {
        self.set_state.update(|s| s.begin_authenticating());
    }

    /// Delegates to [`SessionState::complete_success`] on the inner signal.
    pub fn complete_success(self, vault_id: String) {
        self.set_state.update(|s| s.complete_success(vault_id));
    }

    /// Delegates to [`SessionState::complete_failure`] on the inner signal.
    pub fn complete_failure(self, message: String) {
        self.set_state.update(|s| s.complete_failure(message));
    }

    /// Wipes all `SessionState` fields to defaults (Zero-Trace: called on vault lock).
    pub fn clear(self) {
        self.set_state.update(|s| *s = SessionState::default());
    }
}

/// Accessor for `SessionState`. Panics if no `SessionProvider` is mounted.
pub fn use_session() -> ReadSignal<SessionState> {
    use_context::<ReadSignal<SessionState>>().expect(
        "SessionProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Accessor for `SessionActions`. Panics if no `SessionProvider` is mounted.
pub fn use_session_actions() -> SessionActions {
    use_context::<SessionActions>().expect(
        "SessionProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Provides `ReadSignal<SessionState>` + `SessionActions` to descendants
/// and polls `get_session_status` every 5 seconds until unmount.
///
/// The stop flag is checked at three points in the loop:
///   1. At the top of the while condition.
///   2. After `invoke_command` resolves and before `set_state.update()`, preventing
///      a write to a disposed signal when `on_cleanup` fires during an in-flight IPC call.
///   3. After `set_state.update()` and before `TimeoutFuture` begins, preventing a
///      new sleep when `on_cleanup` fires after the IPC call returns. The post-sleep
///      guard is the while condition re-evaluation (point 1) once the timer resolves.
#[component]
pub fn SessionProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SessionState::default());
    provide_context(state);
    provide_context(SessionActions { set_state });

    let stop = Arc::new(AtomicBool::new(false));
    let stop_poll = Arc::clone(&stop);
    leptos::task::spawn_local(async move {
        while !stop_poll.load(Ordering::Relaxed) {
            if let Ok(status) = invoke_command::<(), SessionStatus>("get_session_status", &()).await
            {
                if stop_poll.load(Ordering::Relaxed) {
                    break;
                }
                set_state.update(|s| s.apply_status(status));
            }
            if stop_poll.load(Ordering::Relaxed) {
                break;
            }
            gloo_timers::future::TimeoutFuture::new(5_000).await;
        }
    });

    on_cleanup(move || stop.store(true, Ordering::Relaxed));

    children()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_apply_status_updates_three_fields_only() {
        let mut state = SessionState {
            authenticating: true,
            error: Some("err".to_string()),
            ..Default::default()
        };

        let status = SessionStatus {
            is_unlocked: true,
            vault_id: Some("vault-abc".to_string()),
            timeout_seconds: Some(300),
        };
        state.apply_status(status);

        assert!(state.is_unlocked);
        assert_eq!(state.vault_id, Some("vault-abc".to_string()));
        assert_eq!(state.timeout_seconds, Some(300));
        assert!(state.authenticating);
        assert_eq!(state.error, Some("err".to_string()));
    }

    #[test]
    fn test_session_state_begin_authenticating_sets_flag_and_clears_error() {
        let mut state = SessionState {
            error: Some("previous error".to_string()),
            authenticating: false,
            ..Default::default()
        };

        state.begin_authenticating();

        assert!(state.authenticating);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_session_state_complete_success_sets_unlocked_and_vault_id() {
        let mut state = SessionState {
            authenticating: true,
            ..Default::default()
        };

        state.complete_success("vault-xyz".to_string());

        assert!(!state.authenticating);
        assert!(state.is_unlocked);
        assert_eq!(state.vault_id, Some("vault-xyz".to_string()));
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_session_state_complete_failure_records_message_and_clears_authenticating() {
        let mut state = SessionState {
            vault_id: Some("vault-abc".to_string()),
            timeout_seconds: Some(600),
            authenticating: true,
            ..Default::default()
        };

        state.complete_failure("Authentication failed".to_string());

        assert!(!state.authenticating);
        assert!(!state.is_unlocked);
        assert_eq!(state.error, Some("Authentication failed".to_string()));
        assert_eq!(state.vault_id, Some("vault-abc".to_string()));
        assert_eq!(state.timeout_seconds, Some(600));
    }

    // --- Edge-case / boundary coverage ---

    /// Calling `complete_success` a second time with a different vault id must
    /// overwrite the previous vault id — the operation must be idempotent in the
    /// sense that no stale state from the first call leaks through.
    #[test]
    fn test_session_state_complete_success_called_twice_updates_vault_id() {
        let mut state = SessionState {
            authenticating: true,
            ..Default::default()
        };

        state.complete_success("vault-first".to_string());
        assert_eq!(state.vault_id, Some("vault-first".to_string()));
        assert!(state.is_unlocked);
        assert!(!state.authenticating);

        // Simulate a re-auth / vault switch without an intermediate lock.
        state.authenticating = true;
        state.complete_success("vault-second".to_string());

        assert_eq!(state.vault_id, Some("vault-second".to_string()));
        assert!(state.is_unlocked);
        assert!(!state.authenticating);
        assert_eq!(state.error, None);
    }

    /// When the background poller receives a locked status (`is_unlocked=false`,
    /// `vault_id=None`), `apply_status` must reflect that without touching the
    /// UI-owned `authenticating` and `error` fields.
    #[test]
    fn test_session_state_apply_status_lock_event_clears_is_unlocked_and_vault_id() {
        let mut state = SessionState {
            is_unlocked: true,
            vault_id: Some("vault-was-open".to_string()),
            timeout_seconds: Some(300),
            authenticating: false,
            error: None,
        };

        let lock_status = SessionStatus {
            is_unlocked: false,
            vault_id: None,
            timeout_seconds: None,
        };
        state.apply_status(lock_status);

        assert!(!state.is_unlocked);
        assert_eq!(state.vault_id, None);
        assert_eq!(state.timeout_seconds, None);
        // UI-owned fields are untouched.
        assert!(!state.authenticating);
        assert_eq!(state.error, None);
    }
}
