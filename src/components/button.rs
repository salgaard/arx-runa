//! Button component — generic action button with variant styling and loading state.

use leptos::prelude::*;

/// Generic action button with variant styling and loading state.
///
/// When `loading` is `true`, the button is disabled and shows a spinner
/// placeholder.
#[component]
pub fn Button(
    /// Visual variant: `"primary"` (default), `"secondary"`, or `"danger"`.
    #[prop(optional)]
    variant: &'static str,
    /// Whether the button is in a loading/disabled state.
    #[prop(into, optional)]
    loading: Signal<bool>,
    /// Click event handler.
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static + Clone,
    /// Optional `data-testid` value for e2e test selectors.
    #[prop(optional)]
    testid: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let base = "inline-flex items-center justify-center px-4 py-2 rounded-lg font-medium \
                cursor-pointer transition-colors focus:outline-none focus:ring-2 focus:ring-rune";
    let variant_class = move || match variant {
        "secondary" => "bg-steel text-bone hover:bg-rune",
        "danger" => "bg-danger text-iron hover:bg-danger/80",
        _ => "bg-rune text-bone hover:bg-rune/80",
    };
    let disabled = move || loading.get();

    view! {
        <button
            class=move || format!(
                "{base} {} {}",
                variant_class(),
                if disabled() { "opacity-50 cursor-not-allowed" } else { "" }
            )
            disabled=disabled
            on:click=on_click
            data-testid=testid
        >
            {children()}
        </button>
    }
}
