//! Modal component — overlay modal dialog.

use leptos::prelude::*;

/// Overlay modal dialog.
///
/// Renders nothing when `open` is `false`. Pressing Escape calls `on_close`.
#[component]
pub fn Modal(
    /// Whether the modal is visible.
    #[prop(into)]
    open: Signal<bool>,
    /// Called when the user requests the modal to close.
    on_close: impl Fn() + 'static + Clone + Send + Sync,
    children: ChildrenFn,
) -> impl IntoView {
    let on_close_stored = StoredValue::new(on_close);

    view! {
        <Show when=move || open.get()>
            <div
                class="fixed inset-0 z-50 flex items-center justify-center bg-iron/80"
                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                    if ev.key() == "Escape" {
                        on_close_stored.with_value(|f| f());
                    }
                }
            >
                <div class="bg-stone border border-steel rounded-xl p-6 w-full max-w-lg shadow-2xl">
                    {children()}
                </div>
            </div>
        </Show>
    }
}
