//! Epoch-buffer opt-in toggle for vault creation.

use leptos::prelude::*;

/// Epoch-buffer toggle — checkbox with inline explanation.
///
/// When enabled, files smaller than the chunk size are staged locally and packed
/// together before upload, reducing padding waste. Defaults to off.
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
                    "Pack small files to reduce storage overhead"
                </div>
                <div class="text-xs text-text-secondary mt-1 leading-relaxed">
                    "Small files are bundled together before upload so padding waste is shared \
                     across many files instead of inflating each one individually. \
                     Large files are always uploaded immediately."
                </div>
            </div>
        </label>
    }
}
