//! Epoch-buffer opt-in toggle for vault creation.

use leptos::prelude::*;

/// Epoch-buffer toggle — checkbox with inline explanation.
///
/// The epoch buffer preserves old encrypted file versions during key rotation.
/// Defaulting to off keeps storage costs predictable for most users.
#[component]
pub fn EpochBufferToggle(
    /// Whether the epoch buffer is currently enabled.
    enabled: ReadSignal<bool>,
    /// Called with the new value when the checkbox is toggled.
    set_enabled: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <label class="flex items-start gap-3 cursor-pointer group">
            <input
                type="checkbox"
                checked=move || enabled.get()
                on:change=move |ev| {
                    use leptos::prelude::event_target_checked;
                    set_enabled.set(event_target_checked(&ev));
                }
                class="mt-0.5 cursor-pointer accent-rune"
            />
            <div>
                <div class="text-sm text-bone font-medium group-hover:text-rune transition-colors">
                    "Keep version history during re-encryption"
                </div>
                <div class="text-xs text-text-secondary mt-1 leading-relaxed">
                    "When you rotate encryption keys, old file versions are preserved before \
                     re-encryption. Requires additional storage space."
                </div>
            </div>
        </label>
    }
}
