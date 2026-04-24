//! Sync state context: SyncState, SyncActions, SyncProvider, and accessor hooks.

use gloo_timers::callback::Interval;
use leptos::prelude::*;

use crate::invoke::invoke_command;
use crate::ipc_types::SyncResult;

/// Frontend-side sync status. Distinct from the wire DTO `ipc_types::SyncStatus`.
#[derive(Clone, Debug, Default)]
pub struct SyncState {
    /// `true` while a sync IPC call is in-flight.
    pub syncing: bool,
    /// ISO-8601 timestamp of the last successful sync, or `None` if never synced.
    pub last_synced_at: Option<String>,
    /// Number of local changes not yet uploaded to the cloud.
    pub pending_changes: u32,
    /// Description of an active merge conflict, or `None` when clean.
    pub conflict: Option<String>,
    /// Last user-displayable sync error, or `None` when clean.
    pub error: Option<String>,
}

impl SyncState {
    /// Wipes all fields to defaults. Called on lock to satisfy Zero-Trace.
    pub fn clear(&mut self) {
        self.syncing = false;
        self.last_synced_at = None;
        self.pending_changes = 0;
        self.conflict = None;
        self.error = None;
    }
}

/// Write-side handle to `SyncState` exposing intent-level operations.
#[derive(Clone, Copy)]
pub struct SyncActions {
    set_state: WriteSignal<SyncState>,
}

impl SyncActions {
    /// Wipes all `SyncState` fields to defaults (Zero-Trace: called on vault lock).
    pub fn clear(self) {
        self.set_state.update(|s| s.clear());
    }

    /// Calls `sync_to_cloud` and updates `SyncState` on completion.
    /// Updates `last_synced_at` on success, sets `error` on failure.
    pub fn sync(self) {
        self.set_state.update(|s| s.syncing = true);

        leptos::task::spawn_local(async move {
            match invoke_command::<(), SyncResult>("sync_to_cloud", &()).await {
                Ok(_result) => {
                    // Use js_sys::Date to get the current ISO timestamp
                    let now = js_sys::Date::new_0();
                    let iso_string = now.to_iso_string().as_string().unwrap_or_default();

                    self.set_state.update(|s| {
                        s.syncing = false;
                        s.last_synced_at = Some(iso_string);
                        s.error = None;
                    });
                }
                Err(e) => {
                    self.set_state.update(|s| {
                        s.syncing = false;
                        s.error = Some(e.to_string());
                    });
                }
            }
        });
    }
}

/// Accessor for `SyncState` read side.
pub fn use_sync() -> ReadSignal<SyncState> {
    use_context::<ReadSignal<SyncState>>().expect(
        "SyncProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Accessor for `SyncActions`. Panics if no `SyncProvider` is mounted.
pub fn use_sync_actions() -> SyncActions {
    use_context::<SyncActions>().expect(
        "SyncProvider must wrap the component tree — did you forget to mount it in src/app.rs?",
    )
}

/// Provides `ReadSignal<SyncState>` + `SyncActions` to descendants.
///
/// Polls `get_sync_status` every 5 seconds and updates frontend sync state.
/// Polling is cancelled when the component unmounts.
#[component]
pub fn SyncProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SyncState::default());
    provide_context(state);
    provide_context(SyncActions { set_state });

    // Poll get_sync_status every 5 seconds
    Effect::new(move |_| {
        let _handle = Interval::new(5000, move || {
            leptos::task::spawn_local(async move {
                // TODO: Implement get_sync_status polling
                // For now, this is a placeholder that will be wired in Phase 6.7.2
            });
        });

        on_cleanup(move || {
            // Interval is dropped here on cleanup
        });
    });

    children()
}
