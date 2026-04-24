//! Destination management page: list, add, and delete destinations.

use leptos::prelude::*;
use std::sync::Arc;

use crate::invoke::invoke_command;
use crate::ipc_types::{AddDestinationRequest, DeleteDestinationRequest, DestinationEntry};

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
        <div class="flex items-center justify-between p-3 border border-steel rounded bg-iron hover:bg-iron-light transition-colors">
            <div class="flex-1">
                <p class="font-semibold text-bone">{entry.label.clone()}</p>
                <p class="text-sm text-text-secondary">
                    {format!("{} ({})", entry.destination_type, entry.provider)}
                </p>
            </div>
            <button
                class="px-3 py-1 text-sm text-bone bg-rune rounded hover:bg-rune-dark transition-colors disabled:opacity-50"
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
                                        class="px-4 py-2 bg-steel text-bone rounded hover:bg-steel-light transition-colors"
                                        on:click=move |_| {
                                            show_confirm.set(false);
                                        }
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        class="px-4 py-2 bg-rune text-bone rounded hover:bg-rune-dark transition-colors disabled:opacity-50"
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

/// Form to add a new destination.
#[component]
fn AddDestinationForm(on_added: Arc<dyn Fn() + Send + Sync>) -> impl IntoView {
    let label = RwSignal::new(String::new());
    let destination_type = RwSignal::new("local".to_string());
    let path = RwSignal::new(String::new());
    let is_adding = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    view! {
        <div class="border border-steel rounded bg-iron p-4">
            <h3 class="text-lg font-semibold text-bone mb-3">"Add Destination"</h3>

            {move || {
                if !error.get().is_empty() {
                    view! {
                        <div class="mb-3 p-2 bg-rune text-bone text-sm rounded">
                            {error.get()}
                        </div>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}

            <div class="space-y-3">
                <div>
                    <label class="block text-sm text-text-secondary mb-1">"Name"</label>
                    <input
                        type="text"
                        placeholder="My backup"
                        class="w-full px-3 py-2 bg-stone border border-steel rounded text-bone placeholder-text-secondary focus:outline-none focus:border-bone"
                        prop:value=move || label.get()
                        on:input=move |ev| {
                            label.set(event_target_value(&ev));
                        }
                        disabled=move || is_adding.get()
                    />
                </div>

                <div>
                    <label class="block text-sm text-text-secondary mb-1">"Type"</label>
                    <select
                        class="w-full px-3 py-2 bg-stone border border-steel rounded text-bone focus:outline-none focus:border-bone"
                        prop:value=move || destination_type.get()
                        on:change=move |ev| {
                            destination_type.set(event_target_value(&ev));
                        }
                        disabled=move || is_adding.get()
                    >
                        <option value="local">"Local Path"</option>
                        <option value="rclone">"Rclone Remote"</option>
                    </select>
                </div>

                <div>
                    <label class="block text-sm text-text-secondary mb-1">
                        {move || {
                            if destination_type.get() == "local" {
                                "Path"
                            } else {
                                "Remote"
                            }
                        }}
                    </label>
                    <input
                        type="text"
                        placeholder=move || {
                            if destination_type.get() == "local" {
                                "/path/to/backup"
                            } else {
                                "s3:my-bucket/backups"
                            }
                        }
                        class="w-full px-3 py-2 bg-stone border border-steel rounded text-bone placeholder-text-secondary focus:outline-none focus:border-bone"
                        prop:value=move || path.get()
                        on:input=move |ev| {
                            path.set(event_target_value(&ev));
                        }
                        disabled=move || is_adding.get()
                    />
                </div>

                <button
                    class="w-full px-4 py-2 bg-rune text-bone rounded hover:bg-rune-dark transition-colors disabled:opacity-50"
                    on:click=move |_| {
                        if label.get().trim().is_empty() {
                            error.set("Destination name is required".to_string());
                            return;
                        }
                        if path.get().trim().is_empty() {
                            error.set("Path/remote is required".to_string());
                            return;
                        }

                        is_adding.set(true);
                        error.set(String::new());

                        let req = AddDestinationRequest {
                            label: label.get(),
                            destination_type: destination_type.get(),
                            path_or_remote: path.get(),
                        };

                        let on_added_ref = on_added.clone();
                        leptos::task::spawn_local(async move {
                            match invoke_command::<AddDestinationRequest, ()>("add_destination", &req).await {
                                Ok(()) => {
                                    is_adding.set(false);
                                    label.set(String::new());
                                    path.set(String::new());
                                    on_added_ref();
                                }
                                Err(e) => {
                                    is_adding.set(false);
                                    error.set(format!("Failed to add destination: {}", e));
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
    let destinations = LocalResource::new(|| async move {
        invoke_command::<(), Vec<DestinationEntry>>("list_destinations", &()).await
    });

    let refresh_count = RwSignal::new(0);

    let on_add: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        refresh_count.set(refresh_count.get() + 1);
    });

    let on_delete: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        refresh_count.set(refresh_count.get() + 1);
    });

    // Re-fetch when refresh_count changes
    Effect::new(move |_| {
        let _ = refresh_count.get();
        destinations.refetch();
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
                            Err(e) => view! { <p class="text-rune">{"Error loading destinations: "}{e.to_string()}</p> }.into_any(),
                        })
                    }}
                </Suspense>
            </div>
        </div>
    }
}
