//! Sync state context: SyncState, SyncActions, SyncProvider, and accessor hooks.

use leptos::prelude::*;
use serde::Serialize;
use std::cell::Cell;
use std::rc::Rc;

use crate::components::use_toast;
use crate::events::on_tauri_event;
use crate::invoke::{invoke_command, invoke_command_with_channel};
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::{
    DestinationHealth, ReconcileResult, SyncProgressUpdate, SyncResult, SyncStatus,
};
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
    /// `true` when the user dismissed a sync conflict without pulling.
    pub stale_manifest: bool,
    /// Last user-displayable sync error, or `None` when clean.
    pub error: Option<String>,
    /// Per-destination backup failure counts; refreshed after each `sync_backup`.
    pub backup_health: Vec<DestinationHealth>,
    /// Overall sync progress 0–100 while in-flight; `None` when idle.
    pub sync_percent: Option<u8>,
}

impl SyncState {
    /// Wipes all fields to defaults. Called on lock to satisfy Zero-Trace.
    pub fn clear(&mut self) {
        self.syncing = false;
        self.last_synced_at = None;
        self.pending_changes = 0;
        self.conflict = None;
        self.stale_manifest = false;
        self.error = None;
        self.backup_health = Vec::new();
        self.sync_percent = None;
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
        self.set_state.update(|s| {
            s.syncing = true;
            // Show 5% immediately so the user sees activity before the first blob upload.
            s.sync_percent = Some(5);
        });

        leptos::task::spawn_local(async move {
            let channel = IpcChannel::<SyncProgressUpdate>::new();
            let set_state = self.set_state;
            // Primary phase: blob uploads mapped to 5–60%.
            channel.on_message(move |update: SyncProgressUpdate| {
                let mapped = (5 + update.percent as u16 * 55 / 100) as u8;
                set_state.update(|s| s.sync_percent = Some(mapped));
            });
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
                        // Primary done — anchor at 60% before backup begins.
                        s.sync_percent = Some(60);
                    });

                    let backup_channel = IpcChannel::<SyncProgressUpdate>::new();
                    // Backup phase: each destination emits 0-100% independently, which
                    // would cause the bar to jump backwards between destinations. Track
                    // a monotonic floor so the bar only ever advances.
                    let backup_floor = Rc::new(Cell::new(60u8));
                    backup_channel.on_message({
                        let backup_floor = backup_floor.clone();
                        move |update: SyncProgressUpdate| {
                            let raw = (60 + update.percent as u16 * 40 / 100) as u8;
                            let floor = backup_floor.get();
                            let mapped = raw.max(floor);
                            backup_floor.set(mapped);
                            set_state.update(|s| s.sync_percent = Some(mapped));
                        }
                    });
                    if let Err(e) = invoke_command_with_channel::<SyncBackupPayload, SyncResult>(
                        "sync_backup",
                        &SyncBackupPayload {
                            destination_id: None,
                        },
                        "progress",
                        backup_channel.inner(),
                    )
                    .await
                    {
                        use_toast().warning(format!("Backup failed: {e}"));
                    }

                    let health =
                        invoke_command::<(), Vec<DestinationHealth>>("get_backup_health", &())
                            .await
                            .unwrap_or_default();

                    self.set_state.update(|s| {
                        s.backup_health = health;
                        s.syncing = false;
                        s.sync_percent = None;
                    });
                    self.vault.refresh();
                }
                Err(e) => {
                    if e.kind == "syncConflict" {
                        self.set_state.update(|s| {
                            s.syncing = false;
                            s.sync_percent = None;
                            s.conflict = Some(
                                "Another device has synced. Pull changes and continue?".into(),
                            );
                        });
                    } else {
                        let msg = e.to_string();
                        use_toast().error(msg.clone());
                        self.set_state.update(|s| {
                            s.syncing = false;
                            s.sync_percent = None;
                            s.error = Some(msg);
                        });
                    }
                }
            }
        });
    }

    /// Dismisses the active sync conflict without pulling, marking the manifest stale.
    pub fn dismiss_conflict(self) {
        self.set_state.update(|s| {
            s.conflict = None;
            s.stale_manifest = true;
        });
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
                Ok(result) => {
                    if !result.conflicts_renamed.is_empty() {
                        let names = result.conflicts_renamed.join(", ");
                        use_toast().warning(format!(
                            "{} file(s) renamed as conflicted copies: {}",
                            result.conflicts_renamed.len(),
                            names
                        ));
                    }
                    self.set_state.update(|s| s.stale_manifest = false);
                    self.sync();
                }
                Err(e) => {
                    let msg = format!("Pull failed: {e}");
                    use_toast().error(msg.clone());
                    self.set_state.update(|s| {
                        s.syncing = false;
                        s.error = Some(msg);
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
/// Sync state is event-driven (no polling): the backend emits `sync-status-changed`
/// after any operation that alters pending changes (upload / delete / flush / sync).
/// On mount a single `get_sync_status` call seeds the initial `pending_changes`.
#[component]
pub fn SyncProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SyncState::default());
    provide_context(state);
    let vault = use_context::<VaultActions>()
        .expect("VaultProvider must wrap SyncProvider — check provider order in src/app.rs");
    provide_context(SyncActions { set_state, vault });

    // Initial sync: seed pending_changes once at mount.
    leptos::task::spawn_local(async move {
        if let Ok(status) = invoke_command::<(), SyncStatus>("get_sync_status", &()).await {
            set_state.update(|s| s.pending_changes = status.pending_changes);
        }
    });

    // Backend pushes `sync-status-changed` whenever pending changes move.
    on_tauri_event::<SyncStatus, _>("sync-status-changed", move |status| {
        set_state.update(|s| s.pending_changes = status.pending_changes);
    });

    children()
}
