//! Storage destination selector — allows users to choose where their vault data is stored.
//!
//! Supports Local, S3, B2, Rclone, and Custom endpoints.

use crate::ipc_types::DestinationSessionConfig;
use leptos::prelude::*;

/// Storage provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProvider {
    Local,
    S3,
    B2,
    Rclone,
}

impl StorageProvider {
    pub fn label(&self) -> &'static str {
        match self {
            StorageProvider::Local => "Local Filesystem",
            StorageProvider::S3 => "Amazon S3",
            StorageProvider::B2 => "Backblaze B2",
            StorageProvider::Rclone => "Rclone",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            StorageProvider::Local => "Store vault on this computer's local filesystem",
            StorageProvider::S3 => "Store vault in Amazon S3 or S3-compatible storage",
            StorageProvider::B2 => "Store vault in Backblaze B2 object storage",
            StorageProvider::Rclone => "Store vault using rclone (any supported provider)",
        }
    }
}

/// Storage configuration for a specific provider.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub provider: StorageProvider,
    pub label: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
    pub rclone_config_blob: String,
}

impl StorageConfig {
    /// Create a Local storage config.
    pub fn local() -> Self {
        Self {
            provider: StorageProvider::Local,
            label: "Local".to_string(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: String::new(),
            rclone_config_blob: String::new(),
        }
    }

    /// Convert to DestinationSessionConfig for IPC.
    pub fn to_destination(&self) -> DestinationSessionConfig {
        DestinationSessionConfig {
            label: self.label.clone(),
            destination_type: match self.provider {
                StorageProvider::Local => "local_path".to_string(),
                StorageProvider::S3 => "cloud".to_string(),
                StorageProvider::B2 => "cloud".to_string(),
                StorageProvider::Rclone => "cloud".to_string(),
            },
            provider: match self.provider {
                StorageProvider::Local => "local".to_string(),
                StorageProvider::S3 => "aws".to_string(),
                StorageProvider::B2 => "b2".to_string(),
                StorageProvider::Rclone => "rclone".to_string(),
            },
            bucket: self.bucket.clone(),
            region: self.region.clone(),
            endpoint: self.endpoint.clone(),
            path_prefix: self.path_prefix.clone(),
            rclone_config_blob: self.rclone_config_blob.clone(),
            is_primary: true,
            backup_mode: None,
        }
    }
}

/// Storage selector component — radio buttons to choose provider and conditional detail fields.
#[component]
pub fn StorageSelector() -> impl IntoView {
    let (provider, set_provider) = signal(StorageProvider::Local);
    view! {
        <div class="space-y-4">
            <div class="text-sm text-text-secondary mb-3">
                "Choose where your vault data will be stored."
            </div>

            {
                let render_radio = move |provider_option: StorageProvider| {
                    let is_selected = move || provider.get() == provider_option;
                    let on_select = move |_| {
                        set_provider.set(provider_option);
                    };

                    view! {
                        <label class="flex items-start gap-3 p-3 border border-border-default rounded-lg cursor-pointer hover:bg-surface-overlay transition-colors"
                               class=("border-rune", is_selected)
                               class=("bg-surface-overlay", is_selected)>
                            <input
                                type="radio"
                                name="storage-provider"
                                checked=is_selected
                                on:change=on_select
                                class="mt-1 cursor-pointer"
                            />
                            <div class="flex-1">
                                <div class="font-medium text-bone">{provider_option.label()}</div>
                                <div class="text-sm text-text-secondary">{provider_option.description()}</div>
                            </div>
                        </label>
                    }
                };

                vec![
                    render_radio(StorageProvider::Local),
                    render_radio(StorageProvider::S3),
                    render_radio(StorageProvider::B2),
                    render_radio(StorageProvider::Rclone),
                ]
            }

            {
                move || {
                    match provider.get() {
                        StorageProvider::Local => {
                            view! {
                                <div class="mt-4 p-3 bg-surface-overlay rounded-lg border border-border-default">
                                    <div class="text-sm text-text-secondary">
                                        "Vault data will be stored in: "
                                        <code class="font-mono text-bone">"%APPDATA%\\arx-runa\\vaults"</code>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        StorageProvider::S3 => {
                            view! {
                                <div class="mt-4 p-3 bg-surface-overlay rounded-lg border border-border-default">
                                    <div class="text-sm text-text-secondary">
                                        "S3 configuration will be available in the next phase."
                                    </div>
                                </div>
                            }.into_any()
                        }
                        StorageProvider::B2 => {
                            view! {
                                <div class="mt-4 p-3 bg-surface-overlay rounded-lg border border-border-default">
                                    <div class="text-sm text-text-secondary">
                                        "Backblaze B2 configuration will be available in the next phase."
                                    </div>
                                </div>
                            }.into_any()
                        }
                        StorageProvider::Rclone => {
                            view! {
                                <div class="mt-4 p-3 bg-surface-overlay rounded-lg border border-border-default">
                                    <div class="text-sm text-text-secondary">
                                        "Rclone configuration will be available in the next phase."
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }
                }
            }
        </div>
    }
}
