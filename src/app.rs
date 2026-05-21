use leptos::prelude::*;
use leptos_router::StaticSegment;
use leptos_router::components::{Route, Router, Routes};

use crate::auth::{LoginPage, RecoverWithPhrasePage, VaultCreationPage, VaultRecoveryPage};
use crate::components::{ToastProvider, inject_toast_styles};
use crate::contacts::ContactList;
use crate::destinations::DestinationList;
use crate::ipc_types::VaultSummary;
use crate::layout::AppShell;
use crate::settings::SettingsPage;
use crate::shares::SharesPage;
use crate::state::{
    SessionProvider, SyncProvider, VaultProvider, use_session, use_session_actions,
    use_sync_actions, use_vault_actions,
};
use crate::vault::VaultBrowser;
use crate::vault_picker::VaultPicker;

/// Root frontend component — mounts the provider hierarchy and the `Router`.
///
/// Provider order is canonical: `ToastProvider > SessionProvider > VaultProvider > SyncProvider`.
/// Every page renders inside the innermost provider to ensure all state hooks resolve.
#[component]
pub fn App() -> impl IntoView {
    inject_toast_styles();

    view! {
        <ToastProvider>
            <SessionProvider>
                <VaultProvider>
                    <SyncProvider>
                        <AppRouter />
                    </SyncProvider>
                </VaultProvider>
            </SessionProvider>
        </ToastProvider>
    }
}

/// Conditional router that maps session state to page views.
///
/// Three locked states: `VaultPicker` (home) → `LoginPage` (unlock selected vault) →
/// `VaultCreationPage` (create flow). Unlocked state: `AppShell` with inner Router.
///
/// An `Effect` observes `session.is_unlocked` and fans out `vault_actions.clear()` /
/// `sync_actions.clear()` / `session_actions.clear()` on the `true → false` transition —
/// Zero-Trace compliance hook.
#[component]
fn AppRouter() -> impl IntoView {
    let session = use_session();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();
    let session_actions = use_session_actions();

    // Which vault the user has selected but not yet unlocked.
    let selected_vault: RwSignal<Option<VaultSummary>> = RwSignal::new(None);
    // Whether the user clicked "Create vault".
    let create_vault_intent = RwSignal::new(false);
    // Whether the user clicked "Recover vault from cloud".
    let recover_intent = RwSignal::new(false);
    // Which vault the user wants to recover using their recovery phrase.
    let recover_with_phrase_intent: RwSignal<Option<VaultSummary>> = RwSignal::new(None);

    // Lock transition: clear vault, sync, and session state when session becomes inactive.
    Effect::new(move |prev: Option<bool>| {
        let now = session.read().is_unlocked;
        if prev == Some(true) && !now {
            vault_actions.clear();
            sync_actions.clear();
            session_actions.clear();
            // Return to VaultPicker after lock.
            selected_vault.set(None);
            create_vault_intent.set(false);
            recover_intent.set(false);
            recover_with_phrase_intent.set(None);
        }
        now
    });

    let is_unlocked = Memo::new(move |_| session.read().is_unlocked);

    move || {
        if is_unlocked.get() {
            view! {
                <Router>
                    <AppShell>
                        <Routes fallback=|| "404">
                            <Route path=StaticSegment("") view=VaultBrowser />
                            <Route path=StaticSegment("contacts") view=ContactList />
                            <Route path=StaticSegment("shares") view=SharesPage />
                            <Route path=StaticSegment("destinations") view=DestinationList />
                            <Route path=StaticSegment("settings") view=SettingsPage />
                        </Routes>
                    </AppShell>
                </Router>
            }
            .into_any()
        } else if create_vault_intent.get() {
            view! {
                <VaultCreationPage on_back_to_login=move || {
                    create_vault_intent.set(false);
                } />
            }
            .into_any()
        } else if recover_intent.get() {
            view! {
                <VaultRecoveryPage on_back=move || recover_intent.set(false) />
            }
            .into_any()
        } else if let Some(vault) = recover_with_phrase_intent.get() {
            view! {
                <RecoverWithPhrasePage
                    vault=vault
                    on_back=move || recover_with_phrase_intent.set(None)
                />
            }
            .into_any()
        } else if let Some(vault) = selected_vault.get() {
            let vault_for_recover = vault.clone();
            view! {
                <LoginPage
                    vault=vault
                    on_back=move || selected_vault.set(None)
                    on_recover=move || recover_with_phrase_intent.set(Some(vault_for_recover.clone()))
                />
            }
            .into_any()
        } else {
            view! {
                <VaultPicker
                    on_select=move |v| selected_vault.set(Some(v))
                    on_create=move || create_vault_intent.set(true)
                    on_recover=move || recover_intent.set(true)
                />
            }
            .into_any()
        }
    }
}
