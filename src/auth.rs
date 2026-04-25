//! Authentication page components: login, vault creation, and key-file selector.
//!
//! All pages in this module interact with the `SessionProvider` context via
//! `use_session_actions()`. Password memory is zeroed on both the local `String`
//! and the Leptos signal immediately after each IPC call resolves.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use zeroize::Zeroize;

use crate::components::{Button, Input, StorageProvider};
use crate::dialog::{open_directory_dialog, open_file_dialog};
use crate::invoke::invoke_command;
use crate::ipc_types::{
    AuthResponse, AuthenticateRequest, CreateVaultRequest, DestinationSessionConfig,
};
use crate::state::{use_session, use_session_actions};
use crate::auth_wizard::{WizardStep, ProgressIndicator, IdentityStep, StorageStep, ReviewStep};

// ─── Chunk-size constants (VaultCreationPage helpers) ─────────────────────────

/// Minimum allowed chunk size in bytes.
pub const CHUNK_MIN: u64 = 131_072;

/// Maximum allowed chunk size in bytes.
pub const CHUNK_MAX: u64 = 67_108_864;

/// Named chunk size presets shown in the UI.
pub const PRESETS: &[(&str, u64)] = &[
    ("Documents (512 KiB)", 524_288),
    ("Standard (4 MiB)", 4_194_304),
    ("Media (16 MiB)", 16_777_216),
    ("Paranoid (64 MiB)", 67_108_864),
];

/// Clamps `bytes` to `[CHUNK_MIN, CHUNK_MAX]`.
///
/// Server-side `validate_chunk_size` is the final authority; this clamp is
/// best-effort client-side protection against accidental out-of-range values.
pub fn clamp_chunk_size(bytes: u64) -> u64 {
    bytes.clamp(CHUNK_MIN, CHUNK_MAX)
}

