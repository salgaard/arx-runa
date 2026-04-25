//! Spinner component — animated loading spinner.

use leptos::prelude::*;

/// Animated loading spinner.
#[component]
pub fn Spinner(
    /// Tailwind size class, e.g. `"h-4 w-4"` (default) or `"h-8 w-8"`.
    #[prop(optional)]
    size: &'static str,
) -> impl IntoView {
    let size_class = if size.is_empty() { "h-4 w-4" } else { size };
    view! {
        <span
            class=format!(
                "{size_class} inline-block rounded-full border-2 \
                 border-rune border-t-transparent animate-spin"
            )
            aria-label="Loading"
        />
    }
}
