//! Shared UI primitives — leaf module, no project-internal dependencies.
//!
//! All components in this file are stateless primitives; business logic lives
//! in the `auth`, `vault`, `transfer`, and `layout` modules.

use leptos::prelude::*;

// ─── Button ──────────────────────────────────────────────────────────────────

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
    children: Children,
) -> impl IntoView {
    let base = "inline-flex items-center justify-center px-4 py-2 rounded-lg font-medium \
                transition-colors focus:outline-none focus:ring-2 focus:ring-rune";
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
        >
            {children()}
        </button>
    }
}

// ─── Input ───────────────────────────────────────────────────────────────────

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
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    on_input(v);
                }
            />
        </div>
    }
}

// ─── Modal ───────────────────────────────────────────────────────────────────

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

// ─── Spinner ─────────────────────────────────────────────────────────────────

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
