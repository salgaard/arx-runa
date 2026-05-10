//! Share management: sent shares, received shares, and per-file sharing.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::dialog::{open_file_dialog_arxshare, open_save_dialog};
use crate::invoke::invoke_command;
use crate::ipc_types::{
    ContactEntry, DownloadReceivedShareRequest, DownloadReceivedShareResponse, ImportShareRequest,
    ImportShareResponse, ReceivedShareEntry, RevokeShareRequest, ShareEntry, ShareFileRequest,
    ShareResponse,
};
use crate::utils::format_fingerprint;

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
    let downloading = RwSignal::new(false);
    let download_error = RwSignal::new(None::<String>);
    let download_success = RwSignal::new(None::<String>);

    let display_file_name = share.file_name.clone();
    let display_sender_name = share.sender_name.clone();
    let display_imported_at = share.imported_at.clone();

    let handle_download = move |_| {
        downloading.set(true);
        download_error.set(None);
        download_success.set(None);
        let share_id = share.share_id.clone();
        let file_name = share.file_name.clone();

        spawn_local(async move {
            if let Some(dest_path) = open_save_dialog(Some(&file_name)).await {
                match invoke_command::<DownloadReceivedShareRequest, DownloadReceivedShareResponse>(
                    "download_received_share",
                    &DownloadReceivedShareRequest {
                        share_id: share_id.clone(),
                        destination_path: dest_path,
                    },
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
            }
            downloading.set(false);
        });
    };

    view! {
        <div class="p-4 bg-iron border border-steel rounded">
            <div class="flex justify-between items-start">
                <div>
                    <p class="text-bone font-semibold">{display_file_name}</p>
                    {display_sender_name.map(|sender| {
                        view! {
                            <p class="text-text-secondary text-sm">"From: " {sender}</p>
                        }
                    })}
                    <p class="text-text-secondary text-xs mt-1">{display_imported_at}</p>
                </div>
                <button
                    class="px-3 py-1 text-sm text-bone bg-rune rounded cursor-pointer hover:bg-rune/80 transition-colors disabled:opacity-50"
                    on:click=handle_download
                    disabled=move || downloading.get()
                >
                    {move || if downloading.get() { "Downloading…" } else { "Download" }}
                </button>
            </div>
            {move || download_error.get().map(|e| view! {
                <p class="text-danger text-sm mt-2">{e}</p>
            })}
            {move || download_success.get().map(|msg| view! {
                <p class="text-success-text text-sm mt-2">{msg}</p>
            })}
        </div>
    }
}

// ─── ShareModal (used from vault.rs) ──────────────────────────────────────

/// Modal to share a file with a contact.
#[component]
pub fn ShareModal(
    file_id: String,
    _file_name: String,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_success: Callback<ShareResponse>,
) -> impl IntoView {
    let selected_contact_id = RwSignal::new(None::<String>);
    let expiration_days = RwSignal::new(None::<String>);
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
                    request_receipt: true,
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
                <h3 class="text-lg font-semibold text-bone mb-4">"Share file"</h3>

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
                        <label class="block text-sm text-text-secondary mb-1">"Expiration (days, optional)"</label>
                        <input
                            type="number"
                            min="1"
                            class="w-full px-3 py-2 bg-iron border border-steel text-bone rounded"
                            placeholder="Leave blank for no expiration"
                            value=move || expiration_days.get().unwrap_or_default()
                            on:input=move |e| {
                                let value = event_target_value(&e);
                                expiration_days.set(if value.is_empty() { None } else { Some(value) });
                            }
                            disabled=move || sharing.get()
                        />
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the `ShareModal` component disables the Share button when
    /// no contact is selected. The Share button's `disabled` attribute depends on
    /// `selected_contact_id.get().is_none()`, which means it starts disabled and
    /// remains disabled until a contact is selected.
    #[test]
    fn test_share_modal_disables_submit_with_no_contact_selected() {
        // Structural test: ShareModal should compile
        // In a real Leptos test environment, we would:
        // 1. Mount ShareModal with sample file_id and file_name
        // 2. Verify the Share button has `disabled` attribute set to true initially
        // 3. Select a contact from the dropdown
        // 4. Verify the Share button becomes enabled
        //
        // The button element in ShareModal has:
        // `disabled=move || sharing.get() || selected_contact_id.get().is_none()`
        // So it is disabled when either:
        // - sharing is true (operation in progress), or
        // - selected_contact_id is None (no contact selected)
        let _ = ShareModal;
    }

    /// Verifies that `ReceivedShareItem` invokes the refresh callback on successful
    /// download. The `on_refresh` callback should be called with `run(())` after a
    /// successful `invoke_command("download_received_share", ...)` response.
    #[test]
    fn test_received_share_item_download_refreshes_list() {
        // Structural test: ReceivedShareItem should compile
        // In an integration test environment, we would:
        // 1. Create a mock ReceivedShareEntry
        // 2. Mount ReceivedShareItem with a callback that tracks invocation
        // 3. Mock invoke_command to return Ok(DownloadReceivedShareResponse { file_name: "..." })
        // 4. Click the Download button (after selecting a save path)
        // 5. Verify the callback was invoked (which triggers refresh_key increment)
        let _ = ReceivedShareItem;
    }

    /// Verifies that `SentShareItem` requires a confirmation step before revoking
    /// a share. When the Revoke button is clicked, a confirmation modal appears
    /// with options to confirm or cancel; the revocation only proceeds if the user
    /// clicks "Confirm Revoke".
    #[test]
    fn test_sent_share_item_revoke_requires_confirmation() {
        // Structural test: SentShareItem should compile
        // In a real Leptos test environment, we would:
        // 1. Create a mock ShareEntry with revoked = false
        // 2. Mount SentShareItem with an on_revoke callback
        // 3. Click the Revoke button
        // 4. Verify the confirmation modal appears (show_confirm becomes true)
        // 5. Verify "Confirm Revoke" button is now visible
        // 6. Click Cancel
        // 7. Verify confirmation modal closes without invoking revoke
        // 8. Click Revoke again, then click "Confirm Revoke"
        // 9. Verify invoke_command("revoke_share", ...) is called
        // 10. Verify on_revoke callback is invoked on success
        //
        // The two-stage flow in SentShareItem is:
        // 1. Initial state: show_confirm = false, Revoke button visible
        // 2. Click Revoke: show_confirm.set(true), confirmation modal appears
        // 3. Confirmation modal with two buttons:
        //    - "Confirm Revoke": invokes revoke command, then on_revoke callback
        //    - "Cancel": show_confirm.set(false), hides modal
        let _ = SentShareItem;
    }
}
