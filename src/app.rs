use leptos::prelude::*;

use crate::state::{SessionProvider, SyncProvider, VaultProvider};

/// Root frontend component rendered in the Tauri webview.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <SessionProvider>
            <VaultProvider>
                <SyncProvider>
                    <main class="container">
                        <h1>"Hello Arx Runa"</h1>
                    </main>
                </SyncProvider>
            </VaultProvider>
        </SessionProvider>
    }
}
