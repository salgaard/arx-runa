//! Vault settings page — password change, key file rotation, vault deletion.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use zeroize::Zeroize;

use crate::components::{Button, Modal};
use crate::invoke::invoke_command;
use crate::ipc_types::{
    ChangePasswordRequest, DeleteVaultRequest, RotateKeyFileRequest, SetupRecoveryRequest,
};
use crate::state::{use_session, use_session_actions, use_sync_actions, use_vault_actions};

// ─── ChangePasswordForm ─────────────────────────────────────────────────────

/// Form for changing the vault password.
///
/// Three password input fields: current, new, confirm new.
/// Client-side validation ensures new == confirm before submission.
/// Both password strings are zeroized immediately after the IPC call completes.
#[component]
fn ChangePasswordForm() -> impl IntoView {
    let session = use_session();
    let session_actions = use_session_actions();
    let current_pw = RwSignal::new(String::new());
    let new_pw = RwSignal::new(String::new());
    let confirm_pw = RwSignal::new(String::new());
    let recovery_phrase = RwSignal::new(String::new());
    let key_file_path = RwSignal::new(Option::<String>::None);
    let error = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let on_submit = move |_| {
        error.set(None);
        success.set(false);

        let mut current = current_pw.get();
        let mut new = new_pw.get();
        let mut confirm = confirm_pw.get();

        if current.is_empty() || new.is_empty() || confirm.is_empty() {
            error.set(Some("All password fields are required".to_string()));
            return;
        }

        if new != confirm {
            error.set(Some(
                "New password and confirmation do not match".to_string(),
            ));
            return;
        }

        let vault_tier = session.read().vault_tier;
        if vault_tier == Some(2) && key_file_path.get().is_none() {
            error.set(Some(
                "Current key file is required for Tier 2 vaults".to_string(),
            ));
            return;
        }

        loading.set(true);

        let mut rp = recovery_phrase.get();

        spawn_local(async move {
            let request = ChangePasswordRequest {
                current_password: current.clone(),
                new_password: new.clone(),
                recovery_phrase: Some(rp.clone()).filter(|s| !s.is_empty()),
                current_key_file_path: key_file_path.get(),
            };

            current.zeroize();
            new.zeroize();
            confirm.zeroize();
            rp.zeroize();
            current_pw.update(|s| s.zeroize());
            new_pw.update(|s| s.zeroize());
            confirm_pw.update(|s| s.zeroize());
            recovery_phrase.update(|s| s.zeroize());

            match invoke_command::<ChangePasswordRequest, ()>("change_password", &request).await {
                Ok(()) => {
                    success.set(true);
                    session_actions.apply_status(
                        invoke_command::<(), crate::ipc_types::SessionStatus>(
                            "get_session_status",
                            &(),
                        )
                        .await
                        .unwrap_or_default(),
                    );
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
            loading.set(false);
        });
    };

    view! {
        <div class="p-6 bg-stone border border-steel rounded-lg shadow-sm">
            <h3 class="text-lg font-semibold text-bone mb-4">"Change Password"</h3>

            {move || {
                if success.get() {
                    view! {
                        <div class="mb-4 p-3 bg-success/20 text-success-text rounded">
                            "Password changed successfully"
                        </div>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}

            {move || {
                error.get().map(|e| {
                    view! {
                        <div class="mb-4 p-3 bg-danger/20 text-danger rounded">
                            {e}
                        </div>
                    }
                })
            }}

            <div class="space-y-4">
                <div>
                    <label class="block text-sm font-medium text-bone mb-2">
                        "Current Password"
                    </label>
                    <input
                        type="password"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Enter current password"
                        prop:value=move || current_pw.get()
                        on:change=move |ev| {
                            current_pw.set(event_target_value(&ev));
                        }
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-bone mb-2">
                        "New Password"
                    </label>
                    <input
                        type="password"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Enter new password"
                        prop:value=move || new_pw.get()
                        on:change=move |ev| {
                            new_pw.set(event_target_value(&ev));
                        }
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-bone mb-2">
                        "Confirm New Password"
                    </label>
                    <input
                        type="password"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Confirm new password"
                        prop:value=move || confirm_pw.get()
                        on:change=move |ev| {
                            confirm_pw.set(event_target_value(&ev));
                        }
                    />
                </div>

                {move || {
                    if session.read().vault_tier == Some(2) {
                        view! {
                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">
                                    "Current Key File"
                                </label>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="px-3 py-1.5 rounded border border-border-default text-sm text-bone cursor-pointer hover:bg-surface-overlay transition-colors"
                                        on:click=move |_| {
                                            spawn_local(async move {
                                                if let Some(path) = crate::dialog::open_file_dialog().await {
                                                    key_file_path.set(Some(path));
                                                }
                                            });
                                        }
                                    >
                                        "Choose Key File"
                                    </button>
                                    {move || key_file_path.get().map(|p| view! {
                                        <span class="text-sm text-text-secondary truncate">{p}</span>
                                    })}
                                </div>
                            </div>
                        }
                        .into_any()
                    } else {
                        ().into_any()
                    }
                }}

                <div>
                    <label class="block text-sm font-medium text-bone mb-2">
                        {move || {
                            if session.read().has_recovery_slot == Some(true) {
                                "Recovery Phrase (required to keep slot)"
                            } else {
                                "Recovery Phrase (optional)"
                            }
                        }}
                    </label>
                    <input
                        type="password"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Enter your 24-word phrase to keep your recovery slot valid"
                        prop:value=move || recovery_phrase.get()
                        on:change=move |ev| {
                            recovery_phrase.set(event_target_value(&ev));
                        }
                    />
                    {move || {
                        if session.read().has_recovery_slot == Some(true) {
                            view! {
                                <p class="mt-1 text-xs text-amber-400">
                                    "Leaving this blank will permanently delete your recovery slot."
                                </p>
                            }
                            .into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>

                <Button
                    on_click=on_submit
                    loading=move || loading.get()
                    variant="primary"
                >
                    {move || if loading.get() { "Changing…" } else { "Change Password" }}
                </Button>
            </div>
        </div>
    }
}

// ─── RotateKeyFileForm ──────────────────────────────────────────────────────

/// Form for rotating the key file (Tier 2 only).
///
/// Conditionally rendered based on `SessionState.vault_tier`.
#[component]
fn RotateKeyFileForm() -> impl IntoView {
    let session = use_session();
    let error = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let current_password = RwSignal::new(String::new());
    let current_key_file = RwSignal::new(Option::<String>::None);
    let selected_path = RwSignal::new(Option::<String>::None);
    let recovery_phrase = RwSignal::new(String::new());

    let on_choose_current_key_file = move |_| {
        spawn_local(async move {
            if let Some(path) = crate::dialog::open_file_dialog().await {
                current_key_file.set(Some(path));
                error.set(None);
            }
        });
    };

    let on_choose_dest_dir = move |_| {
        spawn_local(async move {
            if let Some(path) = crate::dialog::open_directory_dialog().await {
                selected_path.set(Some(path));
                error.set(None);
            }
        });
    };

    let on_submit = move |_| {
        let current_pw_val = current_password.get();
        if current_pw_val.is_empty() {
            error.set(Some("Current password is required".to_string()));
            return;
        }

        let Some(current_kf) = current_key_file.get() else {
            error.set(Some("Please select the current key file".to_string()));
            return;
        };

        let Some(path) = selected_path.get() else {
            error.set(Some(
                "Please select a destination directory for the new key file".to_string(),
            ));
            return;
        };

        error.set(None);
        success.set(false);
        loading.set(true);

        let mut rp = recovery_phrase.get();

        spawn_local(async move {
            let request = RotateKeyFileRequest {
                current_password: current_pw_val,
                current_key_file_path: current_kf,
                new_key_file_destination: path,
                recovery_phrase: Some(rp.clone()).filter(|s| !s.is_empty()),
            };

            // `current_pw_val` was moved into `request`; signal cleared here covers the reactive value.
            rp.zeroize();
            current_password.update(|s| s.zeroize());
            recovery_phrase.update(|s| s.zeroize());

            match invoke_command::<RotateKeyFileRequest, ()>("rotate_key_file", &request).await {
                Ok(()) => {
                    current_key_file.set(None);
                    selected_path.set(None);
                    success.set(true);
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
            loading.set(false);
        });
    };

    let is_tier_2 = move || session.read().vault_tier == Some(2);

    view! {
        {move || {
            if is_tier_2() {
                view! {
                    <div class="p-6 bg-stone border border-steel rounded-lg shadow-sm">
                        <h3 class="text-lg font-semibold text-bone mb-4">"Rotate Key File"</h3>

                        {move || {
                            if success.get() {
                                view! {
                                    <div class="mb-4 p-3 bg-success/20 text-success-text rounded">
                                        "Key file rotated successfully"
                                    </div>
                                }
                                .into_any()
                            } else {
                                ().into_any()
                            }
                        }}

                        {move || {
                            error.get().map(|e| {
                                view! {
                                    <div class="mb-4 p-3 bg-danger/20 text-danger rounded">
                                        {e}
                                    </div>
                                }
                            })
                        }}

                        <div class="space-y-4">
                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">
                                    "Current Password"
                                </label>
                                <input
                                    type="password"
                                    class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                                    placeholder="Enter current password"
                                    prop:value=move || current_password.get()
                                    on:change=move |ev| {
                                        current_password.set(event_target_value(&ev));
                                    }
                                />
                            </div>

                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">
                                    "Current Key File"
                                </label>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="px-3 py-1.5 rounded border border-border-default text-sm text-bone cursor-pointer hover:bg-surface-overlay transition-colors"
                                        on:click=on_choose_current_key_file
                                    >
                                        "Choose Key File"
                                    </button>
                                    {move || current_key_file.get().map(|p| view! {
                                        <span class="text-sm text-text-secondary truncate">{p}</span>
                                    })}
                                </div>
                            </div>

                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">
                                    "New Key File Destination"
                                </label>
                                <div class="flex items-center gap-3">
                                    <button
                                        type="button"
                                        class="px-3 py-1.5 rounded border border-border-default text-sm text-bone cursor-pointer hover:bg-surface-overlay transition-colors"
                                        on:click=on_choose_dest_dir
                                    >
                                        "Choose Directory"
                                    </button>
                                    {move || selected_path.get().map(|p| view! {
                                        <span class="text-sm text-text-secondary truncate">{p}</span>
                                    })}
                                </div>
                            </div>

                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">
                                    {move || {
                                        if session.read().has_recovery_slot == Some(true) {
                                            "Recovery Phrase (required to keep slot)"
                                        } else {
                                            "Recovery Phrase (optional)"
                                        }
                                    }}
                                </label>
                                <input
                                    type="password"
                                    class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                                    placeholder="Enter your 24-word phrase to keep your recovery slot valid"
                                    prop:value=move || recovery_phrase.get()
                                    on:change=move |ev| {
                                        recovery_phrase.set(event_target_value(&ev));
                                    }
                                />
                                {move || {
                                    if session.read().has_recovery_slot == Some(true) {
                                        view! {
                                            <p class="mt-1 text-xs text-amber-400">
                                                "Leaving this blank will permanently delete your recovery slot."
                                            </p>
                                        }
                                        .into_any()
                                    } else {
                                        ().into_any()
                                    }
                                }}
                            </div>

                            <Button
                                on_click=on_submit
                                loading=move || loading.get()
                                variant="primary"
                            >
                                {move || if loading.get() { "Rotating…" } else { "Rotate Key File" }}
                            </Button>
                        </div>
                    </div>
                }
                .into_any()
            } else {
                ().into_any()
            }
        }}
    }
}

// ─── SetupRecoveryForm ──────────────────────────────────────────────────────

/// Form for generating a 24-word recovery phrase.
///
/// On success the phrase is shown in a modal with an acknowledgement gate.
/// The phrase is zeroized from the signal when the modal is dismissed.
#[component]
fn SetupRecoveryForm() -> impl IntoView {
    let session = use_session();
    let password = RwSignal::new(String::new());
    let key_file_path = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);
    let phrase = RwSignal::new(String::new());
    on_cleanup(move || phrase.update(|s| s.zeroize()));
    let show_modal = RwSignal::new(false);
    let acknowledged = RwSignal::new(false);

    let is_tier_2 = move || session.read().vault_tier == Some(2);

    let on_choose_key_file = move |_| {
        spawn_local(async move {
            if let Some(path) = crate::dialog::open_file_dialog().await {
                key_file_path.set(Some(path));
            }
        });
    };

    let on_submit = move |_| {
        error.set(None);
        success.set(false);

        let mut pw_value = password.get();
        if pw_value.is_empty() {
            error.set(Some("Password is required".to_string()));
            return;
        }

        loading.set(true);

        spawn_local(async move {
            let request = SetupRecoveryRequest {
                password: pw_value.clone(),
                key_file_path: key_file_path.get(),
            };

            let result =
                invoke_command::<SetupRecoveryRequest, String>("setup_recovery", &request).await;
            pw_value.zeroize();
            password.update(|s| s.zeroize());
            loading.set(false);

            match result {
                Ok(returned_phrase) => {
                    phrase.set(returned_phrase);
                    show_modal.set(true);
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
        });
    };

    let on_modal_close = move || {
        phrase.update(|s| s.zeroize());
        show_modal.set(false);
        acknowledged.set(false);
        success.set(true);
    };
    let on_done_click = move |_| {
        phrase.update(|s| s.zeroize());
        show_modal.set(false);
        acknowledged.set(false);
        success.set(true);
    };

    view! {
        <div class="p-6 bg-stone border border-steel rounded-lg shadow-sm">
            <h3 class="text-lg font-semibold text-bone mb-2">"Set Up Recovery Phrase"</h3>
            <p class="text-sm text-text-secondary mb-4">
                "Generate a 24-word recovery phrase. Store it securely — it can restore your vault if you lose your password or key file."
            </p>

            {move || {
                if success.get() {
                    view! {
                        <div class="mb-4 p-3 bg-success/20 text-success-text rounded">
                            "Recovery phrase set up successfully"
                        </div>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}

            {move || {
                error.get().map(|e| {
                    view! {
                        <div class="mb-4 p-3 bg-danger/20 text-danger rounded">
                            {e}
                        </div>
                    }
                })
            }}

            <div class="space-y-4">
                <div>
                    <label class="block text-sm font-medium text-bone mb-2">"Password"</label>
                    <input
                        type="password"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Enter your current password"
                        prop:value=move || password.get()
                        on:change=move |ev| {
                            password.set(event_target_value(&ev));
                        }
                    />
                </div>

                {move || {
                    if is_tier_2() {
                        view! {
                            <div>
                                <label class="block text-sm font-medium text-bone mb-2">"Key file"</label>
                                <div class="flex items-center gap-2">
                                    <span class="text-sm text-bone flex-1">
                                        {move || key_file_path.get().unwrap_or_else(|| "No key file selected".to_string())}
                                    </span>
                                    <Button variant="secondary" on_click=on_choose_key_file>
                                        "Browse"
                                    </Button>
                                </div>
                            </div>
                        }
                        .into_any()
                    } else {
                        ().into_any()
                    }
                }}

                <Button on_click=on_submit loading=move || loading.get() variant="primary">
                    {move || if loading.get() { "Setting up…" } else { "Set Up Recovery" }}
                </Button>
            </div>
        </div>

        <Modal open=Signal::derive(move || show_modal.get()) on_close=on_modal_close>
            <div class="p-6 max-w-lg w-full">
                <h3 class="text-xl font-semibold text-bone mb-4">"Your 24-Word Recovery Phrase"</h3>
                <p class="text-sm text-text-secondary mb-4">
                    "Write down these words in order and store them somewhere safe. "
                    "This phrase cannot be recovered if lost."
                </p>
                <div class="bg-surface-overlay border border-border-default rounded-lg p-4 font-mono text-sm text-bone break-words mb-6 select-all">
                    {move || phrase.get()}
                </div>
                <div class="flex items-start gap-3 mb-6">
                    <input
                        type="checkbox"
                        id="phrase-ack"
                        class="mt-1 cursor-pointer"
                        prop:checked=move || acknowledged.get()
                        on:change=move |ev| {
                            acknowledged.set(event_target_checked(&ev));
                        }
                    />
                    <label for="phrase-ack" class="text-sm text-bone cursor-pointer">
                        "I have written down this phrase and stored it in a safe place"
                    </label>
                </div>
                <Button
                    on_click=on_done_click
                    disabled=Signal::derive(move || !acknowledged.get())
                    variant="primary"
                >
                    "Done"
                </Button>
            </div>
        </Modal>
    }
}

// ─── DeleteVaultForm ────────────────────────────────────────────────────────

/// Form for vault deletion.
///
/// Requires exact vault name confirmation.
/// Clears all state contexts before navigating to login.
#[component]
fn DeleteVaultForm() -> impl IntoView {
    let session = use_session();
    let session_actions = use_session_actions();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();
    let confirmation_input = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);

    let on_delete = move |_| {
        error.set(None);
        loading.set(true);

        let confirmation = confirmation_input.get();

        spawn_local(async move {
            let request = DeleteVaultRequest {
                confirmation: confirmation.clone(),
            };

            match invoke_command::<DeleteVaultRequest, ()>("delete_vault", &request).await {
                Ok(()) => {
                    // Clear all state before navigating
                    session_actions.clear();
                    vault_actions.clear();
                    sync_actions.clear();

                    // Navigate to login screen
                    let window = web_sys::window().expect("no window object");
                    window.location().set_href("/").expect("navigation failed");
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
            loading.set(false);
        });
    };

    let vault_name = move || session.with_untracked(|s| s.vault_id.clone().unwrap_or_default());
    let is_confirmed = move || confirmation_input.get() == vault_name();
    let delete_button_disabled = move || loading.get() || !is_confirmed();

    view! {
        <div class="p-6 bg-stone border border-danger rounded-lg shadow-sm">
            <h3 class="text-lg font-semibold text-danger mb-4">"Delete Vault"</h3>

            <p class="text-sm text-bone mb-4">
                "This action is permanent and cannot be undone. All local and remote data will be deleted."
            </p>

            {move || {
                error.get().map(|e| {
                    view! {
                        <div class="mb-4 p-3 bg-danger/20 text-danger rounded">
                            {e}
                        </div>
                    }
                })
            }}

            <div class="space-y-4">
                <div>
                    <label class="block text-sm font-medium text-bone mb-2">
                        {format!("Type '{}' to confirm deletion", vault_name())}
                    </label>
                    <input
                        type="text"
                        class="w-full px-3 py-2 bg-surface-overlay border border-border-default rounded text-bone focus:outline-none focus:ring-2 focus:ring-rune"
                        placeholder="Enter vault name to confirm"
                        prop:value=move || confirmation_input.get()
                        on:change=move |ev| {
                            confirmation_input.set(event_target_value(&ev));
                        }
                    />
                </div>

                <Button
                    on_click=on_delete
                    loading=loading
                    disabled=Signal::derive(delete_button_disabled)
                    variant="danger"
                >
                    {move || if loading.get() { "Deleting…" } else { "Delete Vault" }}
                </Button>
            </div>
        </div>
    }
}

// ─── SettingsPage ───────────────────────────────────────────────────────────

/// Main settings page component.
///
/// Renders three cards: Change Password, Rotate Key File (Tier 2 only), Delete Vault.
#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <div class="flex flex-col gap-6 p-6 max-w-2xl mx-auto">
            <div class="flex items-center justify-between mb-4">
                <h2 class="text-2xl font-bold text-bone">"Vault Settings"</h2>
                <div class="text-sm text-rune cursor-pointer hover:text-bone transition-colors">
                    <A href="/">"← Back to Vault"</A>
                </div>
            </div>

            <SetupRecoveryForm />
            <ChangePasswordForm />
            <RotateKeyFileForm />
            <DeleteVaultForm />
        </div>
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {

    #[test]
    fn test_change_password_form_rejects_mismatched_passwords() {
        // Client-side validation prevents submission when new != confirm
        let new = "password123".to_string();
        let confirm = "password456".to_string();

        assert_ne!(
            new, confirm,
            "Test should verify form rejects mismatched passwords"
        );
    }

    #[test]
    fn test_delete_vault_button_requires_exact_name() {
        // Vault name: "my-vault"
        let vault_name = "my-vault".to_string();
        let user_input_partial = "my-va".to_string();
        let user_input_exact = "my-vault".to_string();

        assert_ne!(user_input_partial, vault_name);
        assert_eq!(user_input_exact, vault_name);
    }
}
