//! Conflict-resolution dialog shown when `sync_to_cloud` returns a `SyncConflict` error.

use leptos::prelude::*;

use crate::state::{use_sync, use_sync_actions};

/// Renders a modal conflict dialog when `SyncState::conflict` is `Some`.
///
/// Confirm → `pull_and_reconcile_then_sync`; Cancel → `dismiss_conflict`.
#[component]
pub fn SyncConflictDialog() -> impl IntoView {
    let sync_state = use_sync();
    let actions = use_sync_actions();

    move || {
        let conflict_msg = sync_state.with(|s| s.conflict.clone());
        if let Some(msg) = conflict_msg {
            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
                    <div class="bg-iron border border-steel rounded-lg p-6 max-w-sm w-full space-y-4">
                        <h2 class="text-lg font-semibold text-bone">"Sync Conflict"</h2>
                        <p class="text-sm text-text-secondary">{msg}</p>
                        <div class="flex gap-3 justify-end">
                            <button
                                class="px-4 py-2 text-sm text-bone bg-steel rounded hover:bg-steel/80 transition-colors cursor-pointer"
                                on:click=move |_| actions.dismiss_conflict()
                            >
                                "Cancel"
                            </button>
                            <button
                                class="px-4 py-2 text-sm text-bone bg-rune rounded hover:bg-rune/80 transition-colors cursor-pointer"
                                on:click=move |_| actions.pull_and_reconcile_then_sync()
                            >
                                "Pull & Sync"
                            </button>
                        </div>
                    </div>
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        }
    }
}
