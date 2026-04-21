use leptos::prelude::*;

use crate::auth::{LoginPage, VaultCreationPage};
use crate::layout::AppShell;
use crate::state::{
    SessionProvider, SyncProvider, VaultProvider, use_session, use_sync_actions, use_vault_actions,
};
use crate::vault::VaultBrowser;

/// Root frontend component — mounts the provider hierarchy and the `Router`.
///
/// Provider order is canonical: `SessionProvider > VaultProvider > SyncProvider`.
/// Every page renders inside the innermost provider to ensure all state hooks
/// resolve.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <SessionProvider>
            <VaultProvider>
                <SyncProvider>
                    <Router />
                </SyncProvider>
            </VaultProvider>
        </SessionProvider>
    }
}

/// Conditional router that maps session state to page views.
///
/// An `Effect` observes `session.is_unlocked` and fans out `vault_actions.clear()` /
/// `sync_actions.clear()` on the `true → false` transition — this is the canonical
/// locked-transition hook for Zero-Trace compliance. Pages must not duplicate this
/// logic.
#[component]
fn Router() -> impl IntoView {
    let session = use_session();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();
    let create_vault_intent = RwSignal::new(false);

    Effect::new(move |prev: Option<bool>| {
        let now = session.read().is_unlocked;
        if prev == Some(true) && !now {
            vault_actions.clear();
            sync_actions.clear();
        }
        now
    });

    move || {
        let is_unlocked = session.read().is_unlocked;
        if is_unlocked {
            view! { <AppShell><VaultBrowser /></AppShell> }.into_any()
        } else if create_vault_intent.get() {
            view! {
                <VaultCreationPage on_back_to_login=move || create_vault_intent.set(false) />
            }
            .into_any()
        } else {
            view! {
                <LoginPage on_request_create_vault=move || create_vault_intent.set(true) />
            }
            .into_any()
        }
    }
}
