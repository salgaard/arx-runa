//! Application shell layout components: header bar, session status footer,
//! and the `AppShell` wrapper that composes them around page content.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use crate::invoke::invoke_command;
use crate::state::{
    use_session, use_session_actions, use_sync, use_sync_actions, use_vault_actions,
};
use crate::utils::format_relative_time;

// ─── Header ───────────────────────────────────────────────────────────────────

/// Application header bar showing the Arx Runa logo, title, sync button, and navigation.
#[component]
pub fn Header() -> impl IntoView {
    let sync = use_sync();
    let sync_actions = use_sync_actions();
    let location = use_location();

    let on_sync = move |_| {
        sync_actions.sync();
    };

    let _is_destinations = move || location.pathname.get() == "/destinations";

    view! {
        <header class="flex items-center justify-between gap-3 px-6 py-4 bg-stone border-b border-steel">
            <div class="flex items-center gap-3">
                <span class="text-rune text-xl font-bold">"⬡"</span>
                <span class="text-bone font-semibold">"Arx Runa"</span>
            </div>
            <div class="flex items-center gap-3">
                <div class="px-3 py-1 text-sm text-bone rounded hover:bg-steel transition-colors">
                    <A href="/">
                        "Vault"
                    </A>
                </div>
                <div class="px-3 py-1 text-sm text-bone rounded hover:bg-steel transition-colors">
                    <A href="/destinations">
                        "Destinations"
                    </A>
                </div>
                <A href="/contacts">
                    <div class="px-3 py-1 text-bone rounded hover:bg-steel transition-colors" title="Contacts">
                        "👤"
                    </div>
                </A>
                <button
                    class="px-3 py-1 text-sm text-bone bg-rune rounded hover:bg-rune-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    on:click=on_sync
                    disabled=move || sync.read().syncing
                >
                    {move || if sync.read().syncing { "Syncing…" } else { "Sync" }}
                </button>
                <A href="/shares">
                    <div class="px-3 py-1 text-bone rounded hover:bg-steel transition-colors" title="Shares">
                        "🔗"
                    </div>
                </A>
                <A href="/settings">
                    <div class="px-3 py-1 text-bone rounded hover:bg-steel transition-colors" title="Settings">
                        "⚙"
                    </div>
                </A>
            </div>
        </header>
    }
}

// ─── format_countdown_seconds ────────────────────────────────────────────────

/// Formats a remaining-seconds countdown into `MM:SS` or `HH:MM:SS`.
pub fn format_countdown_seconds(remaining: u64) -> String {
    let h = remaining / 3600;
    let m = (remaining % 3600) / 60;
    let s = remaining % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

// ─── SessionStatusBar ────────────────────────────────────────────────────────

/// Footer status bar showing the session countdown, last-synced timestamp, and a lock button.
///
/// The lock button clears Vault, Sync, and Session state before invoking
/// `lock_session` IPC — satisfying the Zero-Trace requirement that all
/// sensitive state is wiped regardless of IPC success or failure.
#[component]
pub fn SessionStatusBar() -> impl IntoView {
    let session = use_session();
    let sync = use_sync();
    let session_actions = use_session_actions();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();

    let on_lock = move |_| {
        vault_actions.clear();
        sync_actions.clear();
        session_actions.clear();
        leptos::task::spawn_local(async move {
            let _ = invoke_command::<(), ()>("lock_session", &()).await;
        });
    };

    view! {
        <footer class="flex justify-between items-center p-4 bg-stone border-t border-steel text-sm">
            <div class="flex items-center gap-4">
                <span class="text-text-secondary font-mono">
                    {move || session.read().timeout_seconds
                        .map(format_countdown_seconds)
                        .unwrap_or_default()}
                </span>
                <span class="text-text-secondary">
                    {move || {
                        let state = sync.read();
                        match &state.last_synced_at {
                            Some(ts) => format!("Last synced: {}", format_relative_time(ts)),
                            None => "Never synced".to_string(),
                        }
                    }}
                </span>
            </div>
            <button class="text-rune hover:text-bone transition-colors" on:click=on_lock>
                "Lock"
            </button>
        </footer>
    }
}

// ─── AppShell ────────────────────────────────────────────────────────────────

/// Application shell wrapping page content with a header and session status bar.
#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen bg-iron text-bone flex flex-col">
            <Header />
            <main class="flex-1 p-6 overflow-auto">{children()}</main>
            <SessionStatusBar />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_countdown_seconds_zero_returns_zero_zero() {
        assert_eq!(format_countdown_seconds(0), "00:00");
    }

    #[test]
    fn test_format_countdown_seconds_sub_minute_returns_mm_ss() {
        assert_eq!(format_countdown_seconds(45), "00:45");
        assert_eq!(format_countdown_seconds(59), "00:59");
    }

    #[test]
    fn test_format_countdown_seconds_over_hour_returns_hhmmss() {
        assert_eq!(format_countdown_seconds(3600), "01:00:00");
        assert_eq!(format_countdown_seconds(3661), "01:01:01");
        assert_eq!(format_countdown_seconds(7322), "02:02:02");
    }
}
