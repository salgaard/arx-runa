//! Destination management page: list, add, and delete destinations.

use leptos::prelude::*;
use std::sync::Arc;

use crate::components::DestinationSelector;
use crate::invoke::invoke_command;
use crate::ipc_types::{
    AddDestinationRequest, DeleteDestinationRequest, DestinationEntry, DestinationSessionConfig,
};

// ─── DestinationItem ──────────────────────────────────────────────────────────

/// Single destination row with delete button and confirmation dialog.
#[component]
fn DestinationItem(
    entry: DestinationEntry,
    on_delete: Arc<dyn Fn() + Send + Sync>,
) -> impl IntoView {
    let show_confirm = RwSignal::new(false);
    let is_deleting = RwSignal::new(false);

    view! {
        <div class="flex items-center justify-between p-3 border border-steel rounded bg-iron hover:bg-surface-overlay transition-colors">
            <div class="flex-1">
                <p class="font-semibold text-bone">{entry.label.clone()}</p>
                <p class="text-sm text-text-secondary">
                    {format!("{} ({})", entry.destination_type, entry.provider)}
                </p>
            </div>
            <button
                class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                on:click=move |_| {
                    show_confirm.set(true);
                }
                disabled=move || is_deleting.get() || show_confirm.get()
            >
                "Delete"
            </button>

            {move || {
                if show_confirm.get() {
                    let destination_id = entry.destination_id.clone();
                    let on_delete_ref = on_delete.clone();
                    view! {
                        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                            <div class="bg-stone p-6 rounded border border-steel">
                                <p class="mb-4 text-bone">
                                    "Are you sure you want to delete this destination?"
                                </p>
                                <div class="flex gap-3">
                                    <button
                                        class="px-4 py-2 bg-steel text-bone rounded cursor-pointer hover:bg-rune/20 transition-colors"
                                        on:click=move |_| {
                                            show_confirm.set(false);
                                        }
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        class="px-4 py-2 bg-rune text-bone rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                                        on:click=move |_| {
                                            is_deleting.set(true);
                                            let dest_id = destination_id.clone();
                                            let on_del = on_delete_ref.clone();
                                            leptos::task::spawn_local(async move {
                                                match invoke_command::<DeleteDestinationRequest, ()>(
                                                    "delete_destination",
                                                    &DeleteDestinationRequest { destination_id: dest_id },
                                                )
                                                .await
                                                {
                                                    Ok(()) => {
                                                        is_deleting.set(false);
                                                        show_confirm.set(false);
                                                        on_del();
                                                    }
                                                    Err(_) => {
                                                        is_deleting.set(false);
                                                        // TODO: Show error message to user
                                                    }
                                                }
                                            });
                                        }
                                        disabled=move || is_deleting.get()
                                    >
                                        {move || if is_deleting.get() { "Deleting…" } else { "Delete" }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

// ─── AddDestinationForm ───────────────────────────────────────────────────────

/// Form to add a new backup destination.
#[component]
fn AddDestinationForm(on_added: Arc<dyn Fn() + Send + Sync>) -> impl IntoView {
    let label = RwSignal::new(String::new());
    let backup_mode = RwSignal::new("mirror".to_string());
    let is_adding = RwSignal::new(false);
    let validation_error = RwSignal::new(String::new());

    let config: RwSignal<DestinationSessionConfig> = RwSignal::new(DestinationSessionConfig {
        label: String::new(),
        destination_type: "local_path".to_string(),
        provider: "local".to_string(),
        bucket: String::new(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: String::new(),
        rclone_config_blob: String::new(),
        is_primary: false,
        backup_mode: None,
    });

    let field_class = "w-full px-3 py-2 bg-stone border border-steel rounded text-bone placeholder-text-secondary focus:outline-none focus:border-bone";

    view! {
        <div class="border border-steel rounded bg-iron p-4">
            <h3 class="text-lg font-semibold text-bone mb-3">"Add Destination"</h3>

            {move || {
                if !validation_error.get().is_empty() {
                    view! {
                        <div class="mb-3 p-2 bg-danger/20 text-danger text-sm rounded">
                            {validation_error.get()}
                        </div>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}

            <div class="space-y-4">
                <div>
                    <label class="block text-sm text-text-secondary mb-1">"Name"</label>
                    <input
                        type="text"
                        placeholder="My backup"
                        class=field_class
                        prop:value=move || label.get()
                        on:input=move |ev| {
                            label.set(event_target_value(&ev));
                            validation_error.set(String::new());
                        }
                        disabled=move || is_adding.get()
                    />
                </div>

                <div>
                    <label class="block text-sm text-text-secondary mb-1">"Backup Mode"</label>
                    <select
                        class=field_class
                        prop:value=move || backup_mode.get()
                        on:change=move |ev| {
                            backup_mode.set(event_target_value(&ev));
                        }
                        disabled=move || is_adding.get()
                    >
                        <option value="mirror">"Mirror — keep destination in sync with source"</option>
                        <option value="accumulating">"Accumulating — retain deleted files"</option>
                    </select>
                </div>

                <DestinationSelector on_change=move |c| config.set(c) />

                <button
                    class="w-full px-4 py-2 bg-rune text-bone rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                    on:click=move |_| {
                        let lbl = label.get();
                        if lbl.trim().is_empty() {
                            validation_error.set("Destination name is required".to_string());
                            return;
                        }

                        is_adding.set(true);
                        validation_error.set(String::new());

                        let mut final_config = config.get();
                        final_config.label = lbl.clone();
                        final_config.is_primary = false;
                        final_config.backup_mode = Some(backup_mode.get());

                        let req = AddDestinationRequest { config: final_config };
                        let on_added_ref = on_added.clone();
                        leptos::task::spawn_local(async move {
                            match invoke_command::<AddDestinationRequest, DestinationEntry>(
                                "add_destination",
                                &req,
                            )
                            .await
                            {
                                Ok(_entry) => {
                                    is_adding.set(false);
                                    label.set(String::new());
                                    crate::components::use_toast()
                                        .success(format!("Destination \"{}\" added.", lbl));
                                    on_added_ref();
                                }
                                Err(e) => {
                                    is_adding.set(false);
                                    crate::components::use_toast()
                                        .error(format!("Failed to add destination: {}", e));
                                }
                            }
                        });
                    }
                    disabled=move || is_adding.get()
                >
                    {move || if is_adding.get() { "Adding…" } else { "Add Destination" }}
                </button>
            </div>
        </div>
    }
}

// ─── DestinationList ──────────────────────────────────────────────────────────

/// List of configured destinations with add and delete controls.
#[component]
pub fn DestinationList() -> impl IntoView {
    let refresh_count = RwSignal::new(0u32);

    // LocalResource re-runs automatically when refresh_count changes because
    // it is accessed inside the synchronous closure that LocalResource tracks.
    let destinations = LocalResource::new(move || {
        let _trigger = refresh_count.get();
        async move { invoke_command::<(), Vec<DestinationEntry>>("list_destinations", &()).await }
    });

    let on_add: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        refresh_count.update(|n| *n += 1);
    });

    let on_delete: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        refresh_count.update(|n| *n += 1);
    });

    view! {
        <div class="space-y-6">
            <div>
                <h2 class="text-2xl font-bold text-bone mb-4">"Cloud Destinations"</h2>
                <p class="text-text-secondary mb-6">
                    "Manage the locations where your vault is backed up."
                </p>
            </div>

            <AddDestinationForm on_added=on_add.clone() />

            <div>
                <h3 class="text-lg font-semibold text-bone mb-3">"Configured Destinations"</h3>
                <Suspense fallback=move || {
                    view! { <p class="text-text-secondary">"Loading destinations…"</p> }
                }>
                    {move || {
                        destinations.get().map(|result| match result {
                            Ok(entries) if !entries.is_empty() => {
                                let on_delete_ref = on_delete.clone();
                                view! {
                                    <div class="space-y-2">
                                        {entries
                                            .into_iter()
                                            .map(|entry| {
                                                view! {
                                                    <DestinationItem
                                                        entry
                                                        on_delete=on_delete_ref.clone()
                                                    />
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                .into_any()
                            }
                            Ok(_) => view! { <p class="text-text-secondary">"No destinations configured yet."</p> }.into_any(),
                            Err(e) => view! { <p class="text-danger">{"Error loading destinations: "}{e.to_string()}</p> }.into_any(),
                        })
                    }}
                </Suspense>
            </div>
        </div>
    }
}
