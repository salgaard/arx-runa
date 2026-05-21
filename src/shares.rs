//! Share management: sent shares, received shares, and per-file sharing.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::dialog::{open_file_dialog_arxshare, open_save_dialog};
use crate::invoke::{invoke_command, invoke_command_with_channel};
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::{
    ContactEntry, DownloadReceivedShareRequest, DownloadReceivedShareResponse, FileContentResponse,
    GetReceivedShareContentRequest, ImportShareRequest, ImportShareResponse, ProgressUpdate,
    ReceivedShareEntry, RevokeShareRequest, ShareEntry, ShareFileRequest, ShareResponse,
};
use crate::state::use_sync;
use crate::transfer::ProgressModal;
use crate::utils::format_fingerprint;
use crate::vault::{ContentViewerModal, extension_is_previewable, file_size_allows_preview};

// ─── SharesPage (main component) ──────────────────────────────────────────

/// Main shares page with Sent/Received tabs.
#[component]
pub fn SharesPage() -> impl IntoView {
    let active_tab = RwSignal::new("sent");

    view! {
        <div class="flex flex-col gap-6">
            <div class="flex justify-between items-center">
                <h1 class="text-2xl font-bold text-bone">"Shares"</h1>
                <A href="/">
                    <button class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors">
                        "← Back to Vault"
                    </button>
                </A>
            </div>

            <div class="flex gap-4 border-b border-steel">
                <button
                    class=move || {
                        if active_tab.get() == "sent" {
                            "px-4 py-2 border-b-2 border-rune text-bone font-semibold cursor-pointer"
                        } else {
                            "px-4 py-2 text-text-secondary cursor-pointer hover:text-bone transition-colors"
                        }
                    }
                    on:click=move |_| active_tab.set("sent")
                >
                    "Sent"
                </button>
                <button
                    class=move || {
                        if active_tab.get() == "received" {
                            "px-4 py-2 border-b-2 border-rune text-bone font-semibold cursor-pointer"
                        } else {
                            "px-4 py-2 text-text-secondary cursor-pointer hover:text-bone transition-colors"
                        }
                    }
                    on:click=move |_| active_tab.set("received")
                >
                    "Received"
                </button>
            </div>

            {move || {
                if active_tab.get() == "sent" {
                    view! { <SentSharesList /> }.into_any()
                } else {
                    view! { <ReceivedSharesList /> }.into_any()
                }
            }}
        </div>
    }
}

// ─── SentSharesList ───────────────────────────────────────────────────────

