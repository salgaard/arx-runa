use leptos::prelude::*;

use crate::state::{use_sync, use_sync_actions};

/// Persistent banner shown when the user dismissed a sync conflict without pulling.
/// Disappears once the user pulls the latest manifest or locks the vault.
#[component]
pub fn StaleManifestBanner() -> impl IntoView {
    let sync_state = use_sync();
    let actions = use_sync_actions();

    move || {
        if sync_state.with(|s| s.stale_manifest) {
            view! {
                <div class="bg-amber-900/40 border-b border-amber-600 text-amber-200 text-sm px-4 py-2 flex items-center justify-between">
                    <span>"Working with stale manifest — conflicts possible"</span>
                    <button
                        class="ml-4 underline text-amber-300 hover:text-amber-100 cursor-pointer"
                        on:click=move |_| actions.pull_and_reconcile_then_sync()
                    >
                        "Pull latest"
                    </button>
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        }
    }
}
