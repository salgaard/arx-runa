//! Chunk-size preset selector for vault creation.

use leptos::prelude::*;

/// Minimum chunk size in bytes (128 KiB).
pub const CHUNK_MIN: u64 = 131_072;

/// Maximum chunk size in bytes (64 MiB).
pub const CHUNK_MAX: u64 = 67_108_864;

/// Named chunk-size presets shown in the selector.
///
/// Ordered from most common to most specialised.
pub const PRESETS: &[(&str, u64)] = &[
    ("Standard (4 MiB)", 4_194_304),
    ("Documents (512 KiB)", 524_288),
    ("Media (16 MiB)", 16_777_216),
    ("Paranoid (64 MiB)", 67_108_864),
];

/// Clamps `bytes` to `[CHUNK_MIN, CHUNK_MAX]`.
///
/// Server-side validation is authoritative; this is best-effort client guard.
pub fn clamp_chunk_size(bytes: u64) -> u64 {
    bytes.clamp(CHUNK_MIN, CHUNK_MAX)
}

/// Chunk-size selector — radio group with four presets plus a custom bytes input.
///
/// Calls `set_value` with validated bytes whenever the selection changes.
/// The custom input only calls `set_value` once the entered number is in range.
#[component]
pub fn ChunkSizeSelector(
    /// Current chunk size in bytes.
    value: ReadSignal<u64>,
    /// Called with the new chunk size when the selection changes.
    set_value: WriteSignal<u64>,
) -> impl IntoView {
    let initial_is_custom = !PRESETS.iter().any(|(_, b)| *b == value.get_untracked());
    let (is_custom, set_is_custom) = signal(initial_is_custom);
    let (custom_input, set_custom_input) = signal(if initial_is_custom {
        value.get_untracked().to_string()
    } else {
        String::new()
    });
    let (custom_error, set_custom_error) = signal::<Option<String>>(None);

    let on_custom_change = move |v: String| {
        set_custom_input.set(v.clone());
        match v.parse::<u64>() {
            Ok(bytes) if (CHUNK_MIN..=CHUNK_MAX).contains(&bytes) => {
                set_custom_error.set(None);
                set_value.set(bytes);
            }
            Ok(_) => {
                set_custom_error.set(Some(format!(
                    "Must be between {CHUNK_MIN} and {CHUNK_MAX} bytes"
                )));
            }
            Err(_) if v.is_empty() => {
                set_custom_error.set(None);
            }
            Err(_) => {
                set_custom_error.set(Some("Enter a valid number".to_string()));
            }
        }
    };

    let render_preset = move |(label, bytes): (&'static str, u64)| {
        let is_selected = move || !is_custom.get() && value.get() == bytes;
        view! {
            <label
                class="flex items-center gap-3 p-2 border rounded-lg cursor-pointer hover:bg-surface-overlay transition-colors"
                class=("border-rune", is_selected)
                class=("bg-surface-overlay", is_selected)
                class=("border-transparent", move || !is_selected())
            >
                <input
                    type="radio"
                    name="chunk-size"
                    checked=is_selected
                    on:change=move |_| {
                        set_is_custom.set(false);
                        set_value.set(bytes);
                        set_custom_error.set(None);
                    }
                    class="cursor-pointer accent-rune"
                />
                <span class="text-sm text-bone">{label}</span>
            </label>
        }
    };

    view! {
        <div class="space-y-1">
            {PRESETS.iter().map(|&(label, bytes)| render_preset((label, bytes))).collect::<Vec<_>>()}

            <label
                class="flex items-center gap-3 p-2 border rounded-lg cursor-pointer hover:bg-surface-overlay transition-colors"
                class=("border-rune", move || is_custom.get())
                class=("bg-surface-overlay", move || is_custom.get())
                class=("border-transparent", move || !is_custom.get())
            >
                <input
                    type="radio"
                    name="chunk-size"
                    checked=move || is_custom.get()
                    on:change=move |_| {
                        set_is_custom.set(true);
                        set_custom_error.set(None);
                    }
                    class="cursor-pointer accent-rune"
                />
                <span class="text-sm text-bone">"Custom"</span>
            </label>

            <Show when=move || is_custom.get()>
                <div class="ml-7 mt-1 space-y-1">
                    <input
                        type="number"
                        placeholder=format!("{CHUNK_MIN} – {CHUNK_MAX} bytes")
                        value=move || custom_input.get()
                        on:input=move |ev| {
                            use leptos::prelude::event_target_value;
                            on_custom_change(event_target_value(&ev));
                        }
                        class="w-full bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-rune"
                    />
                    {move || custom_error.get().map(|e| view! {
                        <p class="text-danger text-xs">{e}</p>
                    })}
                </div>
            </Show>
        </div>
    }
}
