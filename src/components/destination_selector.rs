//! Destination type selector for vault creation.
//!
//! Replaces the flat `StorageProvider` enum with a three-tier model that maps
//! cleanly to the backend's `DestinationType`: Local filesystem, External drive
//! (USB / network share), and Cloud (via Rclone).

use leptos::prelude::*;

use crate::dialog::open_directory_dialog;
use crate::ipc_types::DestinationSessionConfig;

/// Top-level destination type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationType {
    Local,
    ExternalDrive,
    Cloud,
}

/// Cloud sub-provider selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    S3,
    B2,
    GoogleDrive,
    Custom,
}

impl CloudProvider {
    fn destination_type_str(self) -> &'static str {
        match self {
            CloudProvider::S3 => "s3",
            CloudProvider::B2 => "b2",
            CloudProvider::GoogleDrive => "rclone",
            CloudProvider::Custom => "rclone",
        }
    }

    fn provider_str(self) -> &'static str {
        match self {
            CloudProvider::S3 => "aws",
            CloudProvider::B2 => "b2",
            CloudProvider::GoogleDrive => "google_drive",
            CloudProvider::Custom => "rclone",
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_config(
    dest_type: DestinationType,
    local_path: &str,
    external_path: &str,
    cloud_provider: CloudProvider,
    cloud_bucket: &str,
    cloud_region: &str,
    cloud_endpoint: &str,
    cloud_path_prefix: &str,
    s3_access_key_id: &str,
    s3_secret_access_key: &str,
    b2_account_id: &str,
    b2_app_key: &str,
    custom_rclone_config: &str,
) -> DestinationSessionConfig {
    match dest_type {
        DestinationType::Local => DestinationSessionConfig {
            label: "Local Filesystem".to_string(),
            destination_type: "local".to_string(),
            provider: "local".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: local_path.to_string(),
            rclone_config_blob: String::new(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationType::ExternalDrive => DestinationSessionConfig {
            label: "External Drive".to_string(),
            destination_type: "external_drive".to_string(),
            provider: "local".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: external_path.to_string(),
            rclone_config_blob: String::new(),
            is_primary: true,
            backup_mode: None,
        },
        DestinationType::Cloud => {
            let (label, rclone_config_blob) = match cloud_provider {
                CloudProvider::S3 => (
                    "Amazon S3".to_string(),
                    format!(
                        "[amazon_s3]\ntype = s3\nprovider = AWS\nregion = {cloud_region}\nendpoint = {cloud_endpoint}\naccess_key_id = {s3_access_key_id}\nsecret_access_key = {s3_secret_access_key}\n"
                    ),
                ),
                CloudProvider::B2 => (
                    "Backblaze B2".to_string(),
                    format!(
                        "[backblaze_b2]\ntype = b2\naccount = {b2_account_id}\nkey = {b2_app_key}\n"
                    ),
                ),
                CloudProvider::GoogleDrive => ("Google Drive".to_string(), String::new()),
                CloudProvider::Custom => {
                    let name = custom_rclone_config
                        .lines()
                        .find_map(|line| {
                            let t = line.trim();
                            t.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
                        })
                        .unwrap_or("custom_rclone")
                        .to_string();
                    (name, custom_rclone_config.to_string())
                }
            };
            DestinationSessionConfig {
                label,
                destination_type: cloud_provider.destination_type_str().to_string(),
                provider: cloud_provider.provider_str().to_string(),
                bucket: cloud_bucket.to_string(),
                region: cloud_region.to_string(),
                endpoint: cloud_endpoint.to_string(),
                path_prefix: cloud_path_prefix.to_string(),
                rclone_config_blob,
                is_primary: true,
                backup_mode: None,
            }
        }
    }
}

/// Destination selector — radio group for destination type with conditional detail fields.
///
/// Calls `on_change` with an updated `DestinationSessionConfig` whenever any field changes.
#[component]
pub fn DestinationSelector(
    /// Called with the new destination config whenever the selection changes.
    on_change: impl Fn(DestinationSessionConfig) + 'static + Clone,
) -> impl IntoView {
    let (dest_type, set_dest_type) = signal(DestinationType::Local);
    let (local_path, set_local_path) = signal(String::new());
    let (external_path, set_external_path) = signal(String::new());
    let (cloud_provider, set_cloud_provider) = signal(CloudProvider::S3);
    let (cloud_bucket, set_cloud_bucket) = signal(String::new());
    let (cloud_region, set_cloud_region) = signal(String::new());
    let (cloud_endpoint, set_cloud_endpoint) = signal(String::new());
    let (cloud_path_prefix, set_cloud_path_prefix) = signal(String::new());
    let (s3_access_key_id, set_s3_access_key_id) = signal(String::new());
    let (s3_secret_access_key, set_s3_secret_access_key) = signal(String::new());
    let (b2_account_id, set_b2_account_id) = signal(String::new());
    let (b2_app_key, set_b2_app_key) = signal(String::new());
    let (custom_rclone_config, set_custom_rclone_config) = signal(String::new());

    let notify = on_change.clone();
    Effect::new(move |_| {
        let config = build_config(
            dest_type.get(),
            &local_path.get(),
            &external_path.get(),
            cloud_provider.get(),
            &cloud_bucket.get(),
            &cloud_region.get(),
            &cloud_endpoint.get(),
            &cloud_path_prefix.get(),
            &s3_access_key_id.get(),
            &s3_secret_access_key.get(),
            &b2_account_id.get(),
            &b2_app_key.get(),
            &custom_rclone_config.get(),
        );
        notify(config);
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

    let field_class = "w-full bg-surface-overlay border border-border-default rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-rune";

    let render_type_radio = move |option: DestinationType,
                                  label: &'static str,
                                  desc: &'static str| {
        let is_selected = move || dest_type.get() == option;
        view! {
            <label
                class="flex items-start gap-3 p-3 border rounded-lg cursor-pointer hover:bg-surface-overlay transition-colors"
                class=("border-rune", is_selected)
                class=("bg-surface-overlay", is_selected)
                class=("border-border-default", move || !is_selected())
            >
                <input
                    type="radio"
                    name="destination-type"
                    checked=is_selected
                    on:change=move |_| set_dest_type.set(option)
                    class="mt-0.5 cursor-pointer accent-rune"
                />
                <div>
                    <div class="font-medium text-bone text-sm">{label}</div>
                    <div class="text-xs text-text-secondary mt-0.5">{desc}</div>
                </div>
            </label>
        }
    };

    view! {
        <div class="space-y-3">
            {render_type_radio(DestinationType::Local, "Local Filesystem", "Store vault data on this computer")}
            {render_type_radio(DestinationType::ExternalDrive, "External Drive", "USB drive or network share via path")}
            {render_type_radio(DestinationType::Cloud, "Cloud Storage", "Amazon S3, Backblaze B2, Google Drive, or any Rclone remote")}

            <Show when=move || dest_type.get() == DestinationType::Local>
                <div class="mt-2 space-y-1">
                    <label class="text-xs text-text-secondary">"Storage path (leave blank for app default)"</label>
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
                            class="px-3 py-2 rounded-lg border border-border-default text-bone text-sm hover:bg-surface-overlay transition-colors whitespace-nowrap"
                        >
                            "Browse"
                        </button>
                    </div>
                </div>
            </Show>

            <Show when=move || dest_type.get() == DestinationType::ExternalDrive>
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
                            class="px-3 py-2 rounded-lg border border-border-default text-bone text-sm hover:bg-surface-overlay transition-colors whitespace-nowrap"
                        >
                            "Browse"
                        </button>
                    </div>
                </div>
            </Show>

            <Show when=move || dest_type.get() == DestinationType::Cloud>
                <div class="mt-2 space-y-3">
                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Provider"</label>
                        <select
                            class=field_class
                            on:change=move |ev| {
                                use leptos::prelude::event_target_value;
                                let v = event_target_value(&ev);
                                let p = match v.as_str() {
                                    "s3" => CloudProvider::S3,
                                    "b2" => CloudProvider::B2,
                                    "google_drive" => CloudProvider::GoogleDrive,
                                    _ => CloudProvider::Custom,
                                };
                                set_cloud_provider.set(p);
                            }
                        >
                            <option value="s3">"Amazon S3"</option>
                            <option value="b2">"Backblaze B2"</option>
                            <option value="google_drive">"Google Drive"</option>
                            <option value="rclone">"Other (Rclone)"</option>
                        </select>
                    </div>

                    // Bucket — S3 and B2 only
                    <Show when=move || matches!(cloud_provider.get(), CloudProvider::S3 | CloudProvider::B2)>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Bucket"</label>
                            <input
                                type="text"
                                placeholder="my-bucket"
                                value=move || cloud_bucket.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_cloud_bucket.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>
                    </Show>

                    // S3: region, endpoint, credentials
                    <Show when=move || cloud_provider.get() == CloudProvider::S3>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Region"</label>
                            <input
                                type="text"
                                placeholder="us-east-1"
                                value=move || cloud_region.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_cloud_region.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Endpoint (optional, for S3-compatible providers)"</label>
                            <input
                                type="text"
                                placeholder="https://s3.example.com"
                                value=move || cloud_endpoint.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_cloud_endpoint.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Access Key ID"</label>
                            <input
                                type="password"
                                placeholder="AKIAIOSFODNN7EXAMPLE"
                                value=move || s3_access_key_id.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_s3_access_key_id.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Secret Access Key"</label>
                            <input
                                type="password"
                                placeholder="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
                                value=move || s3_secret_access_key.get()
                                on:input=move |ev| {
                                    use leptos::prelude::event_target_value;
                                    set_s3_secret_access_key.set(event_target_value(&ev));
                                }
                                class=field_class
                            />
                        </div>
                    </Show>

                    // B2: account ID and application key
                    <Show when=move || cloud_provider.get() == CloudProvider::B2>
                        <div class="space-y-1">
                            <label class="text-xs text-text-secondary">"Account ID"</label>
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
                    </Show>

                    // Custom: paste full rclone config section
                    <Show when=move || cloud_provider.get() == CloudProvider::Custom>
                        <div class="space-y-1">
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
                                "Paste one complete rclone remote section. The "[name]" header sets the remote name."
                            </p>
                        </div>
                    </Show>

                    // Google Drive: OAuth configured post-creation
                    <Show when=move || cloud_provider.get() == CloudProvider::GoogleDrive>
                        <div class="p-3 bg-surface-overlay border border-border-default rounded-lg">
                            <p class="text-xs text-text-secondary">
                                "Google Drive authorization requires additional setup. Configure credentials in "
                                <span class="text-rune">"Settings → Destinations"</span>
                                " after vault creation."
                            </p>
                        </div>
                    </Show>

                    <div class="space-y-1">
                        <label class="text-xs text-text-secondary">"Path prefix (optional)"</label>
                        <input
                            type="text"
                            placeholder="arx-runa/vault"
                            value=move || cloud_path_prefix.get()
                            on:input=move |ev| {
                                use leptos::prelude::event_target_value;
                                set_cloud_path_prefix.set(event_target_value(&ev));
                            }
                            class=field_class
                        />
                    </div>
                </div>
            </Show>
        </div>
    }
}
