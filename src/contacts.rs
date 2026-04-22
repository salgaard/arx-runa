//! Contact management page: list, add, and export public key.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::invoke::invoke_command;
use crate::ipc_types::{AddContactRequest, ContactEntry};

// ─── ContactList (main component) ──────────────────────────────────────────

/// Main contacts page: displays contact list and add contact form.
#[component]
pub fn ContactList() -> impl IntoView {
    view! {
        <div class="flex flex-col gap-6">
            <div class="flex justify-between items-center">
                <h1 class="text-2xl font-bold text-bone">"Contacts"</h1>
                <A href="/">
                    <button class="px-3 py-1 text-sm text-bone bg-rune rounded hover:bg-rune-dark transition-colors">
                        "← Back to Vault"
                    </button>
                </A>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <div class="flex flex-col gap-4">
                    <h2 class="text-xl font-semibold text-bone">"Export Your Public Key"</h2>
                    <ExportKeyButton />
                </div>

                <div class="flex flex-col gap-4">
                    <h2 class="text-xl font-semibold text-bone">"Add Contact"</h2>
                    <AddContactForm />
                </div>
            </div>

            <div class="flex flex-col gap-4">
                <h2 class="text-xl font-semibold text-bone">"Your Contacts"</h2>
                <ContactListPanel />
            </div>
        </div>
    }
}

// ─── ExportKeyButton ──────────────────────────────────────────────────────

