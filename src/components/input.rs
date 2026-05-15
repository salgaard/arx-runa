//! Input component — labelled text input field.

use leptos::prelude::*;

/// Labelled text input field.
#[component]
pub fn Input(
    /// HTML input type, e.g. `"text"` or `"password"`.
    #[prop(optional)]
    input_type: &'static str,
    /// Visible label text displayed above the input.
    #[prop(into)]
    label: String,
    /// Placeholder text shown when the input is empty.
    #[prop(optional)]
    placeholder: &'static str,
    /// Current input value signal.
    value: ReadSignal<String>,
    /// Callback invoked with the new value on each keystroke.
    on_input: impl Fn(String) + 'static + Clone,
    /// Optional `data-testid` value for e2e test selectors.
    #[prop(optional)]
    testid: Option<&'static str>,
) -> impl IntoView {
    let resolved_type = if input_type.is_empty() {
        "text"
    } else {
        input_type
    };

    view! {
        <div class="flex flex-col gap-1 mb-4">
            <label class="text-sm text-text-secondary">{label}</label>
            <input
                type=resolved_type
                class="bg-surface-overlay border border-border-default rounded-lg px-3 py-2 \
                       text-bone placeholder:text-text-ghost focus:outline-none \
                       focus:ring-2 focus:ring-rune"
                placeholder=placeholder
                prop:value=move || value.get()
                data-testid=testid
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    on_input(v);
                }
            />
        </div>
    }
}
