use leptos::prelude::*;

use super::Modal;

/// One-time warning modal shown once per app launch.
///
/// Owns a RAM-only signal initialised to `true`, so the warning re-appears on every
/// launch (intentional for a demo) without any persistence — Zero-Trace forbids
/// localStorage/sessionStorage. Dismissing only clears the in-memory flag.
#[component]
pub fn DemoWarningModal() -> impl IntoView {
    let open = RwSignal::new(true);
    let on_close = move || open.set(false);

    view! {
        <Modal open=Signal::derive(move || open.get()) on_close=on_close>
            <h2 class="text-lg font-semibold text-amber-300 mb-3" data-testid="demo-warning-modal">
                "\u{26A0} This is an early demo"
            </h2>
            <ul class="list-disc list-inside text-sm text-bone space-y-1 mb-5">
                <li>"The cryptography has " <strong>"not been independently audited"</strong> "."</li>
                <li>"It " <strong>"may corrupt or lose your files"</strong> " between versions."</li>
                <li>"Do " <strong>"not use it for confidential data"</strong> "."</li>
                <li>"Always keep an independent backup of anything you store here."</li>
            </ul>
            <div class="flex justify-end">
                <button
                    class="px-4 py-2 rounded-lg bg-steel text-bone hover:bg-rune cursor-pointer"
                    data-testid="demo-warning-dismiss"
                    on:click=move |_| open.set(false)
                >
                    "I understand"
                </button>
            </div>
        </Modal>
    }
}