/// Displays a list of shares sent to contacts.
#[component]
fn SentSharesList() -> impl IntoView {
    let shares = RwSignal::new(Vec::<ShareEntry>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let refresh_key = RwSignal::new(0);

    // Load shares on mount and when refresh_key changes
    Effect::new(move |_| {
        let _ = refresh_key.get(); // Dependency for refresh
        let shares_clone = shares;
        let loading_clone = loading;
        let error_clone = error;

        spawn_local(async move {
            loading_clone.set(true);
            error_clone.set(None);
            match invoke_command::<(), Vec<ShareEntry>>("list_shares", &()).await {
                Ok(entries) => {
                    shares_clone.set(entries);
                }
                Err(e) => {
                    error_clone.set(Some(e.to_string()));
                }
            }
            loading_clone.set(false);
        });
    });

    let checking_receipts = RwSignal::new(false);

    // Auto-check receipts once on mount.
    Effect::new(move |_| {
        let shares_clone = shares;
        let checking_clone = checking_receipts;
        spawn_local(async move {
            checking_clone.set(true);
            if let Ok(entries) =
                invoke_command::<(), Vec<ShareEntry>>("check_share_receipts", &()).await
            {
                shares_clone.set(entries);
            }
            checking_clone.set(false);
        });
    });

    let handle_check_receipts = move |_| {
        checking_receipts.set(true);
        spawn_local(async move {
            if let Ok(entries) =
                invoke_command::<(), Vec<ShareEntry>>("check_share_receipts", &()).await
            {
                shares.set(entries)
            }
            checking_receipts.set(false);
        });
    };

    // Re-check receipts whenever a pull/sync completes (last_synced_at changes).
    let sync_state = use_sync();
    Effect::new(move |prev: Option<Option<String>>| {
        let current = sync_state.with(|s| s.last_synced_at.clone());
        if let Some(prev_ts) = prev
            && current != prev_ts
        {
            spawn_local(async move {
                if let Ok(entries) =
                    invoke_command::<(), Vec<ShareEntry>>("check_share_receipts", &()).await
                {
                    shares.set(entries);
                }
            });
        }
        current
    });

    view! {
        <div class="flex flex-col gap-4">
            <div class="flex justify-end">
                <button
                    class="px-3 py-1 text-sm text-bone bg-steel rounded cursor-pointer hover:bg-rune/20 transition-colors disabled:opacity-50"
                    on:click=handle_check_receipts
                    disabled=move || checking_receipts.get()
                >
                    {move || if checking_receipts.get() { "Checking…" } else { "Refresh receipt status" }}
                </button>
            </div>
            {move || {
                if loading.get() {
                    view! { <p class="text-text-secondary">"Loading shares…"</p> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <p class="text-danger">"Error: " {err}</p> }.into_any()
                } else if shares.get().is_empty() {
                    view! { <p class="text-text-secondary">"No shares yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="grid gap-4">
                            {move || {
                                shares.get().into_iter().map(|share| {
                                    view! {
                                        <SentShareItem
                                            share=share.clone()
                                            on_revoke=move || {
                                                refresh_key.set(refresh_key.get() + 1);
                                            }
                                        />
                                    }
                                }).collect_view()
                            }}
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

/// Individual sent share item.
#[component]
fn SentShareItem(share: ShareEntry, #[prop(into)] on_revoke: Callback<()>) -> impl IntoView {
    let show_confirm = RwSignal::new(false);
    let revoking = RwSignal::new(false);
    let share_id = share.share_id.clone();
    let contact_name = share.contact_name.clone();
    let receipt_received_at = share.receipt_received_at.clone();
    let import_receipt_received_at = share.import_receipt_received_at.clone();
    let expires_at = share.expires_at.clone();

    view! {
        <div class="p-4 bg-iron border border-steel rounded">
            <div class="flex justify-between items-start">
                <div>
                    <p class="text-bone font-semibold">{share.file_name.clone()}</p>
                    <p class="text-text-secondary text-sm">"Shared with: " {contact_name.clone()}</p>
                    <p class="text-text-secondary text-xs mt-1">{share.created_at.clone()}</p>
                    {if let Some(ts) = receipt_received_at {
                        view! {
                            <p class="text-success-text text-xs mt-1">"Downloaded " {ts}</p>
                        }.into_any()
                    } else if let Some(ts) = import_receipt_received_at {
                        view! {
                            <p class="text-text-secondary text-xs mt-1">"Received " {ts}</p>
                        }.into_any()
                    } else {
                        view! {
                            <p class="text-text-secondary text-xs mt-1">"Awaiting receipt"</p>
                        }.into_any()
                    }}
                    {expires_at.map(|ts| view! {
                        <p class="text-text-secondary text-xs mt-1">"Expires " {ts}</p>
                    })}
                    {move || {
                        if share.revoked {
                            view! {
                                <p class="text-danger text-sm mt-2">"(Revoked)"</p>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
                {move || {
                    if !share.revoked {
                        view! {
                            <button
                                class="px-3 py-1 text-sm text-danger bg-danger/20 rounded cursor-pointer hover:bg-danger/30 transition-colors disabled:opacity-50"
                                on:click=move |_| show_confirm.set(true)
                                disabled=move || revoking.get()
                            >
                                {move || if revoking.get() { "Revoking…" } else { "Revoke" }}
                            </button>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
            </div>

            {move || {
                if show_confirm.get() {
                    let share_id_for_revoke = share_id.clone();
                    let contact_name_for_confirm = contact_name.clone();

                    view! {
                        <div class="mt-4 p-3 bg-danger/10 border border-danger rounded">
                            <p class="text-bone text-sm mb-3">"Revoke access for " {contact_name_for_confirm} "?"</p>
                            <div class="flex gap-2">
                                <button
                                    class="px-3 py-1 text-sm bg-danger/20 text-danger rounded cursor-pointer hover:bg-danger/30 transition-colors"
                                    on:click=move |_| {
                                        revoking.set(true);
                                        let share_id_clone = share_id_for_revoke.clone();

                                        spawn_local(async move {
                                            match invoke_command::<RevokeShareRequest, ()>(
                                                "revoke_share",
                                                &RevokeShareRequest {
                                                    share_id: share_id_clone,
                                                },
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    show_confirm.set(false);
                                                    on_revoke.run(());
                                                }
                                                Err(_e) => {
                                                    // Error handled by IPC layer
                                                }
                                            }
                                            revoking.set(false);
                                        });
                                    }
                                    disabled=move || revoking.get()
                                >
                                    "Confirm Revoke"
                                </button>
                                <button
                                    class="px-3 py-1 text-sm bg-steel text-bone rounded cursor-pointer hover:bg-rune/20 transition-colors"
                                    on:click=move |_| show_confirm.set(false)
                                    disabled=move || revoking.get()
                                >
                                    "Cancel"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

// ─── ReceivedSharesList ───────────────────────────────────────────────────

/// Displays a list of shares received from other users.
#[component]
fn ReceivedSharesList() -> impl IntoView {
    let shares = RwSignal::new(Vec::<ReceivedShareEntry>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let refresh_key = RwSignal::new(0);

    let importing_file = RwSignal::new(false);
    let import_error = RwSignal::new(None::<String>);

    let handle_import_file = move |_| {
        importing_file.set(true);
        import_error.set(None);
        let shares_clone = shares;
        let loading_clone = loading;
        let refresh = refresh_key;

        spawn_local(async move {
            let _ = (shares_clone, loading_clone);
            if let Some(path) = open_file_dialog_arxshare().await {
                match invoke_command::<ImportShareRequest, ImportShareResponse>(
                    "import_share",
                    &ImportShareRequest {
                        share_package_path: path,
                    },
                )
                .await
                {
                    Ok(_) => {
                        refresh.set(refresh.get() + 1);
                    }
                    Err(e) => {
                        import_error.set(Some(e.to_string()));
                    }
                }
            }
            importing_file.set(false);
        });
    };

    // Load shares on mount and when refresh_key changes
    Effect::new(move |_| {
        let _ = refresh_key.get(); // Dependency for refresh
        let shares_clone = shares;
        let loading_clone = loading;
        let error_clone = error;

        spawn_local(async move {
            loading_clone.set(true);
            error_clone.set(None);
            match invoke_command::<(), Vec<ReceivedShareEntry>>("list_received_shares", &()).await {
                Ok(entries) => {
                    shares_clone.set(entries);
                }
                Err(e) => {
                    error_clone.set(Some(e.to_string()));
                }
            }
            loading_clone.set(false);
        });
    });

    view! {
        <div class="flex flex-col gap-4">
            <div class="flex items-center gap-3">
                <button
                    class="px-3 py-2 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                    on:click=handle_import_file
                    disabled=move || importing_file.get()
                >
                    {move || if importing_file.get() { "Importing…" } else { "Import from file" }}
                </button>
                {move || import_error.get().map(|e| view! {
                    <p class="text-danger text-sm">{e}</p>
                })}
            </div>

            {move || {
                if loading.get() {
                    view! { <p class="text-text-secondary">"Loading shares…"</p> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <p class="text-danger">"Error: " {err}</p> }.into_any()
                } else if shares.get().is_empty() {
                    view! { <p class="text-text-secondary">"No received shares yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="grid gap-4">
                            {move || {
                                shares.get().into_iter().map(|share| {
                                    view! {
                                        <ReceivedShareItem
                                            share=share.clone()
                                            on_refresh=move || {
                                                refresh_key.set(refresh_key.get() + 1);
                                            }
                                        />
                                    }
                                }).collect_view()
                            }}
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

/// Individual received share item.
#[component]
fn ReceivedShareItem(
    share: ReceivedShareEntry,
    #[prop(into)] on_refresh: Callback<()>,
) -> impl IntoView {
    let show_download_warn = RwSignal::new(false);
    let downloading = RwSignal::new(false);
    let download_error = RwSignal::new(None::<String>);
    let download_success = RwSignal::new(None::<String>);
    let file_content = RwSignal::new(None::<FileContentResponse>);
    let preview_loading = RwSignal::new(false);
    let (download_channel, set_download_channel) =
        signal::<Option<IpcChannel<ProgressUpdate>>>(None);

    let file_name_stored = StoredValue::new(share.file_name.clone());
    let display_file_name = share.file_name.clone();
    let display_sender_name = share.sender_name.clone();
    let display_imported_at = share.imported_at.clone();
    let display_expires_at = share.expires_at.clone();

    let is_expired = share.is_expired;
    let can_preview = !is_expired
        && file_size_allows_preview(share.size_bytes)
        && extension_is_previewable(&share.file_name);

    let share_id_for_preview = share.share_id.clone();
    let share_id_for_download = share.share_id.clone();
    let file_name_for_download = share.file_name.clone();

    let handle_preview = move |_| {
        preview_loading.set(true);
        let share_id = share_id_for_preview.clone();
        spawn_local(async move {
            match invoke_command::<GetReceivedShareContentRequest, FileContentResponse>(
                "get_received_share_content",
                &GetReceivedShareContentRequest {
                    share_id: share_id.clone(),
                },
            )
            .await
            {
                Ok(content) => {
                    file_content.set(Some(content));
                }
                Err(e) => {
                    download_error.set(Some(e.to_string()));
                }
            }
            preview_loading.set(false);
        });
    };

    // Opens the plaintext-on-disk warning; the actual download runs on confirmation.
    let handle_download_click = move |_| show_download_warn.set(true);

    view! {
        <>
            <div class="p-4 bg-iron border border-steel rounded">
                <div class="flex justify-between items-start">
                    <div>
                        <p
                            class=move || {
                                if can_preview {
                                    "text-bone font-semibold cursor-pointer hover:underline"
                                } else {
                                    "text-bone font-semibold"
                                }
                            }
                            on:click=move |e| {
                                if can_preview {
                                    handle_preview(e);
                                }
                            }
                        >
                            {display_file_name}
                            <Show when=move || preview_loading.get() fallback=|| ()>
                                <span class="ml-2 text-text-secondary text-xs">"Loading…"</span>
                            </Show>
                        </p>
                        {display_sender_name.map(|sender| {
                            view! {
                                <p class="text-text-secondary text-sm">"From: " {sender}</p>
                            }
                        })}
                        <p class="text-text-secondary text-xs mt-1">{display_imported_at}</p>
                    </div>
                    {if is_expired {
                        view! {
                            <span
                                class="px-3 py-1 text-sm text-danger bg-danger/20 rounded cursor-default"
                                title="Contact sender for renewed access"
                            >
                                "Expired"
                            </span>
                        }
                        .into_any()
                    } else {
                        view! {
                            <button
                                class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                                on:click=handle_download_click
                                disabled=move || downloading.get()
                            >
                                {move || if downloading.get() { "Downloading…" } else { "Download" }}
                            </button>
                        }
                        .into_any()
                    }}
                </div>
                {if is_expired {
                    let expires_msg = display_expires_at
                        .as_ref()
                        .map(|ts| format!("Share expired on {} — contact sender for renewed access", ts))
                        .unwrap_or_else(|| "Share expired — contact sender for renewed access".to_string());
                    view! {
                        <p class="text-danger text-xs mt-2">{expires_msg}</p>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }}
                {move || {
                    if show_download_warn.get() {
                        let share_id_for_warn = share_id_for_download.clone();
                        let file_name_for_warn = file_name_for_download.clone();
                        view! {
                            <div class="mt-4 p-3 bg-amber-900/30 border border-amber-600 rounded">
                                <p class="text-bone text-sm mb-3">
                                    "The downloaded file will be written to disk in plaintext, outside vault protection. You are responsible for the exported copy."
                                </p>
                                <div class="flex gap-2">
                                    <button
                                        class="px-3 py-1 text-sm bg-amber-600 text-white rounded cursor-pointer hover:bg-amber-500 transition-colors"
                                        on:click=move |_| {
                                            show_download_warn.set(false);
                                            downloading.set(true);
                                            download_error.set(None);
                                            download_success.set(None);
                                            let share_id = share_id_for_warn.clone();
                                            let file_name = file_name_for_warn.clone();
                                            spawn_local(async move {
                                                if let Some(dest_path) = open_save_dialog(Some(&file_name)).await {
                                                    let channel = IpcChannel::<ProgressUpdate>::new();
                                                    set_download_channel.set(Some(channel.clone()));
                                                    match invoke_command_with_channel::<DownloadReceivedShareRequest, DownloadReceivedShareResponse>(
                                                        "download_received_share",
                                                        &DownloadReceivedShareRequest {
                                                            share_id: share_id.clone(),
                                                            destination_path: dest_path,
                                                        },
                                                        "progress",
                                                        channel.inner(),
                                                    )
                                                    .await
                                                    {
                                                        Ok(resp) => {
                                                            download_success.set(Some(format!("Saved: {}", resp.file_name)));
                                                            on_refresh.run(());
                                                        }
                                                        Err(e) => {
                                                            download_error.set(Some(e.to_string()));
                                                        }
                                                    }
                                                    set_download_channel.set(None);
                                                }
                                                downloading.set(false);
                                            });
                                        }
                                    >
                                        "Download anyway"
                                    </button>
                                    <button
                                        class="px-3 py-1 text-sm bg-steel text-bone rounded cursor-pointer hover:bg-rune/20 transition-colors"
                                        on:click=move |_| show_download_warn.set(false)
                                    >
                                        "Cancel"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
                {move || download_error.get().map(|e| view! {
                    <p class="text-danger text-sm mt-2">{e}</p>
                })}
                {move || download_success.get().map(|msg| view! {
                    <p class="text-success-text text-sm mt-2">{msg}</p>
                })}
            </div>

            <ContentViewerModal
                content=file_content
                filename=file_name_stored.get_value()
            />
            <Show when=move || download_channel.get().is_some() fallback=|| ()>
                {move || download_channel.get().map(|ch| view! {
                    <ProgressModal
                        channel=ch
                        title="Downloading file…"
                        on_close=move || set_download_channel.set(None)
                    />
                })}
            </Show>
        </>
    }
}

// ─── ShareModal (used from vault.rs) ──────────────────────────────────────

/// Modal to share a file with a contact.
#[component]
pub fn ShareModal(
    file_id: String,
    file_name: String,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_success: Callback<ShareResponse>,
) -> impl IntoView {
    let selected_contact_id = RwSignal::new(None::<String>);
    let expiration_days = RwSignal::new(Some("30".to_string()));
    let contacts = RwSignal::new(Vec::<ContactEntry>::new());
    let loading_contacts = RwSignal::new(true);
    let sharing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Load contacts when modal opens (via LocalResource equivalent)
    Effect::new(move |_| {
        let contacts_clone = contacts;
        let loading_clone = loading_contacts;

        spawn_local(async move {
            loading_clone.set(true);
            match invoke_command::<(), Vec<ContactEntry>>("list_contacts", &()).await {
                Ok(entries) => {
                    contacts_clone.set(entries);
                }
                Err(_e) => {
                    // Error handled silently; show empty list
                }
            }
            loading_clone.set(false);
        });
    });

    let handle_share = move |_| {
        if selected_contact_id.get().is_none() {
            error.set(Some("Please select a contact".to_string()));
            return;
        }

        sharing.set(true);
        error.set(None);

        let contact_id = selected_contact_id.get().unwrap();
        let file_id_clone = file_id.clone();
        let expiry_opt = expiration_days.get().and_then(|s| s.parse::<u32>().ok());

        spawn_local(async move {
            match invoke_command::<ShareFileRequest, ShareResponse>(
                "share_file",
                &ShareFileRequest {
                    file_id: file_id_clone,
                    contact_id: contact_id.clone(),
                    expiration_days: expiry_opt,
                },
            )
            .await
            {
                Ok(response) => {
                    on_success.run(response);
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }
            sharing.set(false);
        });
    };

    view! {
        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div class="bg-stone p-6 rounded border border-steel max-w-md w-full mx-4">
                <h3 class="text-lg font-semibold text-bone mb-1">"Share file"</h3>
                <p class="text-text-secondary text-sm mb-4">{file_name}</p>

                <div class="flex flex-col gap-4 mb-4">
                    <div>
                        <label class="block text-sm text-text-secondary mb-1">"Recipient"</label>
                        {move || {
                            if loading_contacts.get() {
                                view! {
                                    <p class="text-text-secondary text-sm">"Loading contacts…"</p>
                                }.into_any()
                            } else if contacts.get().is_empty() {
                                view! {
                                    <p class="text-text-secondary text-sm">"No contacts available."</p>
                                }.into_any()
                            } else {
                                view! {
                                    <select
                                        class="w-full px-3 py-2 bg-iron border border-steel text-bone rounded"
                                        on:change=move |e| {
                                            let value = event_target_value(&e);
                                            selected_contact_id.set(if value.is_empty() { None } else { Some(value) });
                                        }
                                    >
                                        <option value="">"-- Select a contact --"</option>
                                        {move || {
                                            contacts.get().into_iter().map(|contact| {
                                                view! {
                                                    <option value=contact.contact_id.clone()>
                                                        {contact.display_name}
                                                    </option>
                                                }
                                            }).collect_view()
                                        }}
                                    </select>
                                }.into_any()
                            }
                        }}
                    </div>

                    {move || {
                        selected_contact_id.get().and_then(|selected_id| {
                            contacts.get().into_iter().find(|c| c.contact_id == selected_id).map(|selected_contact| {
                                let fingerprint = format_fingerprint(&selected_contact.public_key);
                                view! {
                                    <div class="p-3 bg-stone border border-steel rounded">
                                        <p class="text-text-secondary text-xs mb-2">"Recipient fingerprint (verify before sharing)"</p>
                                        <div class="bg-iron p-2 rounded border border-steel-light cursor-text select-all">
                                            <code class="text-bone font-mono text-sm">{fingerprint}</code>
                                        </div>
                                        <p class="text-text-secondary text-xs mt-2 italic">
                                            "Verify this fingerprint matches what the recipient sees on their device (phone, video call, QR code, etc.)"
                                        </p>
                                    </div>
                                }
                            })
                        })
                    }}

                    <div>
                        <label class="block text-sm text-text-secondary mb-1">"Expiration"</label>
                        <select
                            class="w-full px-3 py-2 bg-iron border border-steel text-bone rounded"
                            on:change=move |e| {
                                let value = event_target_value(&e);
                                expiration_days.set(if value.is_empty() { None } else { Some(value) });
                            }
                            disabled=move || sharing.get()
                        >
                            <option value="7" selected=move || expiration_days.get().as_deref() == Some("7")>"7 days"</option>
                            <option value="30" selected=move || expiration_days.get().as_deref() == Some("30")>"30 days"</option>
                            <option value="90" selected=move || expiration_days.get().as_deref() == Some("90")>"90 days"</option>
                            <option value="" selected=move || expiration_days.get().is_none()>"No expiration"</option>
                        </select>
                    </div>
                </div>

                {move || {
                    error.get().map(|msg| {
                        view! {
                            <div class="p-2 bg-danger/20 text-danger rounded text-sm mb-4">
                                {msg}
                            </div>
                        }
                    })
                }}

                <div class="flex gap-2">
                    <button
                        class="flex-1 px-4 py-2 bg-rune text-bone rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                        on:click=handle_share
                        disabled=move || sharing.get() || selected_contact_id.get().is_none()
                    >
                        {move || if sharing.get() { "Sharing…" } else { "Share" }}
                    </button>
                    <button
                        class="flex-1 px-4 py-2 bg-steel text-bone rounded cursor-pointer hover:bg-rune/20 transition-colors"
                        on:click=move |_| on_close.run(())
                        disabled=move || sharing.get()
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
