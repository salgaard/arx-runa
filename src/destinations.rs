//! Destination management page: list, add, and delete destinations.

use leptos::prelude::*;
use std::sync::Arc;

use crate::components::DestinationSelector;
use crate::dialog::open_file_dialog;
use crate::invoke::invoke_command;
use crate::ipc_types::{
    AddDestinationRequest, DeleteDestinationRequest, DestinationEntry, DestinationHealth,
    DestinationSessionConfig, OpenUrlRequest, SetGdriveServiceAccountRequest,
    SetPrimaryDestinationRequest,
};
use crate::state::use_sync;

// ─── GdriveShareSetupModal ────────────────────────────────────────────────────

/// Step-by-step modal for setting up a Google Drive Service Account for sharing.
///
/// Guides the user through creating a GCP Service Account and selecting the
/// downloaded JSON key file.  Calls `on_file_picked` with the selected path then
/// closes itself via `on_close`.  The caller is responsible for invoking the
/// backend and handling errors.
#[component]
pub(crate) fn GdriveShareSetupModal(
    on_file_picked: impl Fn(String) + Clone + Send + Sync + 'static,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let is_picking = RwSignal::new(false);

    let on_file_picked = Arc::new(on_file_picked);
    let on_close = Arc::new(on_close);
    let on_close_btn = on_close.clone();
    let on_close_cancel = on_close.clone();

    let pick_and_notify = {
        let on_file_picked = on_file_picked.clone();
        let on_close = on_close.clone();
        move |_| {
            is_picking.set(true);
            let on_file_picked = on_file_picked.clone();
            let on_close = on_close.clone();
            leptos::task::spawn_local(async move {
                let path = open_file_dialog().await;
                is_picking.set(false);
                let Some(path) = path else {
                    return;
                };
                on_file_picked(path);
                on_close();
            });
        }
    };

    view! {
        <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
            <div class="bg-stone border border-steel rounded-lg w-full max-w-lg shadow-xl">
                // Header
                <div class="flex items-center justify-between px-6 pt-5 pb-3 border-b border-steel">
                    <h3 class="text-lg font-semibold text-bone">"Set up Google Drive Sharing"</h3>
                    <button
                        class="text-text-secondary hover:text-bone transition-colors text-xl leading-none"
                        on:click=move |_| on_close_btn()
                    >
                        "×"
                    </button>
                </div>

                // Body
                <div class="px-6 py-4 space-y-4 text-sm">
                    <p class="text-text-secondary">
                        "Sharing requires a GCP Service Account so recipients can download "
                        "encrypted blobs directly from your Drive. The key is stored encrypted "
                        "in your vault and never leaves this device."
                    </p>

                    // Steps
                    <ol class="space-y-3">
                        <li class="flex gap-3">
                            <span class="flex-shrink-0 w-6 h-6 rounded-full bg-rune text-bone text-xs font-bold flex items-center justify-center mt-0.5">"1"</span>
                            <div>
                                <p class="text-bone font-medium">"Open Google Cloud Console"</p>
                                <p class="text-text-secondary mt-0.5">
                                    "Go to "
                                    <button
                                        class="text-rune hover:underline cursor-pointer"
                                        on:click=move |_| {
                                            leptos::task::spawn_local(async {
                                                let _ = invoke_command::<OpenUrlRequest, ()>(
                                                    "open_url",
                                                    &OpenUrlRequest {
                                                        url: "https://console.cloud.google.com"
                                                            .into(),
                                                    },
                                                )
                                                .await;
                                            });
                                        }
                                    >
                                        "console.cloud.google.com ↗"
                                    </button>
                                    " and select or create a project."
                                </p>
                            </div>
                        </li>
                        <li class="flex gap-3">
                            <span class="flex-shrink-0 w-6 h-6 rounded-full bg-rune text-bone text-xs font-bold flex items-center justify-center mt-0.5">"2"</span>
                            <div>
                                <p class="text-bone font-medium">"Enable the Google Drive API"</p>
                                <p class="text-text-secondary mt-0.5">
                                    "In the left menu go to "
                                    <span class="font-mono text-bone bg-iron px-1 rounded">"APIs & Services → Library"</span>
                                    ", search for \"Google Drive API\", and enable it."
                                </p>
                            </div>
                        </li>
                        <li class="flex gap-3">
                            <span class="flex-shrink-0 w-6 h-6 rounded-full bg-rune text-bone text-xs font-bold flex items-center justify-center mt-0.5">"3"</span>
                            <div>
                                <p class="text-bone font-medium">"Create a Service Account"</p>
                                <p class="text-text-secondary mt-0.5">
                                    "Go to "
                                    <button
                                        class="text-rune hover:underline cursor-pointer"
                                        on:click=move |_| {
                                            leptos::task::spawn_local(async {
                                                let _ = invoke_command::<OpenUrlRequest, ()>(
                                                    "open_url",
                                                    &OpenUrlRequest {
                                                        url: "https://console.cloud.google.com/iam-admin/serviceaccounts".into(),
                                                    },
                                                )
                                                .await;
                                            });
                                        }
                                    >
                                        "IAM → Service Accounts ↗"
                                    </button>
                                    " → Create Service Account. Give it any name — no roles needed."
                                </p>
                            </div>
                        </li>
                        <li class="flex gap-3">
                            <span class="flex-shrink-0 w-6 h-6 rounded-full bg-rune text-bone text-xs font-bold flex items-center justify-center mt-0.5">"4"</span>
                            <div>
                                <p class="text-bone font-medium">"Download the JSON key"</p>
                                <p class="text-text-secondary mt-0.5">
                                    "Open the service account, go to the "
                                    <span class="font-mono text-bone bg-iron px-1 rounded">"Keys"</span>
                                    " tab → "
                                    <span class="font-mono text-bone bg-iron px-1 rounded">"Add Key → Create new key → JSON"</span>
                                    ". A "
                                    <span class="font-mono text-bone bg-iron px-1 rounded">".json"</span>
                                    " file will be downloaded to your computer."
                                </p>
                            </div>
                        </li>
                        <li class="flex gap-3">
                            <span class="flex-shrink-0 w-6 h-6 rounded-full bg-rune text-bone text-xs font-bold flex items-center justify-center mt-0.5">"5"</span>
                            <div>
                                <p class="text-bone font-medium">"Select the key file below"</p>
                                <p class="text-text-secondary mt-0.5">
                                    "Click the button below and select the downloaded "
                                    <span class="font-mono text-bone bg-iron px-1 rounded">".json"</span>
                                    " file."
                                </p>
                            </div>
                        </li>
                    </ol>

                </div>

                // Footer
                <div class="flex items-center justify-between px-6 pb-5 pt-2 gap-3">
                    <button
                        class="text-xs text-text-secondary hover:text-bone transition-colors cursor-pointer"
                        on:click=move |_| {
                            leptos::task::spawn_local(async {
                                let _ = invoke_command::<OpenUrlRequest, ()>(
                                    "open_url",
                                    &OpenUrlRequest {
                                        url: "https://developers.google.com/workspace/guides/create-credentials#service-account".into(),
                                    },
                                )
                                .await;
                            });
                        }
                    >
                        "Google docs ↗"
                    </button>
                    <div class="flex gap-3">
                        <button
                            class="px-4 py-2 text-sm bg-steel text-bone rounded cursor-pointer hover:bg-steel/80 transition-colors"
                            on:click=move |_| on_close_cancel()
                        >
                            "Cancel"
                        </button>
                        <button
                            class="px-4 py-2 text-sm bg-rune text-bone rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                            on:click=pick_and_notify
                            disabled=move || is_picking.get()
                        >
                            {move || if is_picking.get() { "Selecting…" } else { "Select JSON Key File" }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

// ─── GdriveShareStatus ────────────────────────────────────────────────────────

/// Sharing status row shown inside a Google Drive `DestinationItem`.
///
/// Loads `has_gdrive_service_account` and either shows a green "Sharing enabled"
/// badge or a yellow "Sharing not configured" badge with a "Configure" button
/// that opens `GdriveShareSetupModal`.
#[component]
fn GdriveShareStatus() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let show_modal = RwSignal::new(false);

    let has_sa = LocalResource::new(move || {
        let _t = refresh.get();
        async move { invoke_command::<(), bool>("has_gdrive_service_account", &()).await }
    });

    view! {
        <div class="mt-2 pt-2 border-t border-steel/40">
            <Suspense fallback=move || {
                view! { <span class="text-xs text-text-secondary">"Checking sharing…"</span> }
            }>
                {move || {
                    has_sa.get().map(|result| {
                        let configured = result.unwrap_or(false);
                        if configured {
                            view! {
                                <div class="flex items-center gap-2">
                                    <span class="text-xs text-success">"● Sharing enabled"</span>
                                    <button
                                        class="text-xs text-text-secondary hover:text-bone underline"
                                        on:click=move |_| show_modal.set(true)
                                    >
                                        "Replace key"
                                    </button>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="flex items-center gap-2">
                                    <span class="text-xs text-warning">"● Sharing not configured"</span>
                                    <button
                                        class="text-xs text-rune hover:text-rune/80 underline cursor-pointer"
                                        on:click=move |_| show_modal.set(true)
                                    >
                                        "Configure →"
                                    </button>
                                </div>
                            }
                            .into_any()
                        }
                    })
                }}
            </Suspense>

            {move || {
                if show_modal.get() {
                    let on_file_picked = move |path: String| {
                        leptos::task::spawn_local(async move {
                            match invoke_command::<_, ()>(
                                "set_gdrive_service_account",
                                &SetGdriveServiceAccountRequest { sa_json_path: path },
                            )
                            .await
                            {
                                Ok(()) => {
                                    refresh.update(|n| *n += 1);
                                    crate::components::use_toast()
                                        .success("Google Drive sharing enabled.".to_string());
                                }
                                Err(e) => {
                                    crate::components::use_toast()
                                        .error(format!("Could not save key: {e}"));
                                }
                            }
                        });
                    };
                    let on_close = move || show_modal.set(false);
                    view! { <GdriveShareSetupModal on_file_picked=on_file_picked on_close=on_close /> }.into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

// ─── DestinationItem ──────────────────────────────────────────────────────────

/// Single destination row showing role badge, backup mode, and action buttons.
///
/// Primary destinations show a badge and have Delete hidden (must promote another
/// destination first). Non-primary destinations show a "Set as Primary" button.
/// Google Drive destinations also show a sharing-status sub-row.
#[component]
fn DestinationItem(
    entry: DestinationEntry,
    on_refresh: Arc<dyn Fn() + Send + Sync>,
    #[prop(default = 0u32)] pending_failures: u32,
) -> impl IntoView {
    let is_primary = entry.is_primary;
    let is_gdrive = entry.rclone_type.as_deref() == Some("drive");
    let backup_mode_label = match entry.backup_mode.as_deref() {
        Some("mirror") => "Mirror",
        Some("accumulating") => "Accumulating",
        _ => "",
    };

    let show_confirm = RwSignal::new(false);
    let is_deleting = RwSignal::new(false);
    let is_promoting = RwSignal::new(false);

    view! {
        <div class="p-3 border border-steel rounded bg-iron hover:bg-surface-overlay transition-colors">
            <div class="flex items-center justify-between">
                <div class="flex-1">
                    <div class="flex items-center gap-2">
                        <p class="font-semibold text-bone">{entry.label.clone()}</p>
                        {if is_primary {
                            view! {
                                <span class="px-2 py-0.5 text-xs font-medium bg-rune text-bone rounded">
                                    "Primary"
                                </span>
                            }
                            .into_any()
                        } else {
                            ().into_any()
                        }}
                        {if pending_failures > 0 {
                            view! {
                                <span class="px-2 py-0.5 text-xs font-medium bg-danger/20 text-danger rounded">
                                    {format!(
                                        "{} backup failure{}",
                                        pending_failures,
                                        if pending_failures == 1 { "" } else { "s" },
                                    )}
                                </span>
                            }
                            .into_any()
                        } else {
                            ().into_any()
                        }}
                    </div>
                    <p class="text-sm text-text-secondary">
                        {format!("{} ({})", entry.destination_type, entry.provider)}
                        {if !is_primary && !backup_mode_label.is_empty() {
                            format!(" · {backup_mode_label}")
                        } else {
                            String::new()
                        }}
                    </p>
                </div>

                {if !is_primary {
                    let destination_id_promote = entry.destination_id.clone();
                    let destination_id_delete = entry.destination_id.clone();
                    let on_refresh_promote = on_refresh.clone();
                    let on_refresh_delete = on_refresh.clone();
                    view! {
                        <div class="flex items-center gap-2">
                            <button
                                class="px-3 py-1 text-sm text-bone bg-steel rounded cursor-pointer hover:bg-steel/80 transition-colors disabled:opacity-50"
                                on:click=move |_| {
                                    is_promoting.set(true);
                                    let dest_id = destination_id_promote.clone();
                                    let on_ref = on_refresh_promote.clone();
                                    leptos::task::spawn_local(async move {
                                        match invoke_command::<SetPrimaryDestinationRequest, ()>(
                                            "set_primary_destination_cmd",
                                            &SetPrimaryDestinationRequest {
                                                destination_id: dest_id,
                                            },
                                        )
                                        .await
                                        {
                                            Ok(()) => {
                                                is_promoting.set(false);
                                                crate::components::use_toast()
                                                    .success(
                                                        "Primary destination updated.".to_string(),
                                                    );
                                                on_ref();
                                            }
                                            Err(e) => {
                                                is_promoting.set(false);
                                                crate::components::use_toast()
                                                    .error(format!("Failed to set primary: {e}"));
                                            }
                                        }
                                    });
                                }
                                disabled=move || {
                                    is_promoting.get() || is_deleting.get() || show_confirm.get()
                                }
                            >
                                {move || if is_promoting.get() { "Promoting…" } else { "Set as Primary" }}
                            </button>

                            <button
                                class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                                on:click=move |_| show_confirm.set(true)
                                disabled=move || {
                                    is_deleting.get() || is_promoting.get() || show_confirm.get()
                                }
                            >
                                "Delete"
                            </button>

                            {move || {
                                if show_confirm.get() {
                                    let dest_id = destination_id_delete.clone();
                                    let on_del = on_refresh_delete.clone();
                                    view! {
                                        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                                            <div class="bg-stone p-6 rounded border border-steel">
                                                <p class="mb-4 text-bone">
                                                    "Are you sure you want to delete this destination?"
                                                </p>
                                                <div class="flex gap-3">
                                                    <button
                                                        class="px-4 py-2 bg-steel text-bone rounded cursor-pointer hover:bg-rune/20 transition-colors"
                                                        on:click=move |_| show_confirm.set(false)
                                                    >
                                                        "Cancel"
                                                    </button>
                                                    <button
                                                        class="px-4 py-2 bg-rune text-bone rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                                                        on:click=move |_| {
                                                            is_deleting.set(true);
                                                            let id = dest_id.clone();
                                                            let cb = on_del.clone();
                                                            leptos::task::spawn_local(async move {
                                                                match invoke_command::<
                                                                    DeleteDestinationRequest,
                                                                    (),
                                                                >(
                                                                    "delete_destination",
                                                                    &DeleteDestinationRequest {
                                                                        destination_id: id,
                                                                    },
                                                                )
                                                                .await
                                                                {
                                                                    Ok(()) => {
                                                                        is_deleting.set(false);
                                                                        show_confirm.set(false);
                                                                        cb();
                                                                    }
                                                                    Err(e) => {
                                                                        is_deleting.set(false);
                                                                        show_confirm.set(false);
                                                                        crate::components::use_toast()
                                                                            .error(format!(
                                                                                "Failed to delete: {e}"
                                                                            ));
                                                                    }
                                                                }
                                                            });
                                                        }
                                                        disabled=move || is_deleting.get()
                                                    >
                                                        {move || {
                                                            if is_deleting.get() { "Deleting…" } else { "Delete" }
                                                        }}
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
                    .into_any()
                } else {
                    ().into_any()
                }}
            </div>

            // Sharing status row — shown only for Google Drive destinations.
            {if is_gdrive {
                view! { <GdriveShareStatus /> }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

// ─── AddDestinationForm ───────────────────────────────────────────────────────

/// Form to add a new backup destination.
///
/// When the selected destination is Google Drive, an optional sharing setup
/// step appears so the owner can configure the Service Account key before saving.
#[component]
fn AddDestinationForm(on_added: Arc<dyn Fn() + Send + Sync>) -> impl IntoView {
    let label = RwSignal::new(String::new());
    let backup_mode = RwSignal::new("mirror".to_string());
    let is_adding = RwSignal::new(false);
    let validation_error = RwSignal::new(String::new());
    let show_gdrive_sharing_modal = RwSignal::new(false);

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

    // Derived: true when the currently selected config is Google Drive.
    let is_gdrive_selected = move || config.read().rclone_config_blob.contains("type = drive");

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
                        on:change=move |ev| backup_mode.set(event_target_value(&ev))
                        disabled=move || is_adding.get()
                    >
                        <option value="mirror">"Mirror — keep destination in sync with source"</option>
                        <option value="accumulating">"Accumulating — retain deleted files"</option>
                    </select>
                </div>

                <DestinationSelector on_change=move |c| config.set(c) />

                // Optional sharing setup — shown only after Google Drive is configured.
                {move || {
                    if is_gdrive_selected() {
                        view! {
                            <div class="p-3 border border-steel/60 rounded bg-stone/40 text-sm">
                                <p class="font-medium text-bone mb-1">"File sharing (optional)"</p>
                                <p class="text-text-secondary mb-2">
                                    "To share files from this destination, you need a GCP Service "
                                    "Account key. You can set this up now or later from the destination list."
                                </p>
                                <button
                                    class="text-rune hover:text-rune/80 underline text-sm cursor-pointer"
                                    on:click=move |_| show_gdrive_sharing_modal.set(true)
                                >
                                    "Set up sharing →"
                                </button>
                            </div>
                        }
                        .into_any()
                    } else {
                        ().into_any()
                    }
                }}

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

        // Sharing setup modal — launched from the optional step above.
        {move || {
            if show_gdrive_sharing_modal.get() {
                let on_file_picked = move |path: String| {
                    leptos::task::spawn_local(async move {
                        match invoke_command::<_, ()>(
                            "set_gdrive_service_account",
                            &SetGdriveServiceAccountRequest { sa_json_path: path },
                        )
                        .await
                        {
                            Ok(()) => {
                                crate::components::use_toast()
                                    .success("Google Drive sharing enabled.".to_string());
                            }
                            Err(e) => {
                                crate::components::use_toast()
                                    .error(format!("Could not save key: {e}"));
                            }
                        }
                    });
                };
                let on_close = move || show_gdrive_sharing_modal.set(false);
                view! { <GdriveShareSetupModal on_file_picked=on_file_picked on_close=on_close /> }.into_any()
            } else {
                ().into_any()
            }
        }}
    }
}

// ─── DestinationList ──────────────────────────────────────────────────────────

/// List of configured destinations with add and delete controls.
#[component]
pub fn DestinationList() -> impl IntoView {
    let refresh_count = RwSignal::new(0u32);
    let sync_state = use_sync();

    let destinations = LocalResource::new(move || {
        let _trigger = refresh_count.get();
        async move { invoke_command::<(), Vec<DestinationEntry>>("list_destinations", &()).await }
    });

    let health_data = LocalResource::new(move || {
        let _trigger = refresh_count.get();
        let _sync_at = sync_state.with(|s| s.last_synced_at.clone());
        async move { invoke_command::<(), Vec<DestinationHealth>>("get_backup_health", &()).await }
    });

    let on_refresh: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
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

            <AddDestinationForm on_added=on_refresh.clone() />

            <div>
                <h3 class="text-lg font-semibold text-bone mb-3">"Configured Destinations"</h3>
                <Suspense fallback=move || {
                    view! { <p class="text-text-secondary">"Loading destinations…"</p> }
                }>
                    {move || {
                        let health: Vec<DestinationHealth> = health_data
                            .get()
                            .and_then(|r| r.ok())
                            .unwrap_or_default();
                        destinations.get().map(|result| match result {
                            Ok(entries) if !entries.is_empty() => {
                                let on_refresh_ref = on_refresh.clone();
                                view! {
                                    <div class="space-y-2">
                                        {entries
                                            .into_iter()
                                            .map(|entry| {
                                                let failures = health
                                                    .iter()
                                                    .find(|h| h.destination_id == entry.destination_id)
                                                    .map(|h| h.pending_failure_blobs)
                                                    .unwrap_or(0);
                                                view! {
                                                    <DestinationItem
                                                        entry
                                                        on_refresh=on_refresh_ref.clone()
                                                        pending_failures=failures
                                                    />
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                .into_any()
                            }
                            Ok(_) => {
                                view! {
                                    <p class="text-text-secondary">
                                        "No destinations configured yet."
                                    </p>
                                }
                                .into_any()
                            }
                            Err(e) => {
                                view! {
                                    <p class="text-danger">
                                        {"Error loading destinations: "}{e.to_string()}
                                    </p>
                                }
                                .into_any()
                            }
                        })
                    }}
                </Suspense>
            </div>
        </div>
    }
}