/// Converts storage provider selection to DestinationSessionConfig for IPC.
fn storage_provider_to_destination(provider: StorageProvider) -> DestinationSessionConfig {
    match provider {
        StorageProvider::Local => {
            DestinationSessionConfig {
                label: "Local".to_string(),
                destination_type: "local".to_string(),
                provider: "local".to_string(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: String::new(),
                rclone_config_blob: String::new(),
                is_primary: true,
                backup_mode: None,
            }
        }
        StorageProvider::S3 => {
            DestinationSessionConfig {
                label: "Amazon S3".to_string(),
                destination_type: "s3".to_string(),
                provider: "aws".to_string(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: String::new(),
                rclone_config_blob: String::new(),
                is_primary: true,
                backup_mode: None,
            }
        }
        StorageProvider::B2 => {
            DestinationSessionConfig {
                label: "Backblaze B2".to_string(),
                destination_type: "b2".to_string(),
                provider: "b2".to_string(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: String::new(),
                rclone_config_blob: String::new(),
                is_primary: true,
                backup_mode: None,
            }
        }
        StorageProvider::Rclone => {
            DestinationSessionConfig {
                label: "Rclone".to_string(),
                destination_type: "rclone".to_string(),
                provider: "rclone".to_string(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: String::new(),
                rclone_config_blob: String::new(),
                is_primary: true,
                backup_mode: None,
            }
        }
    }
}

/// Returns the default primary destination used when no cloud backend is configured.
fn default_destination() -> DestinationSessionConfig {
    DestinationSessionConfig {
        label: "Local".to_string(),
        destination_type: "local".to_string(),
        provider: "local".to_string(),
        bucket: String::new(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: String::new(),
        rclone_config_blob: String::new(),
        is_primary: true,
        backup_mode: None,
    }
}

// ─── KeyFileIndicator ─────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    /// Subscribes to Tauri events via `window.__TAURI__.event.listen`.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

/// Checks if Tauri event API is available.
fn is_tauri_event_available() -> bool {
    use wasm_bindgen::JsValue;
    match web_sys::window() {
        Some(window) => {
            let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"));
            if let Ok(tauri_obj) = tauri
                && !tauri_obj.is_undefined()
                && !tauri_obj.is_null()
            {
                let event_api = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("event"));
                return event_api.is_ok() && !event_api.unwrap().is_undefined();
            }
            false
        }
        None => false,
    }
}

/// Key-file selector indicator.
///
/// Displays the currently detected or manually-selected key file path.
/// In Phase 6.3, only manual selection is active; the `device-event` subscriber
/// is wired but receives no payloads until Phase 6.5 adds the backend emission
/// bridge (`AppHandle::emit("device-event")` from `DeviceMonitor`).
#[component]
pub fn KeyFileIndicator(
    /// Currently detected or manually-selected key file path signal.
    detected_path: ReadSignal<Option<String>>,
    /// Called with the chosen path when the user selects a file manually.
    on_manual_select: impl Fn(String) + 'static + Clone,
) -> impl IntoView {
    let on_select = on_manual_select.clone();

    let on_browse = move |_| {
        let on_select = on_select.clone();
        leptos::task::spawn_local(async move {
            if let Some(path) = open_file_dialog().await {
                on_select(path);
            }
        });
    };

    // Wire the device-event subscriber — tolerates zero emissions in 6.3.
    // Backend emission bridge (AppHandle::emit from DeviceMonitor) is Phase 6.5 work.
    //
    // Skip subscription in browser dev mode when Tauri event API is unavailable.
    // `on_cleanup` requires `Send + Sync`, so the unlisten handle is stored in
    // `Arc<Mutex<...>>` (both `Send + Sync`). The `Closure` is forgotten after
    // registration — Tauri's JS side holds the reference; once `unlisten` is
    // called in `on_cleanup`, Tauri drops the reference and the JS GC reclaims
    // the closure backing data.
    Effect::new(move |_| {
        if !is_tauri_event_available() {
            return;
        }

        let on_device = on_manual_select.clone();
        let unlisten_fn: Arc<Mutex<Option<js_sys::Function>>> = Arc::new(Mutex::new(None));
        let unlisten_for_cleanup = Arc::clone(&unlisten_fn);

        leptos::task::spawn_local(async move {
            let event_closure = Closure::wrap(Box::new(move |payload: JsValue| {
                let path = js_sys::Reflect::get(&payload, &JsValue::from_str("path"))
                    .ok()
                    .and_then(|v| v.as_string());
                if let Some(p) = path {
                    on_device(p);
                }
            }) as Box<dyn Fn(JsValue)>);
            if let Ok(unlisten_val) =
                listen("device-event", event_closure.as_ref().unchecked_ref()).await
                && let Ok(f) = unlisten_val.dyn_into::<js_sys::Function>()
            {
                *unlisten_fn.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
            }
            // Tauri holds the JS reference until unlisten() is called.
            event_closure.forget();
        });

        on_cleanup(move || {
            if let Ok(mut guard) = unlisten_for_cleanup.lock()
                && let Some(f) = guard.take()
            {
                let _ = f.call0(&JsValue::undefined());
            }
        });
    });

    view! {
        <div class="flex items-center gap-2 mb-4">
            <span class="text-sm text-text-secondary flex-1">
                {move || detected_path.read().clone().unwrap_or_else(|| "No key file selected".to_string())}
            </span>
            <Button variant="secondary" on_click=on_browse>"Browse"</Button>
        </div>
    }
}

// ─── LoginPage ────────────────────────────────────────────────────────────────

/// Login page — presented when no vault session is active.
///
/// The "Create new vault" link calls `on_request_create_vault` to trigger the
/// routing transition to `VaultCreationPage`.
#[component]
pub fn LoginPage(
    /// Called when the user clicks "Create new vault".
    on_request_create_vault: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    let session_actions = use_session_actions();
    let on_create = on_request_create_vault.clone();

    let on_submit = move |_| {
        let mut password_value = password.get();
        let key_file = key_file_path.get();

        if password_value.is_empty() {
            crate::components::use_toast().warning("Password is required");
            return;
        }

        session_actions.begin_authenticating();
        set_loading.set(true);

        let session_actions = session_actions;
        let set_loading = set_loading;
        let set_password = set_password;

        leptos::task::spawn_local(async move {
            let result = invoke_command::<AuthenticateRequest, AuthResponse>(
                "authenticate",
                &AuthenticateRequest {
                    password: password_value.clone(),
                    key_file_path: key_file,
                },
            )
            .await;
            password_value.zeroize();
            set_password.update(|s| s.zeroize());
            set_loading.set(false);
            match result {
                Ok(resp) => {
                    crate::components::use_toast().success(&format!("Vault unlocked: {}", resp.vault_id));
                    session_actions.complete_success(resp.vault_id);
                }
                Err(err) => {
                    crate::components::use_toast().error(&err.message);
                    session_actions.complete_failure(err.message);
                }
            }
        });
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-md bg-stone border border-steel rounded-xl p-6 shadow-xl">
                <h1 class="text-2xl text-bone text-center mb-6">"Unlock Vault"</h1>
                <Input
                    input_type="password"
                    label="Password".to_string()
                    value=password
                    on_input=move |v| set_password.set(v)
                />
                <KeyFileIndicator
                    detected_path=key_file_path
                    on_manual_select=move |p| set_key_file_path.set(Some(p))
                />
                <Button loading=loading on_click=on_submit>"Unlock"</Button>
                <button
                    class="mt-4 text-rune text-sm w-full text-center"
                    on:click=move |_| on_create()
                >
                    "Create new vault"
                </button>
            </div>
        </div>
    }
}

// ─── Vault creation form validation ──────────────────────────────────────────

/// Validates vault creation form inputs before the IPC call is dispatched.
///
/// Returns `Ok(())` when all fields are valid. Returns `Err(message)` with a
/// user-displayable description for the first failing check.
///
/// Validations:
/// - `vault_name` must not be empty
/// - `password` must not be empty
/// - `tier == 2` requires a `key_file_destination`
pub fn validate_vault_creation_form(
    vault_name: &str,
    password: &str,
    tier: u8,
    key_file_destination: Option<&str>,
) -> Result<(), String> {
    if vault_name.is_empty() {
        return Err("Vault name is required".into());
    }
    if password.is_empty() {
        return Err("Password is required".into());
    }
    if tier == 2 && key_file_destination.is_none() {
        return Err("A key file destination is required for Tier 2 vaults".into());
    }
    Ok(())
}

// ─── VaultCreationPage ────────────────────────────────────────────────────────

/// Vault creation page — guides the user through naming, securing, and
/// configuring a new vault.
///
/// The "Back to login" link calls `on_back_to_login` to return to `LoginPage`.
#[component]
pub fn VaultCreationPage(
    /// Called when the user clicks "Back to login".
    on_back_to_login: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (vault_name, set_vault_name) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (tier, set_tier) = signal::<u8>(2);
    let (key_file_destination, set_key_file_destination) = signal::<Option<String>>(None);
    let (chunk_size_bytes, _set_chunk_size_bytes) = signal::<u64>(4_194_304);
    let (epoch_buffer_enabled, _set_epoch_buffer_enabled) = signal(false);
    let (storage_provider, set_storage_provider) = signal(StorageProvider::Local);
    let (loading, set_loading) = signal(false);
    let (current_step, set_current_step) = signal(WizardStep::Identity);
    
    let session = use_session();
    let session_actions = use_session_actions();
    let on_back = on_back_to_login.clone();

    let on_browse_key = move |_| {
        leptos::task::spawn_local(async move {
            if let Some(path) = open_directory_dialog().await {
                set_key_file_destination.set(Some(path));
                crate::components::use_toast().info("Key file destination selected");
            }
        });
    };

    let can_proceed = move || {
        match current_step.get() {
            WizardStep::Identity => {
                !vault_name.get().is_empty() && !password.get().is_empty() && 
                (tier.get() == 1 || key_file_destination.get().is_some())
            }
            WizardStep::Storage => true,
            WizardStep::Review => true,
        }
    };

    let on_next = move |_| {
        if !can_proceed() {
            crate::components::use_toast().warning("Please complete all required fields");
            return;
        }
        
        match current_step.get() {
            WizardStep::Identity => set_current_step.set(WizardStep::Storage),
            WizardStep::Storage => set_current_step.set(WizardStep::Review),
            WizardStep::Review => {},
        }
    };

    let on_back_step = move |_| {
        match current_step.get() {
            WizardStep::Identity => on_back(),
            WizardStep::Storage => set_current_step.set(WizardStep::Identity),
            WizardStep::Review => set_current_step.set(WizardStep::Storage),
        }
    };

    let on_submit = move |_| {
        let mut password_value = password.get();
        let vault_name_value = vault_name.get();
        let tier_value = tier.get();
        let key_file_destination_value = key_file_destination.get();

        if let Err(message) = validate_vault_creation_form(
            &vault_name_value,
            &password_value,
            tier_value,
            key_file_destination_value.as_deref(),
        ) {
            crate::components::use_toast().warning(&message);
            return;
        }

        let clamped_chunk = clamp_chunk_size(chunk_size_bytes.get());
        set_loading.set(true);

        let session_actions = session_actions;
        let set_loading = set_loading;
        let set_password = set_password;

        let req = CreateVaultRequest {
            vault_name: vault_name_value.clone(),
            password: password_value.clone(),
            tier: tier_value,
            key_file_destination: key_file_destination_value,
            primary_destination: storage_provider_to_destination(storage_provider.get()),
            chunk_size_bytes: clamped_chunk,
            epoch_buffer_enabled: epoch_buffer_enabled.get(),
        };

        leptos::task::spawn_local(async move {
            let result =
                invoke_command::<CreateVaultRequest, AuthResponse>("create_vault", &req).await;
            password_value.zeroize();
            set_password.update(|s| s.zeroize());
            set_loading.set(false);
            match result {
                Ok(resp) => {
                    crate::components::use_toast().success(&format!("Vault '{}' created successfully!", vault_name_value));
                    session_actions.complete_success(resp.vault_id);
                }
                Err(err) => {
                    crate::components::use_toast().error(&err.message);
                    session_actions.complete_failure(err.message);
                }
            }
        });
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-lg bg-stone border border-steel rounded-xl p-6 shadow-xl">
                <ProgressIndicator current_step=current_step.get() />
                
                <h1 class="text-2xl text-bone text-center mb-2">{current_step.get().title()}</h1>
                <p class="text-sm text-text-secondary text-center mb-6">{current_step.get().description()}</p>

                {
                    move || match current_step.get() {
                        WizardStep::Identity => view! {
                            <IdentityStep
                                vault_name=vault_name
                                set_vault_name=set_vault_name
                                password=password
                                set_password=set_password
                                tier=tier
                                set_tier=set_tier
                                key_file_destination=key_file_destination
                                on_browse_key=on_browse_key
                            />
                        }.into_any(),
                        WizardStep::Storage => view! {
                            <StorageStep
                                _storage_provider=storage_provider
                                _set_storage_provider=set_storage_provider
                            />
                        }.into_any(),
                        WizardStep::Review => view! {
                            <ReviewStep
                                vault_name=vault_name
                                tier=tier
                                key_file_destination=key_file_destination
                                storage_provider=storage_provider
                            />
                        }.into_any(),
                    }
                }

                {move || session.read().error.clone().map(|e| view! {
                    <p class="text-danger text-sm mt-4 mb-4">{e}</p>
                })}

                <div class="flex gap-3 mt-8">
                    <button
                        class="flex-1 px-4 py-2 rounded-lg border border-border-default text-bone hover:bg-surface-overlay transition-colors"
                        on:click=on_back_step
                    >
                        {if current_step.get() == WizardStep::Identity { "Cancel" } else { "Back" }}
                    </button>
                    
                    <Show when=move || current_step.get() != WizardStep::Review>
                        <Button loading=Signal::derive(move || false) on_click=on_next>
                            "Next"
                        </Button>
                    </Show>

                    <Show when=move || current_step.get() == WizardStep::Review>
                        <Button loading=loading on_click=on_submit>
                            "Create Vault"
                        </Button>
                    </Show>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_chunk_size_below_min_returns_min() {
        assert_eq!(clamp_chunk_size(0), CHUNK_MIN);
        assert_eq!(clamp_chunk_size(1), CHUNK_MIN);
        assert_eq!(clamp_chunk_size(131_071), CHUNK_MIN);
    }

    #[test]
    fn test_clamp_chunk_size_above_max_returns_max() {
        assert_eq!(clamp_chunk_size(67_108_865), CHUNK_MAX);
        assert_eq!(clamp_chunk_size(u64::MAX), CHUNK_MAX);
    }

    #[test]
    fn test_clamp_chunk_size_default_preset_unchanged() {
        // Standard preset (4 MiB) is within bounds — must pass through unchanged.
        assert_eq!(clamp_chunk_size(4_194_304), 4_194_304);
        // All four presets must survive clamping.
        for (_, bytes) in PRESETS {
            assert_eq!(clamp_chunk_size(*bytes), *bytes);
        }
    }

    #[test]
    fn test_validate_vault_creation_form_empty_name_returns_error() {
        assert!(validate_vault_creation_form("", "password", 1, None).is_err());
    }

    #[test]
    fn test_validate_vault_creation_form_empty_password_returns_error() {
        assert!(validate_vault_creation_form("MyVault", "", 1, None).is_err());
    }

    #[test]
    fn test_validate_vault_creation_form_tier2_no_key_file_returns_error() {
        assert!(validate_vault_creation_form("MyVault", "password", 2, None).is_err());
    }

    #[test]
    fn test_validate_vault_creation_form_tier2_with_key_file_returns_ok() {
        assert!(
            validate_vault_creation_form("MyVault", "password", 2, Some("/path/to/key")).is_ok()
        );
    }

    #[test]
    fn test_validate_vault_creation_form_tier1_no_key_file_returns_ok() {
        assert!(validate_vault_creation_form("MyVault", "password", 1, None).is_ok());
    }
}
