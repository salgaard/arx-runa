//! Button component — generic action button with variant styling and loading state.

use super::spinner::Spinner;
use leptos::prelude::*;

/// Generic action button with variant styling and loading state.
///
/// When `loading` is `true`, the button shows a spinner and is disabled.
/// When `disabled` is `true`, the button is disabled without a spinner.
#[component]
pub fn Button(
    /// Visual variant: `"primary"` (default), `"secondary"`, or `"danger"`.
    #[prop(optional)]
    variant: &'static str,
    /// Whether the button is performing an async operation (shows spinner and disables).
    #[prop(into, optional)]
    loading: Signal<bool>,
    /// Whether the button is disabled without showing a spinner (e.g. form not complete).
    #[prop(into, optional)]
    disabled: Signal<bool>,
    /// Click event handler.
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static + Clone,
    /// Optional `data-testid` value for e2e test selectors.
    #[prop(optional)]
    testid: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let base = "inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg font-medium \
                cursor-pointer transition-colors focus:outline-none focus:ring-2 focus:ring-rune";
    let variant_class = move || match variant {
        "secondary" => "bg-steel text-bone hover:bg-rune",
        "danger" => "bg-danger text-iron hover:bg-danger/80",
        _ => "bg-rune text-bone hover:bg-rune/80",
    };
    let is_disabled = move || loading.get() || disabled.get();

    view! {
        <button
            class=move || format!(
                "{base} {} {}",
                variant_class(),
                if is_disabled() { "opacity-50 cursor-not-allowed" } else { "" }
            )
            disabled=is_disabled
            on:click=on_click
            data-testid=testid
        >
            <Show when=move || loading.get() fallback=|| ()>
                <Spinner />
            </Show>
            {children()}
        </button>
    }
}