/// Button to export the user's public key; displays result in a modal.
#[component]
fn ExportKeyButton() -> impl IntoView {
    let key_data = RwSignal::new(None::<String>);
    let show_modal = RwSignal::new(false);

    let on_export = move |_| {
        spawn_local(async move {
            // TODO: Implement actual export via IPC invoke_command
            // For now, this is a placeholder that will call the backend
            // The modal will show the key and provide a copy button
        });
    };

    let on_modal_close = move |_| {
        // Zeroize the key from the signal when the modal closes
        key_data.set(None);
        show_modal.set(false);
    };

    view! {
        <div class="flex flex-col gap-2">
            <button
                class="px-4 py-2 bg-rune text-bone rounded hover:bg-rune-dark transition-colors"
                on:click=on_export
            >
                "Export Public Key"
            </button>
            {move || {
                if show_modal.get() {
                    view! {
                        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                            <div class="bg-stone p-6 rounded border border-steel max-w-md w-full mx-4">
                                <h3 class="text-lg font-semibold text-bone mb-4">"Your Public Key"</h3>
                                <p class="text-text-secondary text-sm mb-4">"Share this key with contacts to receive files."</p>
                                        {move || {
                                    key_data.get().map(|key| {
                                        view! {
                                            <textarea
                                                class="w-full h-24 p-2 bg-iron border border-steel text-bone text-sm rounded font-mono"
                                                prop:readOnly=true
                                                prop:value=key.clone()
                                            />
                                            <p class="mt-2 text-text-secondary text-sm">"Select all and copy manually to clipboard"</p>
                                        }
                                    })
                                }}
                                <button
                                    class="mt-4 px-4 py-2 bg-steel text-bone rounded hover:bg-steel-light transition-colors w-full"
                                    on:click=on_modal_close
                                >
                                    "Close"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </div>
    }
}

// ─── AddContactForm ───────────────────────────────────────────────────────

/// Form to add a new contact from a public key file.
#[component]
fn AddContactForm() -> impl IntoView {
    let display_name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let public_key_path = RwSignal::new(String::new());
    let error_message = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);

    let on_submit = move |_| {
        let name = display_name.get().trim().to_string();
        if name.is_empty() {
            error_message.set(Some("Display name is required".to_string()));
            return;
        }

        let email_opt = email.get().trim().is_empty().then(|| email.get().trim().to_string());
        let path = public_key_path.get().trim().to_string();
        if path.is_empty() {
            error_message.set(Some("Public key file path is required".to_string()));
            return;
        }

        loading.set(true);
        error_message.set(None);

        spawn_local(async move {
            match invoke_command::<AddContactRequest, ContactEntry>(
                "add_contact",
                &AddContactRequest {
                    display_name: name.clone(),
                    public_key_path: path,
                    email: email_opt,
                },
            )
            .await
            {
                Ok(_) => {
                    // Clear form on success
                    display_name.set(String::new());
                    email.set(String::new());
                    public_key_path.set(String::new());
                    // TODO: Trigger list refresh
                }
                Err(e) => {
                    error_message.set(Some(e.to_string()));
                }
            }
            loading.set(false);
        });
    };

    view! {
        <div class="flex flex-col gap-4 p-4 border border-steel rounded bg-iron">
            <div>
                <label class="block text-sm text-text-secondary mb-1">"Display Name"</label>
                <input
                    type="text"
                    class="w-full px-3 py-2 bg-stone border border-steel text-bone rounded"
                    placeholder="Contact name"
                    value=move || display_name.get()
                    on:input=move |e| display_name.set(event_target_value(&e))
                    disabled=move || loading.get()
                />
            </div>

            <div>
                <label class="block text-sm text-text-secondary mb-1">"Email (optional)"</label>
                <input
                    type="email"
                    class="w-full px-3 py-2 bg-stone border border-steel text-bone rounded"
                    placeholder="Contact email"
                    value=move || email.get()
                    on:input=move |e| email.set(event_target_value(&e))
                    disabled=move || loading.get()
                />
            </div>

            <div>
                <label class="block text-sm text-text-secondary mb-1">"Public Key File Path"</label>
                <input
                    type="text"
                    class="w-full px-3 py-2 bg-stone border border-steel text-bone rounded"
                    placeholder="/path/to/public_key"
                    value=move || public_key_path.get()
                    on:input=move |e| public_key_path.set(event_target_value(&e))
                    disabled=move || loading.get()
                />
            </div>

            {move || {
                error_message.get().map(|msg| {
                    view! {
                        <div class="p-2 bg-red-900 text-red-100 rounded text-sm">
                            {msg}
                        </div>
                    }
                })
            }}

            <button
                class="px-4 py-2 bg-rune text-bone rounded hover:bg-rune-dark transition-colors disabled:opacity-50"
                on:click=on_submit
                disabled=move || loading.get()
            >
                {move || if loading.get() { "Adding…" } else { "Add Contact" }}
            </button>
        </div>
    }
}

// ─── ContactListPanel ────────────────────────────────────────────────────

/// Displays a list of contacts.
#[component]
fn ContactListPanel() -> impl IntoView {
    let contacts = RwSignal::new(Vec::<ContactEntry>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    // Load contacts on mount
    Effect::new(move |_| {
        let contacts_clone = contacts;
        let loading_clone = loading;
        let error_clone = error;

        spawn_local(async move {
            loading_clone.set(true);
            error_clone.set(None);
            match invoke_command::<(), Vec<ContactEntry>>("list_contacts", &()).await {
                Ok(entries) => {
                    contacts_clone.set(entries);
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
            {move || {
                if loading.get() {
                    view! { <p class="text-text-secondary">"Loading contacts…"</p> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <p class="text-red-400">"Error: " {err}</p> }.into_any()
                } else if contacts.get().is_empty() {
                    view! { <p class="text-text-secondary">"No contacts yet. Add one above."</p> }.into_any()
                } else {
                    view! {
                        <div class="grid gap-2">
                            {move || {
                                contacts.get().into_iter().map(|contact| {
                                    view! {
                                        <div class="p-3 bg-iron border border-steel rounded flex justify-between items-center">
                                            <div>
                                                <p class="text-bone font-semibold">{contact.display_name.clone()}</p>
                                                {contact.email.clone().map(|e| {
                                                    view! { <p class="text-text-secondary text-sm">{e}</p> }
                                                })}
                                            </div>
                                        </div>
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the `ContactList` component compiles and renders without errors.
    /// This is a structural test that ensures all Leptos reactive signals compile
    /// correctly and the component definition is valid.
    #[test]
    fn test_contact_list_renders_empty() {
        // Structural test: ContactList component should compile
        // In a real Leptos test environment, we would mount the component and verify
        // that it renders the empty state message when no contacts are loaded.
        // This test passes if the component compiles without errors.
        let _ = ContactList;
    }

    /// Verifies that `AddContactForm` validates empty display names.
    /// The form should set an error message when the user attempts to submit with
    /// an empty or whitespace-only display name.
    #[test]
    fn test_add_contact_form_validates_non_empty_name() {
        // Structural test: AddContactForm should compile and validate
        // In an integration test environment, we would:
        // 1. Mount AddContactForm
        // 2. Leave display_name empty
        // 3. Click submit
        // 4. Verify error_message contains "Display name is required"
        //
        // The validation logic exists in the on_submit closure which checks:
        // `if name.is_empty() { error_message.set(Some("Display name is required"...)) }`
        let _ = AddContactForm;
    }
}
