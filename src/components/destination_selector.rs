//! Destination selector — flat six-option model with OAuth flows for OneDrive
//! and Google Drive.
//!
//! The component calls `on_change` with a completed `DestinationSessionConfig`
//! whenever the user finishes configuring a destination.  For OAuth providers
//! (OneDrive, Google Drive) `on_change` fires only after the OAuth callback
//! has been received and the rclone config blob is available.

use std::time::Duration;

use gloo_timers::future::sleep;
use leptos::prelude::*;

use crate::dialog::open_directory_dialog;
use crate::invoke::invoke_command;
use crate::ipc_types::{
    BeginOauthSetupResponse, CancelOauthSetupRequest, DestinationSessionConfig, OauthPollResponse,
    PollOauthSetupRequest,
};

// ─── Destination kind ────────────────────────────────────────────────────────

/// Flat destination variant — replaces the old two-level DestinationType / CloudProvider model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationKind {
    /// Local filesystem on this machine.
    Local,
    /// External drive or network share reached by a local path.
    ExternalDrive,
    /// Backblaze B2 object storage (key-ID + app-key credentials).
    BackblazeB2,
    /// Personal Microsoft OneDrive (OAuth, auto-selected single drive).
    OneDrive,
    /// Google Drive (OAuth).
    GoogleDrive,
    /// Any rclone remote — user pastes a full INI stanza.
    CustomRclone,
}

impl DestinationKind {
    /// Returns `true` for providers that use the two-phase OAuth flow.
    fn is_oauth(self) -> bool {
        matches!(
            self,
            DestinationKind::OneDrive | DestinationKind::GoogleDrive
        )
    }
}

// ─── OAuth state machine ──────────────────────────────────────────────────────

/// State of an in-progress OAuth setup flow.
#[derive(Debug, Clone, PartialEq)]
enum OAuthFlowState {
    /// No OAuth in progress.
    Idle,
    /// `begin_*_setup` IPC call is in flight.
    Starting,
    /// rclone is running; user is completing the browser flow.
    WaitingForBrowser { setup_id: String, auth_url: String },
    /// OAuth completed; blob ready — `on_change` has been fired.
    Completed,
    /// OAuth failed.
    Failed { message: String },
}

// ─── Config helpers ────────────────────────────────────────────────────────────

/// Input fields for `build_config_for_kind`.
struct DestinationFields<'a> {
    local_path: &'a str,
    external_path: &'a str,
    b2_account_id: &'a str,
    b2_app_key: &'a str,
    b2_bucket: &'a str,
    b2_path_prefix: &'a str,
    custom_rclone_config: &'a str,
    oauth_blob: &'a str,
    oauth_path: &'a str,
}

