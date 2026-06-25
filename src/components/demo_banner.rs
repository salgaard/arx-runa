use leptos::prelude::*;

/// Always-on banner marking this as a pre-release demo build.
///
/// Unlike [`StaleManifestBanner`](super::StaleManifestBanner) it carries no state and
/// always renders — it is mounted above the router so it appears on every screen,
/// including the lock/vault-picker screen, before any file is touched. The version is
/// the compile-time crate version (inherited from the workspace).
#[component]
pub fn DemoBanner() -> impl IntoView {
    let version = env!("CARGO_PKG_VERSION");

    view! {
        <div class="sticky top-0 z-40 bg-amber-900/40 border-b border-amber-600 text-amber-200 text-sm px-4 py-2 text-center">
            "\u{26A0} DEMO BUILD v"{version}" \u{2014} unaudited \u{B7} may lose data \u{B7} not for confidential files"
        </div>
    }
}
