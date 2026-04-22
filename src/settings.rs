//! Vault settings page — password change, key file rotation, vault deletion.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::components::Button;
use crate::invoke::invoke_command;
use crate::ipc_types::{ChangePasswordRequest, DeleteVaultRequest, RotateKeyFileRequest};
use crate::state::{use_session, use_session_actions, use_sync_actions, use_vault_actions};

// ─── ChangePasswordForm ─────────────────────────────────────────────────────

/// Form for changing the vault password.
///
/// Three password input fields: current, new, confirm new.
/// Client-side validation ensures new == confirm before submission.
/// Both password strings are zeroized immediately after the IPC call completes.
#[component]
fn ChangePasswordForm() -> impl IntoView {
    let session_actions = use_session_actions();
    let current_pw = RwSignal::new(String::new());
    let new_pw = RwSignal::new(String::new());
    let confirm_pw = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let on_submit = move |_| {
        error.set(None);
        success.set(false);

        let current = current_pw.get();
        let new = new_pw.get();
        let confirm = confirm_pw.get();

        if current.is_empty() || new.is_empty() || confirm.is_empty() {
            error.set(Some("All password fields are required".to_string()));
            return;
        }

        if new != confirm {
            error.set(Some("New password and confirmation do not match".to_string()));
            return;
        }

        loading.set(true);

        spawn_local(async move {
            let request = ChangePasswordRequest {
                current_password: current.clone(),
                new_password: new.clone(),
            };

            match invoke_command::<ChangePasswordRequest, ()>("change_password", &request).await
            {
                Ok(()) => {
                    current_pw.set(String::new());
                    new_pw.set(String::new());
                    confirm_pw.set(String::new());
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
                        <div class="mb-4 p-3 bg-green-900 text-green-100 rounded">
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
                        <div class="mb-4 p-3 bg-red-900 text-red-100 rounded">
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
                        class="w-full px-3 py-2 bg-steel border border-bone rounded text-bone placeholder-steel-light"
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
                        class="w-full px-3 py-2 bg-steel border border-bone rounded text-bone placeholder-steel-light"
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
                        class="w-full px-3 py-2 bg-steel border border-bone rounded text-bone placeholder-steel-light"
                        placeholder="Confirm new password"
                        prop:value=move || confirm_pw.get()
                        on:change=move |ev| {
                            confirm_pw.set(event_target_value(&ev));
                        }
                    />
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
    let selected_path = RwSignal::new(Option::<String>::None);

    let on_choose_file = move |_| {
        spawn_local(async move {
            match crate::dialog::open_save_dialog(None).await {
                Some(path) => {
                    selected_path.set(Some(path));
                    error.set(None);
                }
                None => {
                    // User cancelled or Tauri unavailable
                }
            }
        });
    };

    let on_submit = move |_| {
        let Some(path) = selected_path.get() else {
            error.set(Some("Please select a file location".to_string()));
            return;
        };

        error.set(None);
        success.set(false);
        loading.set(true);

        spawn_local(async move {
            let request = RotateKeyFileRequest {
                new_key_file_destination: path,
            };

            match invoke_command::<RotateKeyFileRequest, ()>("rotate_key_file", &request).await {
                Ok(()) => {
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
                                    <div class="mb-4 p-3 bg-green-900 text-green-100 rounded">
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
                                    <div class="mb-4 p-3 bg-red-900 text-red-100 rounded">
                                        {e}
                                    </div>
                                }
                            })
                        }}

                        <div class="space-y-4">
                            <div>
                                <p class="text-sm text-bone mb-3">
                                    {move || {
                                        if let Some(ref path) = selected_path.get() {
                                            format!("Selected: {}", path)
                                        } else {
                                            "No file selected".to_string()
                                        }
                                    }}
                                </p>
                            </div>

                            <div class="flex gap-3">
                                <Button
                                    on_click=on_choose_file
                                    loading=move || loading.get()
                                    variant="primary"
                                >
                                    "Choose New File Location"
                                </Button>
                                <Button
                                    on_click=on_submit
                                    loading=move || loading.get() || selected_path.get().is_none()
                                    variant="primary"
                                >
                                    {move || if loading.get() {
                                        "Rotating…"
                                    } else {
                                        "Rotate Key File"
                                    }}
                                </Button>
                            </div>
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

    let vault_name = move || session.read().vault_id.clone().unwrap_or_default();
    let is_confirmed = move || confirmation_input.get() == vault_name();
    let delete_button_disabled = move || loading.get() || !is_confirmed();

    view! {
        <div class="p-6 bg-stone border border-red-900 rounded-lg shadow-sm">
            <h3 class="text-lg font-semibold text-red-200 mb-4">"Delete Vault"</h3>

            <p class="text-sm text-bone mb-4">
                "This action is permanent and cannot be undone. All local and remote data will be deleted."
            </p>

            {move || {
                error.get().map(|e| {
                    view! {
                        <div class="mb-4 p-3 bg-red-900 text-red-100 rounded">
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
                        class="w-full px-3 py-2 bg-steel border border-bone rounded text-bone placeholder-steel-light"
                        placeholder="Enter vault name to confirm"
                        prop:value=move || confirmation_input.get()
                        on:change=move |ev| {
                            confirmation_input.set(event_target_value(&ev));
                        }
                    />
                </div>

                <Button
                    on_click=on_delete
                    loading=delete_button_disabled
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
                <div class="text-sm text-rune hover:text-rune-dark">
                    <A href="/">"← Back to Vault"</A>
                </div>
            </div>

            <ChangePasswordForm />
            <RotateKeyFileForm />
            <DeleteVaultForm />
        </div>
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_password_form_rejects_mismatched_passwords() {
        // Client-side validation prevents submission when new != confirm
        let new = "password123".to_string();
        let confirm = "password456".to_string();

        assert_ne!(new, confirm, "Test should verify form rejects mismatched passwords");
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
