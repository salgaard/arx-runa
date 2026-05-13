//! Authentication page components: login and vault creation.
//!
//! All pages interact with `SessionProvider` context via `use_session_actions()`.
//! Password memory is zeroed on both the local `String` and the Leptos signal
//! immediately after each IPC call resolves.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use zeroize::Zeroize;

use crate::components::{Button, ChunkSizeSelector, DestinationSelector, EpochBufferToggle, Input};
use crate::destinations::GdriveShareSetupModal;
use crate::dialog::{open_directory_dialog, open_file_dialog};
use crate::invoke::invoke_command;
use crate::ipc_types::VaultSummary;
use crate::ipc_types::{
    AuthResponse, AuthenticateRequest, CreateVaultRequest, DestinationSessionConfig,
    RecoverVaultFromCloudRequest, SessionStatus, SetGdriveServiceAccountRequest,
};
use crate::state::use_session_actions;

// Re-export chunk-size primitives so existing tests via `use super::*` still compile.
pub use crate::components::{CHUNK_MAX, CHUNK_MIN, PRESETS, clamp_chunk_size};

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
/// The `device-event` subscriber is wired but receives no payloads until Phase 6.5
/// adds the backend emission bridge.
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

/// Login page — presented when the user selects a vault to unlock.
#[component]
pub fn LoginPage(
    /// The vault the user intends to unlock.
    vault: VaultSummary,
    /// Called when the user clicks "Back" to return to the vault picker.
    on_back: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    let session_actions = use_session_actions();
    let vault_id = vault.vault_id.clone();
    let vault_tier = vault.tier;
    let vault_display_name = vault
        .name
        .clone()
        .unwrap_or_else(|| format!("{}…", &vault.vault_id[..8]));

    let on_submit = {
        let vault_id = vault_id.clone();
        move |_| {
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
            let vault_id = vault_id.clone();

            leptos::task::spawn_local(async move {
                let vault_id_check = vault_id.clone();
                let result = invoke_command::<AuthenticateRequest, AuthResponse>(
                    "authenticate",
                    &AuthenticateRequest {
                        password: password_value.clone(),
                        key_file_path: key_file,
                        vault_id: Some(vault_id),
                    },
                )
                .await;
                password_value.zeroize();
                set_password.update(|s| s.zeroize());
                set_loading.set(false);
                match result {
                    Ok(resp) => {
                        crate::components::use_toast()
                            .success(format!("Vault unlocked: {}", resp.vault_id));
                        session_actions.complete_success(resp.vault_id);
                    }
                    Err(err) => {
                        // `SessionAlreadyActive` surfaces as `invalidInput`. This can
                        // happen on hot reload: the frontend WASM resets but the Tauri
                        // backend session stays alive. Check the actual backend state
                        // and sync the frontend rather than surfacing a cryptic error.
                        if err.kind == "invalidInput"
                            && let Ok(status) =
                                invoke_command::<(), SessionStatus>("get_session_status", &()).await
                            && status.is_unlocked
                        {
                            if status.vault_id.as_deref() == Some(&vault_id_check) {
                                // Same vault — backend is already unlocked; sync frontend.
                                crate::components::use_toast()
                                    .success(format!("Vault unlocked: {}", vault_id_check));
                                session_actions.complete_success(vault_id_check);
                                return;
                            } else {
                                // A different vault is already unlocked.
                                let message =
                                    "Another vault is already unlocked. Lock it first.".to_string();
                                crate::components::use_toast().error(&message);
                                session_actions.complete_failure(message);
                                return;
                            }
                        }
                        crate::components::use_toast().error(&err.message);
                        session_actions.complete_failure(err.message);
                    }
                }
            });
        }
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-md bg-stone border border-steel rounded-xl p-6 shadow-xl">
                <h1 class="text-2xl text-bone text-center mb-2">
                    {"Unlock "}{vault_display_name}
                </h1>
                {move || if vault_tier == 2 {
                    view! {
                        <p class="text-sm text-text-secondary text-center mb-6">
                            "Key file required"
                        </p>
                    }.into_any()
                } else {
                    view! { <div class="mb-6"></div> }.into_any()
                }}
                <Input
                    input_type="password"
                    label="Password".to_string()
                    value=password
                    on_input=move |v| set_password.set(v)
                />
                {move || if vault_tier == 2 {
                    view! {
                        <KeyFileIndicator
                            detected_path=key_file_path
                            on_manual_select=move |p| set_key_file_path.set(Some(p))
                        />
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <Button loading=loading on_click=on_submit>"Unlock"</Button>
                <button
                    class="mt-4 text-rune text-sm w-full text-center cursor-pointer hover:text-bone transition-colors"
                    on:click=move |_| on_back()
                >
                    "← Back"
                </button>
            </div>
        </div>
    }
}

// ─── Vault creation validation ────────────────────────────────────────────────

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

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn format_chunk_size(bytes: u64) -> String {
    if let Some((label, _)) = PRESETS.iter().find(|(_, b)| *b == bytes) {
        return (*label).to_string();
    }
    if bytes >= 1_048_576 {
        format!("{} MiB", bytes / 1_048_576)
    } else {
        format!("{} KiB", bytes / 1_024)
    }
}

fn default_local_destination() -> DestinationSessionConfig {
    DestinationSessionConfig {
        label: "Local Filesystem".to_string(),
        destination_type: "local_path".to_string(),
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

// ─── VaultCreationPage ────────────────────────────────────────────────────────

/// Vault creation page — single-page sectioned form.
///
/// Sections: Identity → Advanced (collapsible) → Destination → Review.
/// The "Cancel" button calls `on_back_to_login`.
#[component]
pub fn VaultCreationPage(
    /// Called when the user cancels vault creation.
    on_back_to_login: impl Fn() + 'static + Clone,
) -> impl IntoView {
    // ── Identity signals ──────────────────────────────────────────────────────
    let (vault_name, set_vault_name) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (tier, set_tier) = signal::<u8>(2);
    let (key_file_destination, set_key_file_destination) = signal::<Option<String>>(None);

    // ── Advanced signals ──────────────────────────────────────────────────────
    let (show_advanced, set_show_advanced) = signal(false);
    let (chunk_size_bytes, set_chunk_size_bytes) = signal::<u64>(4_194_304);
    let (epoch_buffer_enabled, set_epoch_buffer_enabled) = signal(false);

    // ── Destination signal (updated by DestinationSelector via callback) ──────
    let (primary_destination, set_primary_destination) = signal(default_local_destination());
    let is_gdrive_selected = move || {
        primary_destination
            .read()
            .rclone_config_blob
            .contains("type = drive")
    };

    // ── Google Drive sharing (deferred until after vault creation) ────────────
    let (pending_sa_path, set_pending_sa_path) = signal::<Option<String>>(None);
    let show_gdrive_sharing_modal = RwSignal::new(false);

    // ── Submit state ──────────────────────────────────────────────────────────
    let (loading, set_loading) = signal(false);

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
            primary_destination: primary_destination.get(),
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
                    if let Some(sa_path) = pending_sa_path.get_untracked() {
                        if let Err(e) = invoke_command::<_, ()>(
                            "set_gdrive_service_account",
                            &SetGdriveServiceAccountRequest {
                                sa_json_path: sa_path,
                            },
                        )
                        .await
                        {
                            crate::components::use_toast()
                                .warning(format!("Vault created, but sharing setup failed: {e}"));
                        }
                    }
                    crate::components::use_toast().success(format!(
                        "Vault '{}' created successfully!",
                        vault_name_value
                    ));
                    session_actions.complete_success(resp.vault_id);
                }
                Err(err) => {
                    crate::components::use_toast().error(&err.message);
                }
            }
        });
    };

    let section_header = |title: &'static str| {
        view! {
            <h2 class="text-xs font-semibold uppercase tracking-widest text-text-secondary border-b border-border-default pb-2 mb-4">
                {title}
            </h2>
        }
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-lg bg-stone border border-steel rounded-xl shadow-xl overflow-hidden">
                <div class="p-6 overflow-y-auto max-h-screen">
                    <h1 class="text-2xl text-bone text-center mb-8">"Create New Vault"</h1>

                    // ── Identity Section ──────────────────────────────────────────────
                    <div class="mb-8">
                        {section_header("Identity")}
                        <Input
                            label="Vault Name".to_string()
                            value=vault_name
                            on_input=move |v| set_vault_name.set(v)
                        />
                        <Input
                            input_type="password"
                            label="Password".to_string()
                            value=password
                            on_input=move |v| set_password.set(v)
                        />

                        <div class="mb-4">
                            <label class="text-sm text-text-secondary block mb-1">
                                "Authentication Tier"
                            </label>
                            <select
                                class="bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone w-full cursor-pointer focus:outline-none focus:border-rune"
                                on:change=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    let v: u8 = event_target_value(&ev).parse().unwrap_or(2);
                                    set_tier.set(v);
                                }
                            >
                                <option value="1">"Tier 1 — Password only"</option>
                                <option value="2" selected=true>"Tier 2 — Password + Key file"</option>
                            </select>
                        </div>

                        <Show when=move || tier.get() == 2>
                            <div class="mb-4">
                                <label class="text-sm text-text-secondary block mb-1">
                                    "Key File Destination"
                                </label>
                                <div class="flex gap-2 items-center">
                                    <span class="text-sm text-bone flex-1">
                                        {move || key_file_destination.get()
                                            .unwrap_or_else(|| "Not selected".to_string())}
                                    </span>
                                    <Button variant="secondary" on_click=move |ev| on_browse_key(ev)>
                                        "Browse"
                                    </Button>
                                </div>
                            </div>
                        </Show>
                    </div>

                    // ── Advanced Section (collapsible) ────────────────────────────────
                    <div class="mb-8">
                        <button
                            type="button"
                            on:click=move |_| set_show_advanced.update(|v| *v = !*v)
                            class="flex items-center gap-2 text-xs font-semibold uppercase tracking-widest text-text-secondary border-b border-border-default pb-2 mb-4 w-full text-left cursor-pointer hover:text-bone transition-colors"
                        >
                            <span class="transition-transform duration-200"
                                  class=("rotate-90", move || show_advanced.get())>
                                "▶"
                            </span>
                            "Advanced"
                        </button>

                        <Show when=move || show_advanced.get()>
                            <div class="space-y-6 pl-2">
                                <div>
                                    <label class="text-sm text-text-secondary block mb-2">
                                        "Chunk Size"
                                    </label>
                                    <ChunkSizeSelector
                                        value=chunk_size_bytes
                                        set_value=set_chunk_size_bytes
                                    />
                                </div>
                                <EpochBufferToggle
                                    enabled=epoch_buffer_enabled
                                    set_enabled=set_epoch_buffer_enabled
                                />
                            </div>
                        </Show>
                    </div>

                    // ── Destination Section ───────────────────────────────────────────
                    <div class="mb-8">
                        {section_header("Storage Destination")}
                        <DestinationSelector
                            on_change=move |config| set_primary_destination.set(config)
                        />
                        <Show when=is_gdrive_selected>
                            <div class="mt-3 p-3 border border-steel/60 rounded bg-stone/40 text-sm">
                                <p class="font-medium text-bone mb-1">"File sharing (optional)"</p>
                                <p class="text-text-secondary mb-2">
                                    "To share files from this vault, you need a GCP Service Account key. "
                                    "You can set this up now or later from the Destinations page."
                                </p>
                                {move || {
                                    if pending_sa_path.get().is_some() {
                                        view! {
                                            <p class="text-green-400 text-sm">"✓ Service account key selected"</p>
                                        }
                                        .into_any()
                                    } else {
                                        view! {
                                            <button
                                                class="text-rune hover:text-rune/80 underline text-sm cursor-pointer"
                                                on:click=move |_| show_gdrive_sharing_modal.set(true)
                                            >
                                                "Set up sharing →"
                                            </button>
                                        }
                                        .into_any()
                                    }
                                }}
                            </div>
                        </Show>
                    </div>

                    // ── Review Section ────────────────────────────────────────────────
                    <div class="mb-8">
                        {section_header("Review")}
                        <div class="bg-surface-overlay border border-border-default rounded-lg p-4 space-y-2 text-sm">
                            <div class="flex justify-between">
                                <span class="text-text-secondary">"Vault name:"</span>
                                <span class="text-bone font-medium">
                                    {move || {
                                        let n = vault_name.get();
                                        if n.is_empty() { "—".to_string() } else { n }
                                    }}
                                </span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-text-secondary">"Authentication:"</span>
                                <span class="text-bone font-medium">
                                    {move || if tier.get() == 2 {
                                        "Tier 2 (Password + Key file)"
                                    } else {
                                        "Tier 1 (Password only)"
                                    }}
                                </span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-text-secondary">"Chunk size:"</span>
                                <span class="text-bone font-medium">
                                    {move || format_chunk_size(chunk_size_bytes.get())}
                                </span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-text-secondary">"Small-file packing:"</span>
                                <span class="text-bone font-medium">
                                    {move || if epoch_buffer_enabled.get() { "Enabled" } else { "Disabled" }}
                                </span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-text-secondary">"Storage:"</span>
                                <span class="text-bone font-medium">
                                    {move || primary_destination.read().label.clone()}
                                </span>
                            </div>
                        </div>
                    </div>

                    // ── Buttons ───────────────────────────────────────────────────────
                    <div class="flex gap-3">
                        <button
                            type="button"
                            class="flex-1 px-4 py-2 rounded-lg border border-border-default text-bone cursor-pointer hover:bg-surface-overlay transition-colors"
                            on:click=move |_| on_back()
                        >
                            "← Back"
                        </button>
                        <Button loading=loading on_click=on_submit>
                            "Create Vault"
                        </Button>
                    </div>
                </div>
            </div>
            {move || {
                if show_gdrive_sharing_modal.get() {
                    let on_file_picked = move |path: String| {
                        set_pending_sa_path.set(Some(path));
                    };
                    let on_close = move || show_gdrive_sharing_modal.set(false);
                    view! { <GdriveShareSetupModal on_file_picked=on_file_picked on_close=on_close /> }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

// ─── VaultRecoveryPage ────────────────────────────────────────────────────────

/// Recovery page — downloads an existing cloud vault and imports it onto this device.
///
/// Sections: Cloud Destination → Credentials (password + optional key file).
/// Calls `recover_vault_from_cloud` (no active session required). On success,
/// the session transitions to the unlocked state via `session_actions.complete_success`.
#[component]
pub fn VaultRecoveryPage(
    /// Called when the user clicks "← Back" to return to the vault picker.
    on_back: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (primary_destination, set_primary_destination) = signal(default_local_destination());
    let (loading, set_loading) = signal(false);

    let session_actions = use_session_actions();
    let on_back_cancel = on_back.clone();

    let on_submit = move |_| {
        let mut password_value = password.get();
        if password_value.is_empty() {
            crate::components::use_toast().warning("Password is required");
            return;
        }

        set_loading.set(true);
        let session_actions = session_actions;
        let set_loading = set_loading;
        let set_password = set_password;

        let req = RecoverVaultFromCloudRequest {
            password: password_value.clone(),
            key_file_path: key_file_path.get(),
            primary_destination: primary_destination.get(),
        };

        leptos::task::spawn_local(async move {
            let result = invoke_command::<RecoverVaultFromCloudRequest, AuthResponse>(
                "recover_vault_from_cloud",
                &req,
            )
            .await;
            password_value.zeroize();
            set_password.update(|s| s.zeroize());
            set_loading.set(false);
            match result {
                Ok(resp) => {
                    crate::components::use_toast().success("Vault recovered successfully");
                    session_actions.complete_success(resp.vault_id);
                }
                Err(err) => {
                    crate::components::use_toast().error(&err.message);
                }
            }
        });
    };

    let section_header = |title: &'static str| {
        view! {
            <h2 class="text-xs font-semibold uppercase tracking-widest text-text-secondary border-b border-border-default pb-2 mb-4">
                {title}
            </h2>
        }
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-lg bg-stone border border-steel rounded-xl shadow-xl overflow-hidden">
                <div class="p-6 overflow-y-auto max-h-screen">
                    <h1 class="text-2xl text-bone text-center mb-8">"Recover Vault from Cloud"</h1>

                    <div class="mb-8">
                        {section_header("Cloud Destination")}
                        <DestinationSelector on_change=move |cfg| set_primary_destination.set(cfg) />
                    </div>

                    <div class="mb-8">
                        {section_header("Credentials")}
                        <Input
                            input_type="password"
                            label="Password".to_string()
                            value=password
                            on_input=move |v| set_password.set(v)
                        />
                        <div>
                            <label class="text-sm text-text-secondary block mb-1">
                                "Key file (leave empty for Tier 1 / password-only vaults)"
                            </label>
                            <KeyFileIndicator
                                detected_path=key_file_path
                                on_manual_select=move |path| set_key_file_path.set(Some(path))
                            />
                        </div>
                    </div>

                    <div class="flex gap-3">
                        <button
                            type="button"
                            class="flex-1 px-4 py-2 rounded-lg border border-border-default text-bone cursor-pointer hover:bg-surface-overlay transition-colors"
                            on:click=move |_| on_back_cancel()
                        >
                            "\u{2190} Back"
                        </button>
                        <Button loading=loading on_click=on_submit>
                            "Recover Vault"
                        </Button>
                    </div>
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
        assert_eq!(clamp_chunk_size(4_194_304), 4_194_304);
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

    #[test]
    fn test_format_chunk_size_presets_match_labels() {
        assert_eq!(format_chunk_size(4_194_304), "Standard (4 MiB)");
        assert_eq!(format_chunk_size(524_288), "Documents (512 KiB)");
    }

    #[test]
    fn test_format_chunk_size_custom_mib() {
        assert_eq!(format_chunk_size(8_388_608), "8 MiB");
    }

    #[test]
    fn test_format_chunk_size_custom_kib() {
        assert_eq!(format_chunk_size(262_144), "256 KiB");
    }
}
