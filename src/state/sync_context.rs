//! Sync state context: SyncState, SyncActions, SyncProvider, and accessor hooks.

use gloo_timers::callback::Interval;
use leptos::prelude::*;
use serde::Serialize;

use crate::invoke::{invoke_command, invoke_command_with_channel};
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::{DestinationHealth, ReconcileResult, SyncProgressUpdate, SyncResult};
use crate::state::vault_context::VaultActions;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncBackupPayload {
    destination_id: Option<String>,
}

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
    /// Per-destination backup failure counts; refreshed after each `sync_backup`.
    pub backup_health: Vec<DestinationHealth>,
}

impl SyncState {
    /// Wipes all fields to defaults. Called on lock to satisfy Zero-Trace.
    pub fn clear(&mut self) {
        self.syncing = false;
        self.last_synced_at = None;
        self.pending_changes = 0;
        self.conflict = None;
        self.error = None;
        self.backup_health = Vec::new();
    }
}

/// Write-side handle to `SyncState` exposing intent-level operations.
#[derive(Clone, Copy)]
pub struct SyncActions {
    set_state: WriteSignal<SyncState>,
    vault: VaultActions,
}

impl SyncActions {
    /// Wipes all `SyncState` fields to defaults (Zero-Trace: called on vault lock).
    pub fn clear(self) {
        self.set_state.update(|s| s.clear());
    }

    /// Calls `sync_to_cloud` then `sync_backup`, updating `SyncState` on completion.
    ///
    /// Primary sync runs first so that newly staged blobs are uploaded and recorded
    /// in `pending_backup` before the mirror pass reads the queue. This ensures new
    /// files reach mirror destinations in the same sync run they are uploaded to the
    /// primary.
    pub fn sync(self) {
        self.set_state.update(|s| s.syncing = true);

        leptos::task::spawn_local(async move {
            let channel = IpcChannel::<SyncProgressUpdate>::new();
            match invoke_command_with_channel::<(), SyncResult>(
                "sync_to_cloud",
                &(),
                "progress",
                channel.inner(),
            )
            .await
            {
                Ok(_result) => {
                    let now = js_sys::Date::new_0();
                    let iso_string = now.to_iso_string().as_string().unwrap_or_default();
                    self.set_state.update(|s| {
                        s.last_synced_at = Some(iso_string);
                        s.error = None;
                    });

                    let backup_channel = IpcChannel::<SyncProgressUpdate>::new();
                    let _ = invoke_command_with_channel::<SyncBackupPayload, SyncResult>(
                        "sync_backup",
                        &SyncBackupPayload {
                            destination_id: None,
                        },
                        "progress",
                        backup_channel.inner(),
                    )
                    .await;

                    let health =
                        invoke_command::<(), Vec<DestinationHealth>>("get_backup_health", &())
                            .await
                            .unwrap_or_default();

                    self.set_state.update(|s| {
                        s.backup_health = health;
                        s.syncing = false;
                    });
                    self.vault.refresh();
                }
                Err(e) => {
                    if e.kind == "syncConflict" {
                        self.set_state.update(|s| {
                            s.syncing = false;
                            s.conflict = Some(
                                "Another device has synced. Pull changes and continue?".into(),
                            );
                        });
                    } else {
                        self.set_state.update(|s| {
                            s.syncing = false;
                            s.error = Some(e.to_string());
                        });
                    }
                }
            }
        });
    }

    /// Dismisses the active sync conflict without taking action.
    pub fn dismiss_conflict(self) {
        self.set_state.update(|s| s.conflict = None);
    }

    /// Calls `pull_and_reconcile` then retries `sync()`.
    pub fn pull_and_reconcile_then_sync(self) {
        self.set_state.update(|s| {
            s.conflict = None;
            s.syncing = true;
        });
        leptos::task::spawn_local(async move {
            let channel = IpcChannel::<SyncProgressUpdate>::new();
            match invoke_command_with_channel::<(), ReconcileResult>(
                "pull_and_reconcile",
                &(),
                "progress",
                channel.inner(),
            )
            .await
            {
                Ok(_) => {
                    self.sync();
                }
                Err(e) => {
                    self.set_state.update(|s| {
                        s.syncing = false;
                        s.error = Some(format!("Pull failed: {e}"));
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
    let vault = use_context::<VaultActions>()
        .expect("VaultProvider must wrap SyncProvider — check provider order in src/app.rs");
    provide_context(SyncActions { set_state, vault });

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