/// Derives a `DestinationSessionConfig` for the given kind.
fn build_config_for_kind(
    kind: DestinationKind,
    f: DestinationFields<'_>,
) -> DestinationSessionConfig {
    match kind {
        DestinationKind::Local => DestinationSessionConfig {
            label: "Local Filesystem".to_string(),
            destination_type: "local_path".to_string(),
            provider: "local".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: f.local_path.to_string(),
            rclone_config_blob: String::new(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationKind::ExternalDrive => DestinationSessionConfig {
            label: "External Drive".to_string(),
            destination_type: "external_drive".to_string(),
            provider: "local".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: f.external_path.to_string(),
            rclone_config_blob: String::new(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationKind::BackblazeB2 => DestinationSessionConfig {
            label: "Backblaze B2".to_string(),
            destination_type: "cloud".to_string(),
            provider: "b2".to_string(),
            bucket: f.b2_bucket.to_string(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: f.b2_path_prefix.to_string(),
            rclone_config_blob: format!(
                "[backblaze_b2]\ntype = b2\naccount = {}\nkey = {}\n",
                f.b2_account_id, f.b2_app_key,
            ),
            is_primary: true,
            backup_mode: None,
        },
        DestinationKind::OneDrive => DestinationSessionConfig {
            label: "OneDrive".to_string(),
            destination_type: "cloud".to_string(),
            provider: "rclone".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: f.oauth_path.to_string(),
            rclone_config_blob: f.oauth_blob.to_string(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationKind::GoogleDrive => DestinationSessionConfig {
            label: "Google Drive".to_string(),
            destination_type: "cloud".to_string(),
            provider: "rclone".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: f.oauth_path.to_string(),
            rclone_config_blob: f.oauth_blob.to_string(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationKind::CustomRclone => {
            let name = f
                .custom_rclone_config
                .lines()
                .find_map(|line| {
                    let trimmed = line.trim();
                    trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
                })
                .unwrap_or("custom_rclone")
                .to_string();
            DestinationSessionConfig {
                label: name,
                destination_type: "cloud".to_string(),
                provider: "rclone".to_string(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: String::new(),
                rclone_config_blob: f.custom_rclone_config.to_string(),
                is_primary: true,
                backup_mode: None,
            }
        }
    }
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Destination selector — flat six-option radio group with conditional detail fields.
///
/// Calls `on_change` with an updated `DestinationSessionConfig` whenever the
/// user completes configuration.  For OAuth providers `on_change` fires only
/// after the browser callback is received.
///
/// Calls `on_kind_change` immediately whenever the user selects a new kind,
/// even before OAuth or field entry is complete.
#[component]
pub fn DestinationSelector(
    /// Called with the completed destination config when configuration is ready.
    on_change: impl Fn(DestinationSessionConfig) + 'static + Clone + Send + Sync,
    /// Called immediately when the user selects a different destination kind.
    #[prop(optional)]
    on_kind_change: Option<Box<dyn Fn(DestinationKind) + Send + Sync>>,
) -> impl IntoView {
    let (kind, set_kind) = signal(DestinationKind::Local);
    let (local_path, set_local_path) = signal(String::new());
    let (external_path, set_external_path) = signal(String::new());
    let (b2_account_id, set_b2_account_id) = signal(String::new());
    let (b2_app_key, set_b2_app_key) = signal(String::new());
    let (b2_bucket, set_b2_bucket) = signal(String::new());
    let (b2_path_prefix, set_b2_path_prefix) = signal(String::new());
    let (custom_rclone_config, set_custom_rclone_config) = signal(String::new());
    let (oauth_path, set_oauth_path) = signal(String::from("arx-runa"));
    let (oauth_state, set_oauth_state) = signal(OAuthFlowState::Idle);

    let on_change_notify = on_change.clone();
    let on_kind_change_sv = StoredValue::new(on_kind_change);

    // Fire `on_change` reactively for all non-OAuth providers.
    Effect::new(move |_| {
        let current_kind = kind.get();
        if current_kind.is_oauth() {
            return;
        }
        let config = build_config_for_kind(
            current_kind,
            DestinationFields {
                local_path: &local_path.get(),
                external_path: &external_path.get(),
                b2_account_id: &b2_account_id.get(),
                b2_app_key: &b2_app_key.get(),
                b2_bucket: &b2_bucket.get(),
                b2_path_prefix: &b2_path_prefix.get(),
                custom_rclone_config: &custom_rclone_config.get(),
                oauth_blob: "",
                oauth_path: "",
            },
        );
        on_change_notify(config);
    });

    let browse_local = move |_| {
        leptos::task::spawn_local(async move {
            if let Some(path) = open_directory_dialog().await {
                set_local_path.set(path);
            }
        });
    };

    let browse_external = move |_| {
        leptos::task::spawn_local(async move {
            if let Some(path) = open_directory_dialog().await {
                set_external_path.set(path);
            }
        });
    };

    // Cancel any in-progress OAuth when the user switches destination kind.
    let handle_kind_change = move |new_kind: DestinationKind| {
        let current_state = oauth_state.get_untracked();
        if let OAuthFlowState::WaitingForBrowser { setup_id, .. } = current_state {
            leptos::task::spawn_local(async move {
                let _ = invoke_command::<_, ()>(
                    "cancel_oauth_setup",
                    &CancelOauthSetupRequest { setup_id },
                )
                .await;
            });
        }
        set_oauth_state.set(OAuthFlowState::Idle);
        set_kind.set(new_kind);
        on_kind_change_sv.with_value(|cb| {
            if let Some(cb) = cb {
                cb(new_kind);
            }
        });
    };

    let begin_oauth = StoredValue::new({
        let on_change_clone = on_change.clone();
        move |provider_kind: DestinationKind| {
            let on_change_inner = on_change_clone.clone();
            let captured_oauth_path = oauth_path.get_untracked();
            set_oauth_state.set(OAuthFlowState::Starting);
            leptos::task::spawn_local(async move {
                let command = match provider_kind {
                    DestinationKind::GoogleDrive => "begin_google_drive_setup",
                    DestinationKind::OneDrive => "begin_onedrive_setup",
                    _ => return,
                };

                let begin_result = invoke_command::<_, BeginOauthSetupResponse>(command, &()).await;

                let response = match begin_result {
                    Ok(response) => response,
                    Err(error) => {
                        set_oauth_state.set(OAuthFlowState::Failed {
                            message: error.to_string(),
                        });
                        return;
                    }
                };

                let setup_id = response.setup_id.clone();
                let auth_url = response.auth_url.clone();
                set_oauth_state.set(OAuthFlowState::WaitingForBrowser {
                    setup_id: setup_id.clone(),
                    auth_url,
                });

                loop {
                    sleep(Duration::from_secs(2)).await;

                    if !matches!(
                        oauth_state.get_untracked(),
                        OAuthFlowState::WaitingForBrowser { .. }
                    ) {
                        break;
                    }

                    let poll_result = invoke_command::<_, OauthPollResponse>(
                        "poll_oauth_setup",
                        &PollOauthSetupRequest {
                            setup_id: setup_id.clone(),
                        },
                    )
                    .await;

                    match poll_result {
                        Err(error) => {
                            set_oauth_state.set(OAuthFlowState::Failed {
                                message: error.to_string(),
                            });
                            break;
                        }
                        Ok(OauthPollResponse::Pending) => {
                            continue;
                        }
                        Ok(OauthPollResponse::Failed { message }) => {
                            set_oauth_state.set(OAuthFlowState::Failed { message });
                            break;
                        }
                        Ok(OauthPollResponse::Completed { rclone_config_blob }) => {
                            set_oauth_state.set(OAuthFlowState::Completed);
                            let config = build_config_for_kind(
                                provider_kind,
                                DestinationFields {
                                    local_path: "",
                                    external_path: "",
                                    b2_account_id: "",
                                    b2_app_key: "",
                                    b2_bucket: "",
                                    b2_path_prefix: "",
                                    custom_rclone_config: "",
                                    oauth_blob: &rclone_config_blob,
                                    oauth_path: &captured_oauth_path,
                                },
                            );
                            on_change_inner(config);
                            break;
                        }
                    }
                }
            });
        }
    });

    let cancel_oauth = move |_| {
        let current_state = oauth_state.get_untracked();
        if let OAuthFlowState::WaitingForBrowser { setup_id, .. } = current_state {
            leptos::task::spawn_local(async move {
                let _ = invoke_command::<_, ()>(
                    "cancel_oauth_setup",
                    &CancelOauthSetupRequest { setup_id },
                )
                .await;
            });
        }
        set_oauth_state.set(OAuthFlowState::Idle);
    };

    let field_class = "w-full bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-rune";

    let render_option = move |option: DestinationKind,
                              label: &'static str,
                              desc: &'static str,
                              sharing_supported: bool| {
        let is_selected = move || kind.get() == option;
        view! {
            <label
                class="flex items-start gap-3 p-3 border rounded-lg cursor-pointer hover:bg-surface-overlay transition-colors"
                class=("border-rune", is_selected)
                class=("bg-surface-overlay", is_selected)
                class=("border-border-default", move || !is_selected())
            >
                <input
                    type="radio"
                    name="destination-kind"
                    checked=is_selected
                    on:change=move |_| handle_kind_change(option)
                    class="mt-0.5 cursor-pointer accent-rune"
                />
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2 flex-wrap">
                        <span class="font-medium text-bone text-sm">{label}</span>
                        {if sharing_supported {
                            view! {
                                <span class="text-xs px-1.5 py-0.5 rounded bg-green-900/40 text-green-400 border border-green-800/40 whitespace-nowrap">
                                    "Sharing Supported"
                                </span>
                            }.into_any()
                        } else {
                            view! {
                                <span class="text-xs px-1.5 py-0.5 rounded bg-surface-overlay text-text-muted border border-border-default whitespace-nowrap">
                                    "No sharing"
                                </span>
                            }.into_any()
                        }}
                    </div>
                    <div class="text-xs text-text-secondary mt-0.5">{desc}</div>
                </div>
            </label>
        }
    };

    view! {
        <div class="space-y-3">
            {render_option(DestinationKind::Local, "Local Filesystem",
                "Store vault data on this machine only — cannot be opened from other devices", false)}
            {render_option(DestinationKind::ExternalDrive, "External Drive",
                "USB drive or network share — drive must be physically connected to access the vault", false)}
            {render_option(DestinationKind::BackblazeB2, "Backblaze B2",
                "Object storage via Backblaze B2 — key ID and application key", true)}
            {render_option(DestinationKind::OneDrive, "OneDrive",
                "Personal Microsoft OneDrive — sign in with your Microsoft account", false)}
            {render_option(DestinationKind::GoogleDrive, "Google Drive",
                "Google Drive — sign in with your Google account", true)}
            {render_option(DestinationKind::CustomRclone, "Custom (Rclone)",
                "Any rclone-compatible remote — paste your rclone config stanza", false)}

            // ── Local path ────────────────────────────────────────────────────
            <Show when=move || kind.get() == DestinationKind::Local>
                <div class="mt-2 space-y-1">
                    <label class="text-xs text-text-secondary">
                        "Storage path (leave blank for app default)"
                    </label>
                    <div class="flex gap-2">
                        <input
                            type="text"
                            placeholder="Default app data directory"
                            value=move || local_path.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_local_path.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                        <button
                            type="button"
                            on:click=browse_local
                            class="px-3 py-2 rounded-lg border border-border-default text-bone text-sm cursor-pointer hover:bg-surface-overlay transition-colors whitespace-nowrap"
                        >
                            "Browse"
                        </button>
                    </div>
                </div>
            </Show>

            // ── External drive path ───────────────────────────────────────────
            <Show when=move || kind.get() == DestinationKind::ExternalDrive>
                <div class="mt-2 space-y-1">
                    <label class="text-xs text-text-secondary">"Drive path"</label>
                    <div class="flex gap-2">
                        <input
                            type="text"
                            placeholder="/mnt/usb or E:\\"
                            value=move || external_path.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_external_path.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                        <button
                            type="button"
                            on:click=browse_external
                            class="px-3 py-2 rounded-lg border border-border-default text-bone text-sm cursor-pointer hover:bg-surface-overlay transition-colors whitespace-nowrap"
                        >
                            "Browse"
                        </button>
                    </div>
                </div>
            </Show>

            // ── Backblaze B2 ─────────────────────────────────────────────────
            <Show when=move || kind.get() == DestinationKind::BackblazeB2>
                <div class="mt-2 space-y-3">
                    <div class="p-3 bg-surface-overlay border border-border-default rounded-lg">
                        <p class="text-xs text-text-secondary">
                            "Enter your Backblaze B2 application key credentials. "
                            "Find these in the B2 Console under "
                            <span class="text-rune">"Account → App Keys"</span>
                            "."
                        </p>
                    </div>
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Key ID"</label>
                        <input
                            type="text"
                            placeholder="123456789abcdef"
                            value=move || b2_account_id.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_b2_account_id.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Application Key"</label>
                        <input
                            type="password"
                            placeholder="K003..."
                            value=move || b2_app_key.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_b2_app_key.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Bucket"</label>
                        <input
                            type="text"
                            placeholder="my-arx-bucket"
                            value=move || b2_bucket.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_b2_bucket.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Path prefix (optional)"</label>
                        <input
                            type="text"
                            placeholder="arx-runa/vault"
                            value=move || b2_path_prefix.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_b2_path_prefix.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>
                </div>
            </Show>

            // ── OneDrive / Google Drive — shared OAuth flow UI ────────────────
            <Show when=move || kind.get().is_oauth()>
                {move || {
                    let current_kind = kind.get();
                    let (provider_name, scope_note) = match current_kind {
                        DestinationKind::OneDrive => (
                            "OneDrive",
                            "Connects to personal Microsoft OneDrive. \
                            OneDrive Business and SharePoint are not supported — \
                            use Custom (Rclone) for those.",
                        ),
                        _ => (
                            "Google Drive",
                            "Connects to your Google Drive. \
                            Shared drives and Workspace accounts may need Custom (Rclone).",
                        ),
                    };

                    match oauth_state.get() {
                        OAuthFlowState::Idle | OAuthFlowState::Failed { .. } => {
                            let error_msg = if let OAuthFlowState::Failed { message } = oauth_state.get() {
                                Some(message)
                            } else {
                                None
                            };
                            view! {
                                <div class="mt-2 space-y-3">
                                    <div class="p-3 bg-surface-overlay border border-border-default rounded-lg">
                                        <p class="text-xs text-text-secondary">{scope_note}</p>
                                    </div>
                                    <div class="space-y-1">
                                        <label class="text-xs text-text-secondary">"Folder name"</label>
                                        <input
                                            type="text"
                                            placeholder="arx-runa"
                                            prop:value=move || oauth_path.get()
                                            on:input=move |ev| {
                                                use leptos::prelude::event_target_value;
                                                set_oauth_path.set(event_target_value(&ev));
                                            }
                                            class=field_class
                                        />
                                        <p class="text-xs text-text-secondary">
                                            "Subfolder to create inside your cloud drive. Leave blank to use the drive root."
                                        </p>
                                    </div>
                                    {error_msg.map(|msg| view! {
                                        <div class="p-3 bg-surface-overlay border border-red-500 rounded-lg">
                                            <p class="text-xs text-red-400">{msg}</p>
                                        </div>
                                    })}
                                    <button
                                        type="button"
                                        on:click=move |_| begin_oauth.with_value(|f| f(current_kind))
                                        class="w-full px-4 py-2 bg-rune text-white text-sm font-medium rounded-lg hover:bg-rune/90 transition-colors cursor-pointer"
                                    >
                                        {format!("Connect {provider_name}")}
                                    </button>
                                </div>
                            }.into_any()
                        }
                        OAuthFlowState::Starting => view! {
                            <div class="mt-2 p-3 bg-surface-overlay border border-border-default rounded-lg flex items-center gap-2">
                                <div class="w-4 h-4 border-2 border-rune border-t-transparent rounded-full animate-spin" />
                                <span class="text-xs text-text-secondary">"Starting authorization…"</span>
                            </div>
                        }.into_any(),
                        OAuthFlowState::WaitingForBrowser { auth_url, .. } => view! {
                            <div class="mt-2 space-y-3">
                                <div class="p-3 bg-surface-overlay border border-border-default rounded-lg flex items-center gap-2">
                                    <div class="w-4 h-4 border-2 border-rune border-t-transparent rounded-full animate-spin" />
                                    <span class="text-xs text-text-secondary">
                                        "Waiting for browser authorization — complete sign-in then return here"
                                    </span>
                                </div>
                                <p class="text-xs text-text-secondary">
                                    "If your browser did not open, "
                                    <a
                                        href=auth_url
                                        target="_blank"
                                        class="text-rune underline hover:no-underline"
                                    >
                                        "click here to authorize"
                                    </a>
                                    "."
                                </p>
                                <button
                                    type="button"
                                    on:click=cancel_oauth
                                    class="w-full px-4 py-2 border border-border-default text-bone text-sm font-medium rounded-lg hover:bg-surface-overlay transition-colors cursor-pointer"
                                >
                                    "Cancel"
                                </button>
                            </div>
                        }.into_any(),
                        OAuthFlowState::Completed => view! {
                            <div class="mt-2 p-3 bg-surface-overlay border border-green-500 rounded-lg flex items-center gap-2">
                                <span class="text-green-400 text-lg">"✓"</span>
                                <span class="text-xs text-bone">
                                    {format!("{provider_name} connected successfully")}
                                </span>
                            </div>
                        }.into_any(),
                    }
                }}
            </Show>

            // ── Custom rclone ─────────────────────────────────────────────────
            <Show when=move || kind.get() == DestinationKind::CustomRclone>
                <div class="mt-2 space-y-1">
                    <label class="text-xs text-text-secondary">"Rclone config section"</label>
                    <textarea
                        placeholder="[myremote]\ntype = s3\n..."
                        prop:value=move || custom_rclone_config.get()
                        on:input=move |ev| {
                            use leptos::prelude::event_target_value;
                            set_custom_rclone_config.set(event_target_value(&ev));
                        }
                        class=format!("{field_class} h-32 font-mono resize-y")
                        rows="6"
                    />
                    <p class="text-xs text-text-secondary">
                        "Paste one complete rclone remote section. \
                        The \"[name]\" header sets the remote name. \
                        OneDrive Business and SharePoint users should use this option."
                    </p>
                </div>
            </Show>
        </div>
    }
}
