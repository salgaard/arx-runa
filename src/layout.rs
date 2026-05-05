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

    let nav_class = move |path: &str| -> String {
        let active = if location.pathname.get() == path {
            " bg-steel"
        } else {
            ""
        };
        format!(
            "px-3 py-1 text-sm text-bone rounded hover:bg-steel transition-colors cursor-pointer{}",
            active
        )
    };

    view! {
        <header class="flex items-center justify-between gap-3 px-6 py-4 bg-stone border-b border-steel">
            <div class="flex items-center gap-3">
                <svg viewBox="0 0 200 236" width="28" height="33" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
                    <path d="M 20 0 L 180 0 L 200 20 L 200 152 L 100 232 L 0 152 L 0 20 Z"
                          fill="#0C0E14" stroke="#5C7090" stroke-width="8"/>
                    <line x1="100" y1="30" x2="100" y2="188" stroke="#DBD7CD" stroke-width="10" stroke-linecap="square"/>
                    <line x1="100" y1="103" x2="59"  y2="148" stroke="#DBD7CD" stroke-width="10" stroke-linecap="square"/>
                    <line x1="100" y1="103" x2="141" y2="148" stroke="#DBD7CD" stroke-width="10" stroke-linecap="square"/>
                </svg>
                <span class="text-bone font-semibold">"Arx Runa"</span>
            </div>
            <div class="flex items-center gap-3">
                <A href="/">
                    <div class=move || nav_class("/")>"Vault"</div>
                </A>
                <A href="/destinations">
                    <div class=move || nav_class("/destinations")>"Destinations"</div>
                </A>
                <A href="/contacts">
                    <div class=move || nav_class("/contacts") title="Contacts">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M15.75 6a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0ZM4.501 20.118a7.5 7.5 0 0 1 14.998 0A17.933 17.933 0 0 1 12 21.75c-2.676 0-5.216-.584-7.499-1.632Z"/>
                        </svg>
                    </div>
                </A>
                <button
                    class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    on:click=on_sync
                    disabled=move || sync.read().syncing
                >
                    {move || if sync.read().syncing { "Syncing…" } else { "Sync" }}
                </button>
                <A href="/shares">
                    <div class=move || nav_class("/shares") title="Shares">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757m13.35-.622 1.757-1.757a4.5 4.5 0 0 0-6.364-6.364l-4.5 4.5a4.5 4.5 0 0 0 1.242 7.244"/>
                        </svg>
                    </div>
                </A>
                <A href="/settings">
                    <div class=move || nav_class("/settings") title="Settings">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z"/>
                            <path d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"/>
                        </svg>
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
            <button class="text-rune hover:text-bone cursor-pointer transition-colors" on:click=on_lock>
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
